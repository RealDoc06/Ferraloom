//! Coalesced POI history. Every access holds the chunk's terrain guard first.

use std::sync::{Arc, MutexGuard, RwLockReadGuard};

use pumpkin_data::{BlockStateId, poi::PoiType};

use super::{ChunkData, palette::BlockPalette};
use crate::chunk_system::ChunkPos;
use crate::poi::{LoadedPoiRegion, PoiReconcileError, PoiSectionReplacement};

pub(super) struct PoiChanges {
    sections: Box<[PoiSectionChanges]>,
    // Once any section exhausts its revision, no chunk stamp can certify it.
    exhausted: bool,
    store: Option<Arc<()>>,
}

pub(crate) struct PoiSectionChanges {
    revision: u64,
    applied_revision: u64,
    // One bit per y/z/x palette index, allocated only after a type transition.
    reset_tickets: Option<Box<[u64; 64]>>,
}

impl PoiChanges {
    pub(super) fn new(count: usize) -> Self {
        Self {
            sections: (0..count)
                .map(|_| PoiSectionChanges {
                    revision: 1,
                    applied_revision: 0,
                    reset_tickets: None,
                })
                .collect(),
            exhausted: false,
            store: None,
        }
    }

    /// Called only with the actual old/new values from an exclusive palette write.
    pub(super) fn record(
        &mut self,
        section: usize,
        x: usize,
        y: usize,
        z: usize,
        old: BlockStateId,
        new: BlockStateId,
    ) {
        if old == new {
            return;
        }
        assert!(x < 16 && y < 16 && z < 16);
        let changes = &mut self.sections[section];
        // Keep recording reset history and allow terrain writes after exhaustion,
        // but never wrap or issue another successful preparation stamp.
        match changes.revision.checked_add(1) {
            Some(revision) => changes.revision = revision,
            None => self.exhausted = true,
        }
        if PoiType::from_state(old) != PoiType::from_state(new) {
            let index = (y * 16 + z) * 16 + x;
            let bits = changes
                .reset_tickets
                .get_or_insert_with(|| Box::new([0; 64]));
            bits[index / 64] |= 1 << (index % 64);
        }
    }
}

impl PoiSectionChanges {
    pub(crate) fn resets_tickets(&self, index: usize) -> bool {
        assert!(index < BlockPalette::VOLUME);
        self.reset_tickets
            .as_ref()
            .is_some_and(|bits| bits[index / 64] & (1 << (index % 64)) != 0)
    }
}

/// An in-memory preparation stamp, not a durable save receipt or publication ID.
/// Stage B must pair this with the exact live `ChunkData` Arc under its lifecycle fence.
#[derive(Debug, PartialEq, Eq)]
pub struct PoiReconciliationStamp {
    revisions: Box<[u64]>,
}

/// Holds the existing terrain read lock across classification, POI commit and
/// any subsequent terrain snapshot. No disk IO, callbacks or awaits belong here.
pub struct PoiTerrainRead<'a> {
    chunk: &'a ChunkData,
    palettes: RwLockReadGuard<'a, Box<[BlockPalette]>>,
}

/// An unpublished save plan, tied to its exact source and exclusive storage
/// borrow. Dropping it after encode failure changes neither POI nor reset history.
pub(crate) struct PoiSavePlan<'a> {
    source: &'a ChunkData,
    loaded: LoadedPoiRegion<'a>,
    revisions: Box<[u64]>,
    sections: Vec<(usize, PoiSectionReplacement)>,
    complete: bool,
}

impl PoiSavePlan<'_> {
    /// Call only after successful terrain encoding. A later terrain write refuses
    /// the whole plan before mutation; its complete ABA history remains pending.
    /// This brief read guard is released before the caller performs any POI IO.
    pub(crate) fn apply(mut self) -> Result<Option<PoiReconciliationStamp>, PoiReconcileError> {
        let terrain = self.source.read_for_poi();
        let mut changes = terrain.changes_for(&self.loaded)?;
        if self.revisions.len() != changes.sections.len()
            || self
                .revisions
                .iter()
                .zip(changes.sections.iter())
                .any(|(revision, section)| *revision != section.revision)
        {
            return Err(PoiReconcileError::StaleSaveCapture);
        }
        changes.store = Some(self.loaded.identity.clone());
        for (index, replacement) in self.sections {
            self.loaded.apply_section(replacement);
            if self.complete {
                let section = &mut changes.sections[index];
                section.applied_revision = section.revision;
                section.reset_tickets = None;
            }
        }
        Ok(self.complete.then_some(PoiReconciliationStamp {
            revisions: self.revisions,
        }))
    }
}

