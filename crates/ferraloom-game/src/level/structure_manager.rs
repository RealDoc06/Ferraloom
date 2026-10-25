//! Live structure data. Published immutable versions remain readable through
//! nested acquisition; only an exclusive phase boundary reclaims old versions.
use super::{
    Level, LevelError,
    acquisition::{AcquireFailure, ChunkKind},
    chunk_holder::ChunkStatus,
};
use elsa::FrozenVec;
use ferraloom_util::ChunkPos;
use ferraloom_worldgen::structure::pipeline::ChunkStructures;
use std::{ops::Deref, sync::Arc};

pub(super) struct StructureVersion {
    pub revision: u64,
    pub payload: Arc<ChunkStructures>,
}
pub(super) struct StructureVersions(FrozenVec<Box<StructureVersion>>);
impl StructureVersions {
    pub fn new(pos: ChunkPos) -> Self {
        let versions = Self(FrozenVec::new());
        versions.0.push(Box::new(StructureVersion {
            revision: 0,
            payload: Arc::new(ChunkStructures::new([pos.x, pos.z])),
        }));
        versions
    }
    pub fn current(&self) -> &StructureVersion {
        self.0.last().expect("a chunk always has structure data")
    }
    pub fn replace(
        &mut self,
        pos: ChunkPos,
        payload: ChunkStructures,
        revision: u64,
    ) -> Result<(), LevelError> {
        validate(pos, &payload)?;
        let versions = FrozenVec::new();
        versions.push(Box::new(StructureVersion {
            revision,
            payload: Arc::new(payload),
        }));
        self.0 = versions;
        Ok(())
    }
    pub fn publish(
        &self,
        pos: ChunkPos,
        payload: ChunkStructures,
        revision: u64,
    ) -> Result<bool, LevelError> {
        validate(pos, &payload)?;
        if revision <= self.current().revision {
            return Ok(false);
        }
        self.0.push(Box::new(StructureVersion {
            revision,
            payload: Arc::new(payload),
        }));
        Ok(true)
    }
    pub fn collect(&mut self) {
        if self.0.len() > 1 {
            let latest = std::mem::take(&mut self.0).into_vec().pop().unwrap();
            self.0.push(latest);
        }
    }
}
fn validate(pos: ChunkPos, payload: &ChunkStructures) -> Result<(), LevelError> {
    if payload.position == [pos.x, pos.z] {
        Ok(())
    } else {
        Err(LevelError::InvalidChunkStructures(pos))
    }
}
impl Level {
    /// StructureManager obtains the native status, including proto or staged
    /// imposter data. A lower-status read never requires FULL finalization.
    pub fn chunk_structures(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<Arc<ChunkStructures>, LevelError> {
        let reference = self
            .get_chunk(pos, status, true)?
            .ok_or(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::MissingResult,
            ))?;
        let structures = match reference.kind {
            ChunkKind::Full => Ok(Arc::clone(
                &self
                    .resolve_full_chunk(reference)?
                    .structures
                    .current()
                    .payload,
            )),
            ChunkKind::Proto => self
                .acquire_driver()
                .ok_or(LevelError::ChunkAcquisition(
                    pos,
                    status,
                    AcquireFailure::DriverUnavailable,
                ))?
                .proto_chunk_structures(reference),
        }?;
        validate(pos, &structures)?;
        Ok(structures)
    }
    /// Publish a newer source revision without invalidating references already
    /// returned to a suspended reader. Equal/older revisions have no effects.
    pub fn publish_chunk_structures(
        &self,
        pos: ChunkPos,
        revision: u64,
        payload: ChunkStructures,
    ) -> Result<bool, LevelError> {
        self.chunk(pos)
            .ok_or(LevelError::MissingChunk(pos))?
            .publish_structures(payload, revision)
    }
    /// Call after a batch of shared publications at an exclusive phase boundary.
    /// No structure references can survive this mutable borrow. Saves also collect.
    pub fn collect_chunk_structure_versions(&mut self) {
        for (_, chunk) in self.region_mut().chunks.iter_mut() {
            chunk.collect_structure_versions();
        }
    }
}

use ferraloom_protocol::codec::HolderSet;
use ferraloom_util::BlockPos;
use ferraloom_worldgen::{
    Identifier,
    structure::{references::StructureReferences, start::StructureStart},
};
use std::collections::BTreeMap;

