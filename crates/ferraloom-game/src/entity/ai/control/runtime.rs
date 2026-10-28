//! One controller algorithm for live short borrows and explicit native-vector services.
use super::{Operation, rotate_towards, rotlerp};
use crate::entity::Mob;
use ferraloom_util::{Vec3, mth};

/// Port of the Mob services used by the base controls. A live implementation
/// reborrows Mob.ai between virtual calls; it never removes a controller.
pub trait ControlRuntime {
    type Error;
    fn mob(&self) -> Result<&Mob, Self::Error>;
    fn mob_mut(&mut self) -> Result<&mut Mob, Self::Error>;
    fn movement_speed(&self) -> Result<f64, Self::Error>;
    fn max_up_step(&self) -> Result<f32, Self::Error>;
    fn is_walkable(&mut self, dx: f32, dz: f32) -> Result<bool, Self::Error>;
    fn collision_height(&mut self) -> Result<Option<f64>, Self::Error>;
    fn collision_is_door_or_fence(&self) -> Result<bool, Self::Error>;
    fn is_in_liquid(&self) -> Result<bool, Self::Error>;
    fn is_affected_by_fluids(&self) -> Result<bool, Self::Error>;
    fn navigation_is_done(&self) -> Result<bool, Self::Error>;
    fn max_head_y_rot(&self) -> Result<i32, Self::Error>;
    fn carrying_mob_passenger(&self) -> Result<bool, Self::Error>;
    fn set_speed(&mut self, speed: f32) -> Result<(), Self::Error>;
    fn set_zza(&mut self, value: f32) -> Result<(), Self::Error>;
    fn set_xxa(&mut self, value: f32) -> Result<(), Self::Error>;
    fn set_jumping(&mut self, jumping: bool) -> Result<(), Self::Error>;
}

/// Port of MoveControl#tick, preserving writes around synchronous Mob callbacks.
pub fn tick_move<R: ControlRuntime>(r: &mut R) -> Result<(), R::Error> {
    match r.mob()?.ai.move_control.operation {
        Operation::Strafe => {
            let speed = r.movement_speed()? as f32;
            let control = &r.mob()?.ai.move_control;
            let modified = control.speed_modifier as f32 * speed;
            let (mut xa, mut za) = (control.strafe_forwards, control.strafe_right);
            let mut distance = (xa * xa + za * za).sqrt();
            if distance < 1.0 {
                distance = 1.0;
            }
            distance = modified / distance;
            xa *= distance;
            za *= distance;
            let yaw = r.mob()?.parent.parent.yaw;
            let sin = mth::sin(f64::from(yaw * 0.017453292));
            let cos = mth::cos(f64::from(yaw * 0.017453292));
            if !r.is_walkable(xa * cos - za * sin, za * cos + xa * sin)? {
                let c = &mut r.mob_mut()?.ai.move_control;
                c.strafe_forwards = 1.0;
                c.strafe_right = 0.0;
            }
            r.set_speed(modified)?;
            r.set_zza(r.mob()?.ai.move_control.strafe_forwards)?;
            r.set_xxa(r.mob()?.ai.move_control.strafe_right)?;
            r.mob_mut()?.ai.move_control.operation = Operation::Wait;
        }
        Operation::MoveTo => {
            r.mob_mut()?.ai.move_control.operation = Operation::Wait;
            let mob = r.mob()?;
            let delta = mob.ai.move_control.wanted - mob.parent.parent.position;
            if delta.x * delta.x + delta.y * delta.y + delta.z * delta.z < 2.500000277905201E-7 {
                return r.set_zza(0.0);
            }
            let desired = (mth::atan2(delta.z, delta.x) * 57.2957763671875) as f32 - 90.0;
            let yaw = rotlerp(r.mob()?.parent.parent.yaw, desired, 90.0);
            r.mob_mut()?.parent.parent.set_y_rot(yaw);
            let speed = (r.mob()?.ai.move_control.speed_modifier * r.movement_speed()?) as f32;
            r.set_speed(speed)?;
            // Native obtains the block and its collision shape even if the
            // height/distance arm of the following condition will match.
            let collision_height = r.collision_height()?;
            let mob = r.mob()?;
            if (delta.y > f64::from(r.max_up_step()?)
                && delta.x * delta.x + delta.z * delta.z
                    < f64::from(ferraloom_util::java::max_f32(
                        1.0,
                        mob.parent.parent.dimensions.width,
                    )))
                || (collision_height.is_some_and(|height| mob.parent.parent.position.y < height)
                    && !r.collision_is_door_or_fence()?)
            {
                r.mob_mut()?.ai.jump_control.jump();
                r.mob_mut()?.ai.move_control.operation = Operation::Jumping;
            }
        }
        Operation::Jumping => {
            let speed = (r.mob()?.ai.move_control.speed_modifier * r.movement_speed()?) as f32;
            r.set_speed(speed)?;
            if r.mob()?.parent.parent.on_ground
                || (r.is_in_liquid()? && r.is_affected_by_fluids()?)
            {
                r.mob_mut()?.ai.move_control.operation = Operation::Wait;
            }
        }
        Operation::Wait => r.set_zza(0.0)?,
    }
    Ok(())
}

