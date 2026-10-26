//! Mob parent bodies whose canonical inputs are supplied by their owning systems.
use crate::{
    entity::{EntityData, EntityRef, LivingEntity, Mob, PathfinderMob, synched_data},
    level::{Level, LevelContext, LevelError},
};
use ferraloom_util::Vec3;

impl Mob {
    /// Port of Mob's parent construction with no extra derived metadata.
    pub fn with_parent(parent: EntityData, context: &LevelContext) -> Result<Self, LevelError> {
        Self::with_parent_and_metadata(parent, context, |_, _| Ok(()))
    }
    /// Entity's constructor invokes the complete virtual defineSynchedData chain
    /// BEFORE LivingEntity initializes attributes/health and consumes yaw RNG.
    /// Define the leaf metadata here, parent first, using this same EntityData's
    /// random when native metadata initialization draws it. Do not construct
    /// living fields, copy/predraw RNG, or look up an unpublished EntityRef here.
    /// Mob's field construction and native registerGoals stage follow afterward.
    pub fn with_parent_and_metadata(
        mut parent: EntityData,
        context: &LevelContext,
        define: impl FnOnce(&mut EntityData, &LevelContext) -> Result<(), LevelError>,
    ) -> Result<Self, LevelError> {
        LivingEntity::define_synched_data(&mut parent);
        parent.entity_data.define(synched_data::MOB_FLAGS, 0);
        define(&mut parent, context)?;
        Ok(Self {
            parent: LivingEntity::with_prepared_parent(parent),
            target: None,
            ai: Default::default(),
            xp_reward: 0,
            pathfinding_malus: [None; super::ai::pathfinder::PathType::ALL.len()],
            can_pick_up_loot: false,
            persistence_required: false,
            drop_chances: Default::default(),
            leash_data: None,
            home_radius: -1,
            home_position: ferraloom_util::BlockPos::ZERO,
            loot_table: None,
            loot_table_seed: 0,
        })
    }
}
impl PathfinderMob {
    /// PathfinderMob's constructor adds no fields or draws after Mob.
    pub fn with_parent(parent: Mob) -> Self {
        Self { parent }
    }
}

/// Port of Mob#jumpInLiquid. The caller captures getNavigation().canFloat();
/// navigation owns that flag. Live mob dispatch waits for that canonical read.
pub fn jump_in_liquid(
    entity: EntityRef,
    can_float: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    if can_float {
        super::living::jump::jump_in_liquid(entity, level)
    } else {
        let entity = level.entity_mut::<EntityData>(entity)?;
        entity.set_delta_movement(entity.velocity + Vec3::new(0.0, 0.3, 0.0));
        Ok(())
    }
}

mod persistence;
mod spawn;
pub use persistence::{DropChances, LeashAttachment, LeashData};
pub(crate) use persistence::{block_pos, default_variant};
pub use spawn::{AgeableMobGroupData, finalize_spawn};

mod access;
pub(crate) use access::can_be_seen_as_enemy_parent;
pub use access::{
    CanonicalMobAccess, can_attack, controlled_vehicle, get_pathfinding_malus, get_target,
    get_target_unchecked, max_fall_distance, set_pathfinding_malus, set_target,
};

mod tick;
pub use tick::server_ai_step;

#[cfg(test)]
mod foundation_tests;
