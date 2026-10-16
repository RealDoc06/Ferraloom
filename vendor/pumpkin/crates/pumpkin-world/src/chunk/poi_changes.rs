//! Coalesced POI history. Every access holds the chunk's terrain guard first.

use std::sync::RwLockReadGuard;

use pumpkin_data::{BlockStateId, poi::PoiType};

use super::{ChunkData, palette::BlockPalette};
use crate::chunk_system::ChunkPos;
use crate::poi::{LoadedPoiRegion, PoiReconcileError};

pub(super) struct PoiChanges {
    sections: Box<[PoiSectionChanges]>,
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
        // Never wrap a stamp into an earlier acknowledged revision. Exhaustion
        // poisons the metadata mutex, preventing any subsequent acknowledgment.
        changes.revision = changes
            .revision
            .checked_add(1)
            .expect("POI revision exhausted");
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

impl PoiTerrainRead<'_> {
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
        let chunk = self.chunk;
        loaded.check_chunk(ChunkPos::new(chunk.x, chunk.z))?;
        let min_y = chunk.section.min_y;
        let height = i32::try_from(self.palettes.len())
            .ok()
            .and_then(|count| count.checked_mul(16));
        if min_y % 16 != 0 || height.and_then(|h| min_y.checked_add(h)).is_none() {
            return Err(PoiReconcileError::InvalidCoordinates);
        }
        let mut changes = chunk.section.poi_changes.lock().unwrap();
        if changes.sections.len() != self.palettes.len() {
            return Err(PoiReconcileError::InvalidCoordinates);
        }
        for (index, (palette, section)) in self
            .palettes
            .iter()
            .zip(changes.sections.iter_mut())
            .enumerate()
        {
            if section.revision == section.applied_revision {
                continue;
            }
            loaded.reconcile_section(min_y / 16 + index as i32, palette, section)?;
            // A failed section retains both its revision and complete ABA mask.
            section.applied_revision = section.revision;
            section.reset_tickets = None;
        }
        Ok(PoiReconciliationStamp {
            revisions: changes.sections.iter().map(|s| s.revision).collect(),
        })
    }

    /// Call under the final publication fence. A later changed state invalidates
    /// the stamp even if its final POI type or block state equals the old one.
    #[must_use]
    pub fn is_current(&self, stamp: &PoiReconciliationStamp) -> bool {
        let changes = self.chunk.section.poi_changes.lock().unwrap();
        stamp.revisions.len() == changes.sections.len()
            && stamp
                .revisions
                .iter()
                .zip(changes.sections.iter())
                .all(|(revision, section)| {
                    *revision == section.revision && *revision == section.applied_revision
                })
    }
}
