//! Owned observations of the one Level store. No caller predicate runs here.

use std::sync::Arc;

use pumpkin_data::{BlockStateId, poi::PoiType};
use pumpkin_util::math::position::BlockPos;

use super::{PoiEntry, PoiReconcileError, PoiSectionReplacement, PoiStorage};
use crate::chunk::palette::BlockPalette;
use crate::{chunk::poi_binding::PoiRuntime, chunk_system::ChunkPos, level::SyncChunk};

#[derive(Debug, thiserror::Error)]
pub enum PoiQueryError {
    #[error(transparent)]
    Reconciliation(#[from] PoiReconcileError),
    #[error("POI area ownership changed during terrain admission; retry the query")]
    ChangedDuringQuery,
    #[error("POI query radius must be between 0 and 256")]
    InvalidRadius,
    #[error("POI terrain admission stopped or its caller was cancelled")]
    Closed,
    #[error("POI terrain admission queue is full; retry the query")]
    Busy,
    #[error("POI terrain is unavailable at {pos:?}: {reason}")]
    NeedsTerrain { pos: ChunkPos, reason: String },
}

#[derive(Clone, Copy)]
pub struct PoiArea {
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
}

impl PoiArea {
    pub fn new(center: BlockPos, radius: i32) -> Result<Self, PoiQueryError> {
        if !(0..=256).contains(&radius) {
            return Err(PoiQueryError::InvalidRadius);
        }
        let bounds = (
            center.0.x.checked_sub(radius),
            center.0.x.checked_add(radius),
            center.0.z.checked_sub(radius),
            center.0.z.checked_add(radius),
        );
        let (Some(min_x), Some(max_x), Some(min_z), Some(max_z)) = bounds else {
            return Err(PoiReconcileError::InvalidCoordinates.into());
        };
        Ok(Self {
            min_x,
            max_x,
            min_z,
            max_z,
        })
    }

    pub fn chunks(self) -> impl Iterator<Item = ChunkPos> {
        (self.min_x >> 4..=self.max_x >> 4).flat_map(move |x| {
            (self.min_z >> 4..=self.max_z >> 4).map(move |z| ChunkPos::new(x, z))
        })
    }

    fn contains(self, entry: &PoiEntry) -> bool {
        (self.min_x..=self.max_x).contains(&entry.x) && (self.min_z..=self.max_z).contains(&entry.z)
    }

    fn replace_section(self, entries: &mut Vec<PoiEntry>, replacement: PoiSectionReplacement) {
        let (x, _, z) = replacement.origin;
        entries.retain(|entry| {
            entry.x >> 4 != x >> 4
                || entry.z >> 4 != z >> 4
                || entry.y >> 4 != replacement.section_y
        });
        entries.extend(
            replacement
                .entries
                .into_iter()
                .filter(|entry| self.contains(entry)),
        );
    }
}

pub struct PoiAreaReadRequest {
    pub area: PoiArea,
    pub columns: Vec<ChunkPos>,
    pub min_section: i32,
    pub end_section: i32,
    pub reply: tokio::sync::oneshot::Sender<Result<Vec<PoiEntry>, PoiQueryError>>,
}

/// Private query evidence, never an acknowledgment or a persistent replacement.
pub struct PoiTerrainOverlay {
    pub pos: ChunkPos,
    pub sections: Vec<ClassifiedPoiSection>,
}

/// Pure occupancy from one visited section. Tickets and Valid are deliberately
/// absent: only the final common window may resolve them against the store.
pub struct ClassifiedPoiSection {
    pub section_y: i32,
    entries: Vec<(u16, PoiType)>,
}

impl ClassifiedPoiSection {
    fn from_states(section_y: i32, states: impl Iterator<Item = BlockStateId>) -> Self {
        Self {
            section_y,
            // Both callers supply exactly one 4096-state section in palette order.
            entries: states
                .enumerate()
                .filter_map(|(index, state)| {
                    PoiType::from_state(state).map(|kind| (index as u16, kind))
                })
                .collect(),
        }
    }

    #[must_use]
    pub const fn entries(&self) -> &[(u16, PoiType)] {
        self.entries.as_slice()
    }

    fn kinds(&self) -> impl Iterator<Item = Option<PoiType>> + '_ {
        let mut entries = self.entries().iter().peekable();
        (0..BlockPalette::VOLUME).map(move |index| {
            if entries
                .peek()
                .is_some_and(|(at, _)| usize::from(*at) == index)
            {
                entries.next().map(|(_, kind)| *kind)
            } else {
                None
            }
        })
    }
}

impl PoiTerrainOverlay {
    pub fn classify_resident(proto: &crate::ProtoChunk) -> Self {
        let sections = proto
            .poi_read_sections()
            .iter()
            .filter_map(|y| {
                let relative_y = y * 16 - i32::from(proto.bottom_y());
                if relative_y < 0 || relative_y + 16 > i32::from(proto.height()) {
                    return None;
                }
                Some(ClassifiedPoiSection::from_states(
                    *y,
                    (0..BlockPalette::VOLUME).map(|index| {
                        proto.get_block_state_raw(
                            (index % 16) as i32,
                            relative_y + (index / 256) as i32,
                            (index / 16 % 16) as i32,
                        )
                    }),
                ))
            })
            .collect();
        Self {
            pos: ChunkPos::new(proto.x, proto.z),
            sections,
        }
    }