impl ChunkData {
    /// Preload under storage alone first. Then hold lifecycle/storage before
    /// taking this guard; keep that same storage mutex through reconciliation.
    /// Save integration must preserve the existing light/heightmap-before-terrain
    /// lock order and build from `palettes()`, never call `to_bytes()` under here.
    pub fn read_for_poi(&self) -> PoiTerrainRead<'_> {
        PoiTerrainRead {
            chunk: self,
            palettes: self
                .section
                .block_sections
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        }
    }
}

impl<'chunk> PoiTerrainRead<'chunk> {
    /// Stage B's terrain root builder must consume this exact palette snapshot.
    #[must_use]
    pub fn palettes(&self) -> &[BlockPalette] {
        &self.palettes
    }

    /// Replaces pending sections in already-loaded POI memory. Ticket lookup and
    /// replacement share the caller's exclusive storage borrow; no detached ticket
    /// snapshot or externally callable acknowledgment exists.
    pub fn reconcile(
        &self,
        loaded: &mut LoadedPoiRegion<'_>,
    ) -> Result<PoiReconciliationStamp, PoiReconcileError> {
        if self.chunk.status != pumpkin_data::chunk::ChunkStatus::Full {
            return Err(PoiReconcileError::IncompleteTerrain);
        }
        self.apply(loaded, true)?;
        let changes = self
            .chunk
            .section
            .poi_changes
            .lock()
            .map_err(|_| PoiReconcileError::MetadataPoisoned)?;
        Ok(PoiReconciliationStamp {
            revisions: changes.sections.iter().map(|s| s.revision).collect(),
        })
    }

