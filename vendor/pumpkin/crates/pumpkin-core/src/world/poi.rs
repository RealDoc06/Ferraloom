//! Caller policy over Level-owned observations. Predicates and entity callbacks
//! run only after world storage and terrain guards have been released.

use std::sync::Weak;

use pumpkin_data::{Block, villager::VillagerProfession};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::poi::{PoiEntry, PoiQueryError};

use super::{World, villager_poi::JobSiteOwner};
use crate::entity::EntityBase;

pub enum JobSiteClaim {
    Unavailable,
    Rejected,
    Claimed,
}

pub fn distance_squared(origin: BlockPos, position: BlockPos) -> i128 {
    let delta = |a: i32, b: i32| i128::from(a) - i128::from(b);
    delta(origin.0.x, position.0.x).pow(2)
        + delta(origin.0.y, position.0.y).pow(2)
        + delta(origin.0.z, position.0.z).pow(2)
}

impl World {
    pub(crate) fn poi_in_sphere(
        &self,
        center: BlockPos,
        radius: i32,
    ) -> Result<Vec<PoiEntry>, PoiQueryError> {
        let mut entries = self.level.poi_snapshot_in_square(center, radius)?;
        entries.retain(|entry| distance_squared(center, entry.pos()) <= i128::from(radius).pow(2));
        entries.sort_by_key(|entry| distance_squared(center, entry.pos()));
        Ok(entries)
    }

    pub(crate) fn available_job_sites(
        &self,
        center: BlockPos,
        expected: Option<VillagerProfession>,
    ) -> Vec<BlockPos> {
        let owners = self
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .owners();
        // Health/EntityBase callbacks may enter the world. Never call them while
        // holding the transient owner mutex (including during a claim).
        let dead: Vec<_> = owners
            .into_iter()
            .filter(|(pos, owner)| {
                distance_squared(center, *pos) <= 48i128.pow(2) && !owner.is_alive()
            })
            .collect();
        let mut storage = self
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (position, owner) in dead {
            storage.clear_observed_owner(position, &owner);
        }
        storage.available_job_sites(center, 48, expected)
    }

    pub(crate) fn claim_job_site(
        &self,
        position: BlockPos,
        owner: Weak<dyn EntityBase>,
        expected: Option<VillagerProfession>,
    ) -> JobSiteClaim {
        let Some(owner) = JobSiteOwner::new(owner) else {
            return JobSiteClaim::Rejected;
        };
        let observed = self
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .owner_at(position);
        let dead = observed.filter(|owner| !owner.is_alive());
        if !self.is_in_build_limit(position) {
            return JobSiteClaim::Unavailable;
        }
        let (coords, relative) = position.chunk_and_chunk_relative_position();
        // The existing map entry pins this publication until the terrain and
        // transient claim update finish. No entity callback occurs in this window.
        let Some(chunk) = self.level.loaded_chunks.get(&coords) else {
            return JobSiteClaim::Unavailable;
        };
        let terrain = chunk.read_for_poi();
        let Ok(y) = usize::try_from(relative.y - chunk.section.min_y) else {
            return JobSiteClaim::Unavailable;
        };
        let Some(palette) = terrain.palettes().get(y / 16) else {
            return JobSiteClaim::Unavailable;
        };
        let block =
            Block::from_state_id(palette.get(relative.x as usize, y % 16, relative.z as usize));
        let claimed = self
            .villager_poi
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .claim(position, block, owner, dead.as_ref(), expected)
            .is_some();
        if claimed {
            JobSiteClaim::Claimed
        } else {
            JobSiteClaim::Rejected
        }
    }
}

#[cfg(test)]
#[path = "poi_tests.rs"]
mod tests;