    pub fn classify_decoded(chunk: &crate::chunk::ChunkData) -> Result<Self, PoiQueryError> {
        let terrain = chunk.read_for_poi();
        if chunk.section.block_sections.is_poisoned() {
            return Err(PoiReconcileError::TerrainPoisoned.into());
        }
        let sections = terrain
            .palettes()
            .iter()
            .enumerate()
            .filter(|(index, _)| chunk.section.poi_read_sections[*index])
            .map(|(index, palette)| {
                ClassifiedPoiSection::from_states(
                    chunk.section.min_y / 16 + index as i32,
                    palette.iter(),
                )
            })
            .collect();
        Ok(Self {
            pos: ChunkPos::new(chunk.x, chunk.z),
            sections,
        })
    }
}

impl PoiStorage {
    pub(crate) fn columns_needing_terrain(
        &mut self,
        area: PoiArea,
        min_section: i32,
        end_section: i32,
    ) -> Result<Vec<ChunkPos>, PoiQueryError> {
        let mut columns = Vec::new();
        for pos in area.chunks() {
            let loaded = self.loaded_poi_region(pos)?;
            if (min_section..end_section)
                .any(|y| loaded.region.section_validity.get(&(pos.x, pos.y, y)) != Some(&1))
            {
                columns.push(pos);
            }
        }
        Ok(columns)
    }

    fn overlay_area(
        &mut self,
        area: PoiArea,
        overlays: &[PoiTerrainOverlay],
    ) -> Result<Vec<PoiEntry>, PoiQueryError> {
        let mut entries = self.copy_area(area)?;
        for overlay in overlays {
            let loaded = self.loaded_poi_region(overlay.pos)?;
            for section in &overlay.sections {
                if loaded.region.section_validity.get(&(
                    overlay.pos.x,
                    overlay.pos.y,
                    section.section_y,
                )) == Some(&1)
                {
                    continue;
                }
                let replacement =
                    loaded.plan_section_kinds(section.section_y, section.kinds(), None, false)?;
                area.replace_section(&mut entries, replacement);
            }
        }
        Ok(entries)
    }

    /// Disk phase, before any publication or terrain guard is acquired.
    pub(crate) fn preload_area(&mut self, area: PoiArea) -> Result<(), PoiQueryError> {
        for x in area.min_x >> 9..=area.max_x >> 9 {
            for z in area.min_z >> 9..=area.max_z >> 9 {
                self.get_or_load_region(x, z)
                    .map_err(PoiReconcileError::RegionUnavailable)?;
            }
        }
        Ok(())
    }

    /// Already-loaded memory only. Missing sections and Valid0 records retain
    /// their decoded meaning; cache observation does not certify terrain.
    fn copy_area(&mut self, area: PoiArea) -> Result<Vec<PoiEntry>, PoiQueryError> {
        let mut entries = Vec::new();
        for x in area.min_x >> 9..=area.max_x >> 9 {
            for z in area.min_z >> 9..=area.max_z >> 9 {
                let loaded = self.loaded_poi_region(ChunkPos::new(x << 5, z << 5))?;
                entries.extend(
                    loaded
                        .region
                        .entries
                        .values()
                        .filter(|entry| area.contains(entry))
                        .cloned(),
                );
            }
        }
        Ok(entries)
    }
}

impl PoiRuntime {
    pub fn snapshot(self: &Arc<Self>, area: PoiArea) -> Result<Vec<PoiEntry>, PoiQueryError> {
        self.storage
            .lock()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?
            .preload_area(area)?;
        self.capture_area(area)
    }

    pub fn capture_area(self: &Arc<Self>, area: PoiArea) -> Result<Vec<PoiEntry>, PoiQueryError> {
        self.capture_area_with_overlays(area, &[], &[])
    }

    /// Scheduler pins private Proto ownership and cold read reservations through
    /// this call. Published FULL sources are pinned by lifecycle + terrain here.
    pub fn capture_area_with_overlays(
        self: &Arc<Self>,
        area: PoiArea,
        overlays: &[PoiTerrainOverlay],
        expected: &[(ChunkPos, SyncChunk)],
    ) -> Result<Vec<PoiEntry>, PoiQueryError> {
        let _lifecycle = self
            .lifecycle
            .read()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let publications = self
            .chunks
            .upgrade()
            .ok_or(PoiReconcileError::ObsoletePublication)?;
        // Iteration is coordinate sorted. The lifecycle fence pins every exact
        // Arc (including absence) through the complete storage/terrain window.
        let chunks: Vec<_> = area
            .chunks()
            .filter_map(|pos| publications.get(&pos).map(|entry| entry.value().clone()))
            .collect();
        if overlays
            .iter()
            .any(|overlay| publications.contains_key(&overlay.pos))
        {
            return Err(PoiQueryError::ChangedDuringQuery);
        }
        if expected.iter().any(|(pos, chunk)| {
            publications
                .get(pos)
                .is_none_or(|current| !Arc::ptr_eq(current.value(), chunk))
        }) {
            return Err(PoiQueryError::ChangedDuringQuery);
        }
        for chunk in &chunks {
            if !Arc::ptr_eq(chunk.poi_runtime()?, self) {
                return Err(PoiReconcileError::WrongStorage.into());
            }
        }
        let mut storage = self
            .storage
            .lock()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let terrain: Vec<_> = chunks.iter().map(|chunk| chunk.read_for_poi()).collect();
        let mut entries = storage.overlay_area(area, overlays)?;
        for (chunk, read) in chunks.iter().zip(&terrain) {
            let loaded = storage.loaded_poi_region(ChunkPos::new(chunk.x, chunk.z))?;
            for replacement in read.query_replacements(&loaded)? {
                area.replace_section(&mut entries, replacement);
            }
        }
        Ok(entries)
    }
}