/// Port of LookControl#tick and clampHeadRotationToBody's base implementation.
pub fn tick_look<R: ControlRuntime>(r: &mut R) -> Result<(), R::Error> {
    r.mob_mut()?.parent.parent.set_x_rot(0.0);
    if r.mob()?.ai.look_control.look_at_cooldown > 0 {
        r.mob_mut()?.ai.look_control.look_at_cooldown -= 1;
        let mob = r.mob()?;
        if let Some(desired) = mob.ai.look_control.y_rot_d(mob.parent.parent.position) {
            let head = rotate_towards(
                mob.parent.y_head_rot,
                desired,
                mob.ai.look_control.y_max_rot_speed,
            );
            r.mob_mut()?.parent.y_head_rot = head;
        }
        let mob = r.mob()?;
        let base = &mob.parent.parent;
        let eye = Vec3::new(
            base.position.x,
            base.position.y + f64::from(base.eye_height),
            base.position.z,
        );
        if let Some(desired) = mob.ai.look_control.x_rot_d(eye) {
            let pitch = rotate_towards(base.pitch, desired, mob.ai.look_control.x_max_rot_angle);
            r.mob_mut()?.parent.parent.set_x_rot(pitch);
        }
    } else {
        let mob = r.mob()?;
        let head = rotate_towards(mob.parent.y_head_rot, mob.parent.y_body_rot, 10.0);
        r.mob_mut()?.parent.y_head_rot = head;
    }
    if !r.navigation_is_done()? {
        let max = r.max_head_y_rot()? as f32;
        let mob = r.mob()?;
        let head = mth::rotate_if_necessary(mob.parent.y_head_rot, mob.parent.y_body_rot, max);
        r.mob_mut()?.parent.y_head_rot = head;
    }
    Ok(())
}

/// Port of JumpControl#tick. Clearing follows the virtual setter, including reentry.
pub fn tick_jump<R: ControlRuntime>(r: &mut R) -> Result<(), R::Error> {
    r.set_jumping(r.mob()?.ai.jump_control.jump)?;
    r.mob_mut()?.ai.jump_control.jump = false;
    Ok(())
}

/// Port of BodyRotationControl#clientTick, including its unwrapped head delta.
pub fn tick_body<R: ControlRuntime>(r: &mut R) -> Result<(), R::Error> {
    let mob = r.mob()?;
    let base = &mob.parent.parent;
    let dx = base.position.x - base.old_position.x;
    let dz = base.position.z - base.old_position.z;
    if dx * dx + dz * dz > 2.500000277905201E-7 {
        let yaw = base.yaw;
        r.mob_mut()?.parent.y_body_rot = yaw;
        let max = r.max_head_y_rot()? as f32;
        let mob = r.mob()?;
        let head = mth::rotate_if_necessary(mob.parent.y_head_rot, mob.parent.y_body_rot, max);
        r.mob_mut()?.parent.y_head_rot = head;
        let mob = r.mob_mut()?;
        mob.ai.body_rotation_control.last_stable_y_head_rot = mob.parent.y_head_rot;
        mob.ai.body_rotation_control.head_stable_time = 0;
    } else if !r.carrying_mob_passenger()? {
        let mob = r.mob()?;
        if (mob.parent.y_head_rot - mob.ai.body_rotation_control.last_stable_y_head_rot).abs()
            > 15.0
        {
            let mob = r.mob_mut()?;
            mob.ai.body_rotation_control.head_stable_time = 0;
            mob.ai.body_rotation_control.last_stable_y_head_rot = mob.parent.y_head_rot;
            let max = r.max_head_y_rot()? as f32;
            let mob = r.mob()?;
            let body = mth::rotate_if_necessary(mob.parent.y_body_rot, mob.parent.y_head_rot, max);
            r.mob_mut()?.parent.y_body_rot = body;
        } else {
            let c = &mut r.mob_mut()?.ai.body_rotation_control;
            c.head_stable_time = c.head_stable_time.wrapping_add(1);
            if c.head_stable_time > 10 {
                let elapsed = c.head_stable_time.wrapping_sub(10);
                let fraction = mth::clamp_f32(elapsed as f32 / 10.0, 0.0, 1.0);
                let angle = r.max_head_y_rot()? as f32 * (1.0 - fraction);
                let mob = r.mob()?;
                let body =
                    mth::rotate_if_necessary(mob.parent.y_body_rot, mob.parent.y_head_rot, angle);
                r.mob_mut()?.parent.y_body_rot = body;
            }
        }
    }
    Ok(())
}
