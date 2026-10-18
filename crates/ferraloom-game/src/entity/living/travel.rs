//! LivingEntity travel arithmetic; world-dependent travel phases build on these.
use super::LivingEntity;
use ferraloom_util::{Vec3, mth};

/// Port of LivingEntity#computeModifiedFriction, retaining every float step.
pub fn compute_modified_friction(friction: f32, modifier: f32) -> f32 {
    mth::clamp_f32(1.0 - (1.0 - friction) * modifier, 0.0, 1.0)
}

/// Port of LivingEntity#getFrictionInfluencedSpeed with captured virtual speeds.
pub fn friction_influenced_speed(
    speed: f32,
    flying_speed: f32,
    on_ground: bool,
    block_friction: f32,
) -> f32 {
    if on_ground {
        // Native compares the float with a double literal: 0.6F is above it.
        if f64::from(block_friction) > 0.6 {
            speed * (0.216_000_02 / (block_friction * block_friction * block_friction))
        } else {
            speed
        }
    } else {
        flying_speed
    }
}

impl LivingEntity {
    /// Port of LivingEntity#getFluidFallingAdjustedMovement. The falling flag is
    /// captured before travel; it must not be inferred from this later velocity.
    pub fn fluid_falling_adjusted_movement(
        &self,
        base_gravity: f64,
        is_falling: bool,
        movement: Vec3,
    ) -> Vec3 {
        if base_gravity != 0.0 && !self.shared_flag(3) {
            let y = if is_falling
                && (movement.y - 0.005).abs() >= 0.003
                && (movement.y - base_gravity / 16.0).abs() < 0.003
            {
                -0.003
            } else {
                movement.y - base_gravity / 16.0
            };
            Vec3::new(movement.x, y, movement.z)
        } else {
            movement
        }
    }
}

#[cfg(test)]
mod tests;
