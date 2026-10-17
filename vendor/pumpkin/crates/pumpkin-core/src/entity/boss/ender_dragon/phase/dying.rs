use super::EnderDragonPhase;
use crate::entity::boss::ender_dragon::{EnderDragonEntity, Vector3Ext};
use pumpkin_util::math::vector3::Vector3;
use std::sync::{Mutex, atomic::Ordering};

#[derive(Default)]
struct DyingState {
    time: i32,
    target: Option<Vector3<f64>>,
}

/// Fly-to-death phase state, separate from the owner's 200-tick death sequence.
#[derive(Default)]
pub struct DyingPhase {
    state: Mutex<DyingState>,
}

impl super::Phase for DyingPhase {
    fn get_type(&self) -> EnderDragonPhase {
        EnderDragonPhase::Dying
    }

    fn reset_on_entry(&self, _dragon: &EnderDragonEntity) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = DyingState::default();
    }

    fn get_fly_speed(&self) -> f32 {
        3.0
    }

    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .target
    }

    fn tick(&self, dragon: &EnderDragonEntity) {
        let entity = &dragon.mob_entity.living_entity.entity;
        let target = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.time = state.time.wrapping_add(1);
            *state.target.get_or_insert_with(|| {
                let origin = *dragon
                    .fight_origin
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                entity.world.load().dragon_death_target(origin)
            })
        };
        let distance = entity.pos.load().distance_squared(target);
        let still_flying = (100.0..=22500.0).contains(&distance)
            && !entity.horizontal_collision.load(Ordering::Relaxed)
            && !entity.vertical_collision.load(Ordering::Relaxed);
        dragon
            .mob_entity
            .living_entity
            .set_health(if still_flying { 1.0 } else { 0.0 });
        // The outer flight loop consumes this phase's target and speed once.
        // Server Level.addParticle is a no-op; client visuals follow phase metadata.
    }
}