    /// Capture without changing shared memory or acknowledging revisions. Proto
    /// plans remain Valid0 and keep reset history even after successful encoding.
    pub(crate) fn plan_for_save<'storage>(
        &self,
        loaded: LoadedPoiRegion<'storage>,
    ) -> Result<PoiSavePlan<'storage>, PoiReconcileError>
    where
        'chunk: 'storage,
    {
        let changes = self.changes_for(&loaded)?;
        let complete = self.chunk.status == pumpkin_data::chunk::ChunkStatus::Full;
        let mut sections = Vec::new();
        for (index, (palette, section)) in self
            .palettes
            .iter()
            .zip(changes.sections.iter())
            .enumerate()
        {
            if complete && section.revision == section.applied_revision {
                continue;
            }
            sections.push((
                index,
                loaded.plan_section(
                    self.chunk.section.min_y / 16 + index as i32,
                    palette,
                    section,
                    complete,
                )?,
            ));
        }
        Ok(PoiSavePlan {
            source: self.chunk,
            loaded,
            revisions: changes.sections.iter().map(|s| s.revision).collect(),
            sections,
            complete,
        })
    }

    fn changes_for(
        &self,
        loaded: &LoadedPoiRegion<'_>,
    ) -> Result<MutexGuard<'_, PoiChanges>, PoiReconcileError> {
        let chunk = self.chunk;
        if chunk.section.block_sections.is_poisoned() {
            return Err(PoiReconcileError::TerrainPoisoned);
        }
        loaded.check_chunk(ChunkPos::new(chunk.x, chunk.z))?;
        let min_y = chunk.section.min_y;
        let height = i32::try_from(self.palettes.len())
            .ok()
            .and_then(|count| count.checked_mul(16));
        if min_y % 16 != 0 || height.and_then(|h| min_y.checked_add(h)).is_none() {
            return Err(PoiReconcileError::InvalidCoordinates);
        }
        let changes = chunk
            .section
            .poi_changes
            .lock()
            .map_err(|_| PoiReconcileError::MetadataPoisoned)?;
        if changes.exhausted {
            return Err(PoiReconcileError::RevisionExhausted);
        }
        if changes.sections.len() != self.palettes.len() {
            return Err(PoiReconcileError::InvalidCoordinates);
        }
        if changes
            .store
            .as_ref()
            .is_some_and(|store| !Arc::ptr_eq(store, &loaded.identity))
        {
            return Err(PoiReconcileError::WrongStorage);
        }
        Ok(changes)
    }

    fn apply(
        &self,
        loaded: &mut LoadedPoiRegion<'_>,
        complete: bool,
    ) -> Result<(), PoiReconcileError> {
        let mut changes = self.changes_for(loaded)?;
        changes.store = Some(loaded.identity.clone());
        for (index, (palette, section)) in self
            .palettes
            .iter()
            .zip(changes.sections.iter_mut())
            .enumerate()
        {
            if complete && section.revision == section.applied_revision {
                continue;
            }
            loaded.reconcile_section(
                self.chunk.section.min_y / 16 + index as i32,
                palette,
                section,
                complete,
            )?;
            // A failed section retains both its revision and complete ABA mask.
            if complete {
                section.applied_revision = section.revision;
                section.reset_tickets = None;
            }
        }
        Ok(())
    }

    /// Call under the final publication fence. A later changed state invalidates
    /// the stamp even if its final POI type or block state equals the old one.
    #[must_use]
    pub fn is_current(&self, stamp: &PoiReconciliationStamp) -> bool {
        if self.chunk.section.block_sections.is_poisoned() {
            return false;
        }
        let Ok(changes) = self.chunk.section.poi_changes.lock() else {
            return false;
        };
        !changes.exhausted
            && stamp.revisions.len() == changes.sections.len()
            && stamp
                .revisions
                .iter()
                .zip(changes.sections.iter())
                .all(|(revision, section)| {
                    *revision == section.revision && *revision == section.applied_revision
                })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poi::{PoiRegion, PoiStorage};
    use pumpkin_data::Block;
    use pumpkin_util::math::position::BlockPos;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    // UNRUN: inject only the otherwise unreachable failure boundary, then use
    // actual chunk writers, loaded-region reconciliation and persisted records.
    #[test]
    fn failed_revision_and_poison_refuse_ack_while_actual_writers_continue() {
        enum Failure {
            Revision,
            Metadata,
            Terrain,
        }

        for failure in [Failure::Revision, Failure::Metadata, Failure::Terrain] {
            let directory = tempfile::tempdir().unwrap();
            let mut storage = PoiStorage::new(directory.path().to_owned());
            let coords = ChunkPos::new(0, 0);
            storage.preload_poi_region(coords).unwrap();
            let chunk = ChunkData::empty(0, 0);
            let pos = BlockPos::new(1, 0, 1);
            let home = Block::WHITE_BED
                .states
                .iter()
                .map(|state| state.id)
                .find(|state| PoiType::from_state(*state) == Some(PoiType::Home))
                .unwrap();
            chunk.set_block_absolute_y(1, 0, 1, home);
            {
                let mut loaded = storage.loaded_poi_region(coords).unwrap();
                chunk.read_for_poi().reconcile(&mut loaded).unwrap();
            };
            storage
                .add_with_free_tickets(pos, PoiType::Home.identifier(), 0)
                .unwrap();
            if matches!(failure, Failure::Revision) {
                let _terrain = chunk.read_for_poi();
                let mut changes = chunk.section.poi_changes.lock().unwrap();
                changes.sections[4].revision = u64::MAX;
                changes.sections[4].applied_revision = u64::MAX;
            }
            let prepared = {
                let mut loaded = storage.loaded_poi_region(coords).unwrap();
                let terrain = chunk.read_for_poi();
                let stamp = terrain.reconcile(&mut loaded).unwrap();
                assert!(terrain.is_current(&stamp));
                stamp
            };
            match failure {
                Failure::Revision => {}
                Failure::Metadata => {
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            let _terrain = chunk.read_for_poi();
                            let _changes = chunk.section.poi_changes.lock().unwrap();
                            panic!("injected metadata poison");
                        }))
                        .is_err()
                    );
                }
                Failure::Terrain => {
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            let _terrain = chunk.section.block_sections.write().unwrap();
                            panic!("injected terrain poison");
                        }))
                        .is_err()
                    );
                }
            }

            assert_eq!(chunk.set_block_absolute_y(1, 0, 1, BlockStateId::AIR), home);
            assert_eq!(
                chunk.set_blocks_batch([(1, 0, 1, home)]),
                vec![(1, 0, 1, BlockStateId::AIR)]
            );
            {
                let mut loaded = storage.loaded_poi_region(coords).unwrap();
                let terrain = chunk.read_for_poi();
                assert_eq!(terrain.palettes()[4].get(1, 0, 1), home);
                assert!(!terrain.is_current(&prepared));
                let error = terrain.reconcile(&mut loaded).unwrap_err();
                assert!(match failure {
                    Failure::Revision => matches!(error, PoiReconcileError::RevisionExhausted),
                    Failure::Metadata => matches!(error, PoiReconcileError::MetadataPoisoned),
                    Failure::Terrain => matches!(error, PoiReconcileError::TerrainPoisoned),
                });
                // No failed preparation clears the ABA mask or acknowledges a
                // later write. Recovered locks keep their native poison flags.
                let changes = chunk
                    .section
                    .poi_changes
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                assert_eq!(changes.sections[4].applied_revision, prepared.revisions[4]);
                assert!(changes.sections[4].resets_tickets(17));
                assert_eq!(changes.exhausted, matches!(failure, Failure::Revision));
                assert_eq!(
                    chunk.section.poi_changes.is_poisoned(),
                    matches!(failure, Failure::Metadata)
                );
                assert_eq!(
                    chunk.section.block_sections.is_poisoned(),
                    matches!(failure, Failure::Terrain)
                );
            };
            storage.save_all().unwrap();
            let region = PoiRegion::load(&directory.path().join("r.0.0.mca")).unwrap();
            let entries = region.get_all();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].pos(), pos);
            assert_eq!(entries[0].poi_type, PoiType::Home.identifier());
            assert_eq!(entries[0].free_tickets, 0);
        }
    }
}
