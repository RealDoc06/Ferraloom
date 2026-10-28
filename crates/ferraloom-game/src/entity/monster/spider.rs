//! Spider metadata is inherited by CaveSpider without another accessor.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData};

/// Port of Spider#DATA_FLAGS_ID; bit 0 represents climbing.
pub const DATA_FLAGS_ID: EntityDataAccessor<i8> = EntityDataAccessor::new(16);

/// Port of Spider#defineSynchedData's own field.
pub fn define_synched_data(data: &mut SynchedEntityData) {
    data.define(DATA_FLAGS_ID, 0);
}

/// Port of Spider's canonical Monster parent. Climbing is synchronized, not saved.
#[derive(Debug)]
pub struct Spider {
    pub parent: super::Monster,
}
impl Spider {
    /// Port of Spider#Spider and its virtual createNavigation selection. Concrete
    /// registerGoals and WallClimber callbacks remain required AI boundaries.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        let mut mob =
            crate::entity::Mob::with_parent_and_metadata(parent, level.context(), |base, _| {
                define_synched_data(&mut base.entity_data);
                Ok(())
            })?;
        mob.ai.navigation =
            crate::entity::ai::Unported::new(crate::entity::ai::AiOperation::Navigation(
                crate::entity::ai::NavigationKind::WallClimber,
            ));
        Ok(Self {
            parent: super::Monster::with_parent(mob),
        })
    }
    /// Port of Spider#isClimbing.
    pub fn is_climbing(&self) -> bool {
        self.parent.as_living().entity_data.get(DATA_FLAGS_ID) & 1 != 0
    }
    /// Port of Spider#setClimbing. Other flag bits are retained.
    pub fn set_climbing(&mut self, climbing: bool) {
        let data = &mut self.parent.as_living_mut().entity_data;
        let flags = *data.get(DATA_FLAGS_ID);
        data.set(DATA_FLAGS_ID, if climbing { flags | 1 } else { flags & !1 });
    }
}

/// Port of CaveSpider's empty subclass constructor, with its actual type supplier.
#[derive(Debug)]
pub struct CaveSpider {
    pub parent: Spider,
}
impl CaveSpider {
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Ok(Self {
            parent: Spider::with_level(parent, level)?,
        })
    }
}
