//! Owned observations of the one Level store. No caller predicate runs here.

use std::sync::Arc;

use pumpkin_util::math::position::BlockPos;

use super::{PoiEntry, PoiReconcileError, PoiStorage};
use crate::{chunk::poi_binding::PoiRuntime, chunk_system::ChunkPos, level::SyncChunk};

#[derive(Debug, thiserror::Error)]
pub enum PoiQueryError {
    #[error(transparent)]
    Reconciliation(#[from] PoiReconcileError),
    #[error("POI area changed during its bounded preparation pass; retry the query")]
    ChangedDuringQuery,
    #[error("POI query radius must be between 0 and 256")]
    InvalidRadius,
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
}

pub enum AreaCapture {
    Ready(Vec<PoiEntry>),
    NeedsPreparation(Vec<SyncChunk>),
}

impl PoiStorage {
    /// Disk phase, before any publication or terrain guard is acquired.
    pub fn preload_area(&mut self, area: PoiArea) -> Result<(), PoiQueryError> {
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
    pub fn snapshot(
        self: &Arc<Self>,
        area: PoiArea,
    ) -> Result<Vec<PoiEntry>, PoiQueryError> {
        self.storage
            .lock()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?
            .preload_area(area)?;
        match self.capture_area(area)? {
            AreaCapture::Ready(entries) => Ok(entries),
            AreaCapture::NeedsPreparation(chunks) => {
                // No area guard survives here. This is B's encoder barrier,
                // never the public memory-only Stage A reconciliation API.
                for chunk in chunks {
                    chunk.prepare_poi().map_err(query_preparation_error)?;
                }
                match self.capture_area(area)? {
                    AreaCapture::Ready(entries) => Ok(entries),
                    AreaCapture::NeedsPreparation(_) => Err(PoiQueryError::ChangedDuringQuery),
                }
            }
        }
    }

    pub fn capture_area(
        self: &Arc<Self>,
        area: PoiArea,
    ) -> Result<AreaCapture, PoiQueryError> {
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
        let mut pending = Vec::new();
        for (chunk, read) in chunks.iter().zip(&terrain) {
            let loaded = storage.loaded_poi_region(ChunkPos::new(chunk.x, chunk.z))?;
            if !read.is_reconciled(&loaded)? {
                pending.push(chunk.clone());
            }
        }
        if !pending.is_empty() {
            return Ok(AreaCapture::NeedsPreparation(pending));
        }
        Ok(AreaCapture::Ready(storage.copy_area(area)?))
    }
}

fn query_preparation_error(error: PoiReconcileError) -> PoiQueryError {
    match error {
        PoiReconcileError::StaleSaveCapture | PoiReconcileError::ObsoletePublication => {
            PoiQueryError::ChangedDuringQuery
        }
        error => error.into(),
    }
}
