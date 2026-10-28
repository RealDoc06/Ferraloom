use crate::entity::{LivingEntity, Mob, PathfinderMob};

/// Port of Monster's canonical PathfinderMob parent and constructor XP reward.
#[derive(Debug)]
pub struct Monster {
    pub parent: PathfinderMob,
}
impl Monster {
    /// Port of Monster#Monster. Mob owns the sole XP reward and AI state.
    pub fn with_parent(mut parent: Mob) -> Self {
        parent.xp_reward = 5;
        Self {
            parent: PathfinderMob::with_parent(parent),
        }
    }
    pub fn as_mob(&self) -> &Mob {
        &self.parent.parent
    }
    pub fn as_mob_mut(&mut self) -> &mut Mob {
        &mut self.parent.parent
    }
    pub fn as_living(&self) -> &LivingEntity {
        &self.as_mob().parent
    }
    pub fn as_living_mut(&mut self) -> &mut LivingEntity {
        &mut self.as_mob_mut().parent
    }
}

pub mod cubemob;
pub mod spider;
pub mod zombie;

#[cfg(test)]
mod tests;

pub mod piglin {
    pub mod ai {
        use crate::{
            entity::{EntityRef, player::ServerPlayer},
            level::{Level, LevelError},
        };
        use ferraloom_data::EntityType;

        /// Port of PiglinAi#angerNearbyPiglins. The empty class query succeeds
        /// natively; idle/visibility/anger-target behavior needs the Piglin port.
        pub fn anger_nearby_piglins(
            level: &mut Level,
            player: EntityRef,
            _only_if_they_see_the_player: bool,
        ) -> Result<(), LevelError> {
            let bounds = level
                .entity::<ServerPlayer>(player)?
                .parent
                .parent
                .bounding_box
                .inflate_uniform(16.0);
            // PiglinBrute is a distinct class and must never enter this query.
            let nearby =
                level.entities_in(bounds, |_, state| state.entity_type() == EntityType::PIGLIN);
            if nearby.is_empty() {
                Ok(())
            } else {
                Err(LevelError::UnsupportedEntity(EntityType::PIGLIN))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::entity::{EntityState, player::GameType};
            use ferraloom_util::{Uuid, Vec3};

            #[test]
            fn empty_piglin_query_has_no_effect_for_either_visibility_mode() {
                let mut level = crate::entity::tests::level(false);
                let player = level.next_entity_ref();
                let state = ServerPlayer::new(
                    Uuid::new(0, 1),
                    "guarded".into(),
                    level.next_entity_id().unwrap(),
                    Vec3::new(8.0, 64.0, 8.0),
                    GameType::Survival,
                );
                level
                    .add_entity(player, EntityState::Player(Box::new(state)))
                    .unwrap();
                level.take_effects();
                for visible in [false, true] {
                    anger_nearby_piglins(&mut level, player, visible).unwrap();
                    assert!(level.take_effects().is_empty());
                }
                // There is no canonical Piglin state/factory yet; do not
                // fabricate another subclass to exercise the nonempty branch.
            }
        }
    }
}
