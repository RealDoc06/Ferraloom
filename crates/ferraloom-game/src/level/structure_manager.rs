//! Live structure data. Published immutable versions remain readable through
//! nested acquisition; only an exclusive phase boundary reclaims old versions.
use super::{Level, LevelError};
use elsa::FrozenVec;
use ferraloom_util::ChunkPos;
use ferraloom_worldgen::structure::pipeline::ChunkStructures;

pub(super) struct StructureVersion {
    pub revision: u64,
    pub payload: ChunkStructures,
}
pub(super) struct StructureVersions(FrozenVec<Box<StructureVersion>>);
impl StructureVersions {
    pub fn new(pos: ChunkPos) -> Self {
        let versions = Self(FrozenVec::new());
        versions.0.push(Box::new(StructureVersion {
            revision: 0,
            payload: ChunkStructures::new([pos.x, pos.z]),
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
        versions.push(Box::new(StructureVersion { revision, payload }));
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
        self.0
            .push(Box::new(StructureVersion { revision, payload }));
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
        for chunk in self.region_mut().chunks.values_mut() {
            chunk.collect_structure_versions();
        }
    }
}

#[cfg(test)]
mod carrier_tests {
    use super::*;
    use crate::level::{LevelContext, Region, chunk::LevelChunk};
    use ferraloom_worldgen::structure::start::StructureStart;
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    #[test]
    fn publication_is_newer_only_dirty_and_keeps_existing_readers_stable() {
        let pos = ChunkPos::new(-2, 3);
        let id = "minecraft:mineshaft".parse().unwrap();
        let mut payload = ChunkStructures::new([-2, 3]);
        payload.starts.insert(id, StructureStart::invalid());
        let mut chunk = LevelChunk::new(pos, 0, 1);
        chunk.set_structures(payload, 7).unwrap();
        assert!(chunk.try_mark_saved());
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        level.add_chunk(chunk).unwrap();
        let old = level.chunk(pos).unwrap().structures();
        assert_eq!(old.starts.len(), 1);
        for revision in 8..40 {
            assert!(
                level
                    .publish_chunk_structures(pos, revision, ChunkStructures::new([-2, 3]))
                    .unwrap()
            );
        }
        let chunk = level.chunk(pos).unwrap();
        assert!(chunk.structures().starts.is_empty());
        assert_eq!(chunk.structures_revision(), 39);
        assert_eq!(old.starts.len(), 1);
        assert!(chunk.is_unsaved());
        assert!(level.try_mark_chunk_saved(pos).unwrap());
        assert!(
            !level
                .publish_chunk_structures(pos, 39, ChunkStructures::new([-2, 3]))
                .unwrap()
        );
        assert!(
            !level
                .publish_chunk_structures(pos, 1, ChunkStructures::new([-2, 3]))
                .unwrap()
        );
        assert!(!level.chunk(pos).unwrap().is_unsaved());
        assert_eq!(
            level.publish_chunk_structures(pos, 40, ChunkStructures::new([0, 0])),
            Err(LevelError::InvalidChunkStructures(pos))
        );
        assert_eq!(
            level.publish_chunk_structures(ChunkPos::new(9, 9), 1, ChunkStructures::new([9, 9])),
            Err(LevelError::MissingChunk(ChunkPos::new(9, 9)))
        );
        level.collect_chunk_structure_versions();
        assert_eq!(level.chunk(pos).unwrap().structures.0.len(), 1);
    }
}