/// Port of StructureManager. References belong to the queried chunk; starts
/// are read from their source chunks, not inferred from geometry or a seed.
#[derive(Clone, Copy)]
pub struct StructureManager<'a> {
    level: &'a Level,
}
impl Level {
    pub fn structure_manager(&self) -> StructureManager<'_> {
        StructureManager { level: self }
    }
}
/// A start held through its immutable source version. New publications cannot
/// invalidate a start while another source chunk is acquired synchronously.
#[derive(Clone, Debug)]
pub struct StructureStartRef {
    source: Arc<ChunkStructures>,
    structure: Identifier,
}
impl Deref for StructureStartRef {
    type Target = StructureStart;
    fn deref(&self) -> &Self::Target {
        &self.source.starts[&self.structure]
    }
}
impl StructureManager<'_> {
    fn data(&self, pos: ChunkPos, status: ChunkStatus) -> Result<Arc<ChunkStructures>, LevelError> {
        self.level.chunk_structures(pos, status)
    }
    pub fn get_start_for_structure(
        &self,
        pos: ChunkPos,
        structure: &Identifier,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        let source = self.data(pos, ChunkStatus::StructureStarts)?;
        Ok(source
            .starts
            .contains_key(structure)
            .then(|| StructureStartRef {
                source,
                structure: structure.clone(),
            }))
    }
    pub fn get_references_for_structure(
        &self,
        pos: ChunkPos,
        structure: &Identifier,
    ) -> Result<Option<StructureReferences>, LevelError> {
        Ok(self
            .data(pos, ChunkStatus::StructureReferences)?
            .references
            .get(structure)
            .cloned())
    }
    /// Port of getAllStructuresAt; this is the reference map, not local starts.
    pub fn get_all_structures_at(
        &self,
        pos: BlockPos,
    ) -> Result<BTreeMap<Identifier, StructureReferences>, LevelError> {
        Ok(self
            .data(ChunkPos::containing(pos), ChunkStatus::StructureReferences)?
            .references
            .clone())
    }
    pub fn has_any_structure_at(&self, pos: BlockPos) -> Result<bool, LevelError> {
        Ok(!self.get_all_structures_at(pos)?.is_empty())
    }
    /// Port of fillStartsForStructure. A missing/invalid start is omitted, but
    /// acquisition failure of its source chunk remains a required error.
    pub fn fill_starts_for_structure(
        &self,
        structure: &Identifier,
        references: &StructureReferences,
        mut consumer: impl FnMut(StructureStartRef),
    ) -> Result<(), LevelError> {
        for packed in references.iter() {
            if let Some(start) =
                self.get_start_for_structure(ChunkPos::unpack(packed), structure)?
                && start.is_valid()
            {
                consumer(start);
            }
        }
        Ok(())
    }
    pub fn starts_for_structure(
        &self,
        pos: ChunkPos,
        structure: &Identifier,
    ) -> Result<Vec<StructureStartRef>, LevelError> {
        let mut starts = Vec::new();
        if let Some(references) = self.get_references_for_structure(pos, structure)? {
            self.fill_starts_for_structure(structure, &references, |start| starts.push(start))?;
        }
        Ok(starts)
    }
    /// Port of the matcher overload; value-defined LongOpenHashSet order is
    /// preserved within each structure's reference set.
    pub fn starts_for_structure_matching(
        &self,
        pos: ChunkPos,
        mut matcher: impl FnMut(&Identifier) -> bool,
    ) -> Result<Vec<StructureStartRef>, LevelError> {
        let mut starts = Vec::new();
        // JVM identity-keyed map order is non-parity; use deterministic Identifier order.
        for (structure, references) in &self.data(pos, ChunkStatus::StructureReferences)?.references
        {
            if matcher(structure) {
                self.fill_starts_for_structure(structure, references, |start| starts.push(start))?;
            }
        }
        Ok(starts)
    }
    fn resolve(&self, set: &HolderSet) -> Result<Vec<Identifier>, LevelError> {
        let services = &self.level.context().structures;
        match set {
            HolderSet::Direct(ids) => ids
                .iter()
                .map(|id| {
                    let id: Identifier = id
                        .as_str()
                        .parse()
                        .map_err(|_| LevelError::InvalidStructureHolderSet)?;
                    if services.registry.structures.contains_key(&id) {
                        Ok(id)
                    } else {
                        Err(LevelError::InvalidStructureHolderSet)
                    }
                })
                .collect(),
            HolderSet::Named(tag) => {
                let tag = tag
                    .as_str()
                    .parse()
                    .map_err(|_| LevelError::InvalidStructureHolderSet)?;
                services
                    .tags
                    .get(&tag)
                    .cloned()
                    .ok_or(LevelError::InvalidStructureHolderSet)
            }
        }
    }
    pub fn starts_for_structures(
        &self,
        pos: ChunkPos,
        structures: &HolderSet,
    ) -> Result<Vec<StructureStartRef>, LevelError> {
        let ids = self.resolve(structures)?;
        self.starts_for_structure_matching(pos, |id| ids.contains(id))
    }
    pub fn get_structure_at(
        &self,
        pos: BlockPos,
        structure: &Identifier,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        Self::containing(
            pos,
            self.starts_for_structure(ChunkPos::containing(pos), structure)?,
        )
    }
    pub fn get_structure_at_in(
        &self,
        pos: BlockPos,
        structures: &HolderSet,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        Self::containing(
            pos,
            self.starts_for_structures(ChunkPos::containing(pos), structures)?,
        )
    }
    fn containing(
        pos: BlockPos,
        starts: Vec<StructureStartRef>,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        for start in starts {
            if start
                .bounding_box()
                .map_err(|_| LevelError::InvalidChunkStructures(ChunkPos::containing(pos)))?
                .contains([pos.x, pos.y, pos.z])
            {
                return Ok(Some(start));
            }
        }
        Ok(None)
    }
    pub fn get_structure_with_piece_at(
        &self,
        pos: BlockPos,
        structure: &Identifier,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        Ok(self
            .starts_for_structure(ChunkPos::containing(pos), structure)?
            .into_iter()
            .find(|start| Self::structure_has_piece_at(pos, start)))
    }
    pub fn get_structure_with_piece_at_in(
        &self,
        pos: BlockPos,
        structures: &HolderSet,
    ) -> Result<Option<StructureStartRef>, LevelError> {
        Ok(self
            .starts_for_structures(ChunkPos::containing(pos), structures)?
            .into_iter()
            .find(|start| Self::structure_has_piece_at(pos, start)))
    }
    /// Port of structureHasPieceAt; bounds are inclusive and terrain adaptation
    /// expands only the aggregate start box, never these individual piece boxes.
    pub fn structure_has_piece_at(pos: BlockPos, start: &StructureStart) -> bool {
        start
            .pieces
            .iter()
            .any(|piece| piece.bounds().contains([pos.x, pos.y, pos.z]))
    }
}

#[cfg(test)]
#[path = "structure_manager_tests.rs"]
mod tests;
