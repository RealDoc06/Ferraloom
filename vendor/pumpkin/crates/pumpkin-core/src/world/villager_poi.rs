use std::{collections::HashMap, sync::Weak};

use pumpkin_data::{Block, villager::VillagerProfession};
use pumpkin_util::math::position::BlockPos;

use crate::entity::EntityBase;

/// Transient ownership only. Entity callbacks are evaluated on an owned copy,
/// outside the storage mutex; mutation compares the same Weak and UUID again.
#[derive(Clone)]
pub(crate) struct JobSiteOwner {
    id: uuid::Uuid,
    entity: Weak<dyn EntityBase>,
}

impl JobSiteOwner {
    pub(crate) fn new(entity: Weak<dyn EntityBase>) -> Option<Self> {
        let id = entity.upgrade()?.get_entity().entity_uuid;
        Some(Self { id, entity })
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.entity.upgrade().is_some_and(|entity| {
            entity
                .get_living_entity()
                .is_none_or(|living| living.health.load() > 0.0)
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.id == other.id && self.entity.ptr_eq(&other.entity)
    }
}

struct JobSite {
    profession: VillagerProfession,
    owner: Option<JobSiteOwner>,
}

#[derive(Default)]
pub struct VillagerPoiStorage {
    job_sites: HashMap<BlockPos, JobSite>,
}

impl VillagerPoiStorage {
    pub(crate) fn owners(&self) -> Vec<(BlockPos, JobSiteOwner)> {
        self.job_sites
            .iter()
            .filter_map(|(position, site)| site.owner.clone().map(|owner| (*position, owner)))
            .collect()
    }

    pub(crate) fn owner_at(&self, position: BlockPos) -> Option<JobSiteOwner> {
        self.job_sites.get(&position)?.owner.clone()
    }

    pub(crate) fn clear_observed_owner(&mut self, position: BlockPos, observed: &JobSiteOwner) {
        if let Some(site) = self.job_sites.get_mut(&position)
            && site
                .owner
                .as_ref()
                .is_some_and(|owner| owner.same(observed))
        {
            site.owner = None;
        }
    }

    pub fn update_block(&mut self, position: BlockPos, block: &Block) {
        let Some(profession) = profession_for_block(block) else {
            self.job_sites.remove(&position);
            return;
        };
        let site = self.job_sites.entry(position).or_insert(JobSite {
            profession,
            owner: None,
        });
        if site.profession != profession {
            *site = JobSite {
                profession,
                owner: None,
            };
        }
    }

    pub(crate) fn claim(
        &mut self,
        position: BlockPos,
        block: &Block,
        owner: JobSiteOwner,
        observed_dead: Option<&JobSiteOwner>,
        expected_profession: Option<VillagerProfession>,
    ) -> Option<VillagerProfession> {
        self.update_block(position, block);
        let site = self.job_sites.get_mut(&position)?;
        if expected_profession.is_some_and(|profession| profession != site.profession) {
            return None;
        }
        if let Some(current_owner) = site.owner.as_ref()
            && current_owner.id != owner.id
            && !observed_dead.is_some_and(|observed| current_owner.same(observed))
        {
            return None;
        }
        site.owner = Some(owner);
        Some(site.profession)
    }

    pub fn release(&mut self, position: BlockPos, owner: uuid::Uuid) {
        let Some(site) = self.job_sites.get_mut(&position) else {
            return;
        };
        if site
            .owner
            .as_ref()
            .is_none_or(|current| current.entity.strong_count() == 0 || current.id == owner)
        {
            site.owner = None;
        }
    }

    #[must_use]
    pub fn available_job_sites(
        &self,
        origin: BlockPos,
        radius: i32,
        expected_profession: Option<VillagerProfession>,
    ) -> Vec<BlockPos> {
        let radius_squared = i64::from(radius).pow(2);
        let mut sites = self
            .job_sites
            .iter()
            .filter(|(_, site)| {
                expected_profession.is_none_or(|profession| profession == site.profession)
                    && site
                        .owner
                        .as_ref()
                        .is_none_or(|owner| owner.entity.strong_count() == 0)
            })
            .filter_map(|(position, _)| {
                let delta = position.0 - origin.0;
                let distance_squared = i64::from(delta.x).pow(2)
                    + i64::from(delta.y).pow(2)
                    + i64::from(delta.z).pow(2);
                (distance_squared <= radius_squared).then_some((distance_squared, *position))
            })
            .collect::<Vec<_>>();
        sites.sort_unstable_by_key(|(distance, _)| *distance);
        sites.into_iter().map(|(_, position)| position).collect()
    }

    #[must_use]
    pub fn get_nearest_job_site(&self, origin: BlockPos, radius: i32) -> Option<BlockPos> {
        let radius_squared = i64::from(radius).pow(2);
        let mut closest = None;
        let mut closest_dist = radius_squared;

        for position in self.job_sites.keys() {
            let delta = position.0 - origin.0;
            let distance_squared =
                i64::from(delta.x).pow(2) + i64::from(delta.y).pow(2) + i64::from(delta.z).pow(2);
            if distance_squared <= closest_dist {
                closest = Some(*position);
                closest_dist = distance_squared;
            }
        }
        closest
    }
}

#[must_use]
pub fn profession_for_block(block: &Block) -> Option<VillagerProfession> {
    match block {
        block if block == &Block::COMPOSTER => Some(VillagerProfession::Farmer),
        block if block == &Block::LECTERN => Some(VillagerProfession::Librarian),
        block if block == &Block::BLAST_FURNACE => Some(VillagerProfession::Armorer),
        block if block == &Block::SMOKER => Some(VillagerProfession::Butcher),
        block if block == &Block::CARTOGRAPHY_TABLE => Some(VillagerProfession::Cartographer),
        block if block == &Block::BREWING_STAND => Some(VillagerProfession::Cleric),
        block if block == &Block::BARREL => Some(VillagerProfession::Fisherman),
        block if block == &Block::FLETCHING_TABLE => Some(VillagerProfession::Fletcher),
        block
            if [
                &Block::CAULDRON,
                &Block::WATER_CAULDRON,
                &Block::LAVA_CAULDRON,
                &Block::POWDER_SNOW_CAULDRON,
            ]
            .contains(&block) =>
        {
            Some(VillagerProfession::Leatherworker)
        }
        block if block == &Block::STONECUTTER => Some(VillagerProfession::Mason),
        block if block == &Block::LOOM => Some(VillagerProfession::Shepherd),
        block if block == &Block::SMITHING_TABLE => Some(VillagerProfession::Toolsmith),
        block if block == &Block::GRINDSTONE => Some(VillagerProfession::Weaponsmith),
        _ => None,
    }
}

#[must_use]
pub fn poi_type_for_block(block: &Block) -> Option<&'static str> {
    Some(match profession_for_block(block)? {
        VillagerProfession::Armorer => "minecraft:armorer",
        VillagerProfession::Butcher => "minecraft:butcher",
        VillagerProfession::Cartographer => "minecraft:cartographer",
        VillagerProfession::Cleric => "minecraft:cleric",
        VillagerProfession::Farmer => "minecraft:farmer",
        VillagerProfession::Fisherman => "minecraft:fisherman",
        VillagerProfession::Fletcher => "minecraft:fletcher",
        VillagerProfession::Leatherworker => "minecraft:leatherworker",
        VillagerProfession::Librarian => "minecraft:librarian",
        VillagerProfession::Mason => "minecraft:mason",
        VillagerProfession::Shepherd => "minecraft:shepherd",
        VillagerProfession::Toolsmith => "minecraft:toolsmith",
        VillagerProfession::Weaponsmith => "minecraft:weaponsmith",
        VillagerProfession::None | VillagerProfession::Nitwit => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_villager_workstation_maps_to_its_profession() {
        let workstations = [
            (&Block::COMPOSTER, VillagerProfession::Farmer),
            (&Block::LECTERN, VillagerProfession::Librarian),
            (&Block::BLAST_FURNACE, VillagerProfession::Armorer),
            (&Block::SMOKER, VillagerProfession::Butcher),
            (&Block::CARTOGRAPHY_TABLE, VillagerProfession::Cartographer),
            (&Block::BREWING_STAND, VillagerProfession::Cleric),
            (&Block::BARREL, VillagerProfession::Fisherman),
            (&Block::FLETCHING_TABLE, VillagerProfession::Fletcher),
            (&Block::CAULDRON, VillagerProfession::Leatherworker),
            (&Block::STONECUTTER, VillagerProfession::Mason),
            (&Block::LOOM, VillagerProfession::Shepherd),
            (&Block::SMITHING_TABLE, VillagerProfession::Toolsmith),
            (&Block::GRINDSTONE, VillagerProfession::Weaponsmith),
        ];

        for (block, profession) in workstations {
            assert_eq!(profession_for_block(block), Some(profession));
            assert_eq!(
                poi_type_for_block(block).unwrap(),
                format!(
                    "minecraft:{}",
                    profession.translation_key().rsplit('.').next().unwrap()
                )
            );
        }
        assert_eq!(profession_for_block(&Block::DIRT), None);
        assert_eq!(poi_type_for_block(&Block::DIRT), None);
    }

    #[test]
    fn available_job_sites_respect_profession_and_vanilla_search_radius() {
        let origin = BlockPos::new(0, 64, 0);
        let lectern = BlockPos::new(48, 64, 0);
        let too_far = BlockPos::new(49, 64, 0);
        let composter = BlockPos::new(1, 64, 0);
        let mut storage = VillagerPoiStorage::default();

        storage.update_block(lectern, &Block::LECTERN);
        storage.update_block(too_far, &Block::LECTERN);
        storage.update_block(composter, &Block::COMPOSTER);

        assert_eq!(
            storage.available_job_sites(origin, 48, Some(VillagerProfession::Librarian)),
            vec![lectern]
        );
        assert_eq!(
            storage.available_job_sites(origin, 48, Some(VillagerProfession::Farmer)),
            vec![composter]
        );
    }
}
