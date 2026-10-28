//! Owned native controller state. Live ticks reborrow these fields in Mob.ai;
//! controllers do not hold a second entity, world view or mutable dispatcher.
use ferraloom_util::{Vec3, mth};

/// Port of MoveControl.Operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Operation {
    #[default]
    Wait,
    MoveTo,
    Strafe,
    Jumping,
}
/// Port of MoveControl's constructor state and requested movement commands.
#[derive(Debug)]
pub struct MoveControl {
    kind: super::super::MoveControlKind,
    pub(crate) wanted: Vec3,
    pub(crate) speed_modifier: f64,
    pub(crate) strafe_forwards: f32,
    pub(crate) strafe_right: f32,
    pub(crate) operation: Operation,
}
impl Default for MoveControl {
    fn default() -> Self {
        Self::new(super::super::MoveControlKind::Ground)
    }
}
impl MoveControl {
    /// Port of Mob#createMoveControl and constructor replacement. Family bodies
    /// stay required until their concrete controller algorithm is implemented.
    pub fn new(kind: super::super::MoveControlKind) -> Self {
        Self {
            kind,
            wanted: Vec3::ZERO,
            speed_modifier: 0.0,
            strafe_forwards: 0.0,
            strafe_right: 0.0,
            operation: Operation::Wait,
        }
    }
    pub fn kind(&self) -> super::super::MoveControlKind {
        self.kind
    }

    /// Port of MoveControl#hasWanted.
    pub fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }
    /// Port of MoveControl#getSpeedModifier.
    pub fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }
    /// Port of MoveControl#getWantedX/getWantedY/getWantedZ.
    pub fn wanted_position(&self) -> Vec3 {
        self.wanted
    }
    /// Port of MoveControl#setWantedPosition; an ongoing jump retains its operation.
    pub fn set_wanted_position(&mut self, wanted: Vec3, speed_modifier: f64) {
        self.wanted = wanted;
        self.speed_modifier = speed_modifier;
        if self.operation != Operation::Jumping {
            self.operation = Operation::MoveTo;
        }
    }
    /// Port of MoveControl#strafe.
    pub fn strafe(&mut self, forwards: f32, right: f32) {
        self.operation = Operation::Strafe;
        self.strafe_forwards = forwards;
        self.strafe_right = right;
        self.speed_modifier = 0.25;
    }
    /// Port of MoveControl#setWait.
    pub fn set_wait(&mut self) {
        self.operation = Operation::Wait;
    }
}
/// Port of MoveControl#rotlerp. Only one revolution is corrected, preserving
/// native endpoint and out-of-range behavior rather than applying a modulo.
pub fn rotlerp(from: f32, to: f32, max: f32) -> f32 {
    let mut diff = mth::wrap_degrees_f32(to - from);
    if diff > max {
        diff = max;
    }
    if diff < -max {
        diff = -max;
    }
    let mut result = from + diff;
    if result < 0.0 {
        result += 360.0;
    } else if result > 360.0 {
        result -= 360.0;
    }
    result
}
/// Port of Control#rotateTowards; this deliberately does not normalize the result.
pub fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    from + mth::clamp_f32(mth::degrees_difference(from, to), -max, max)
}
/// Port of LookControl's retained target and two-tick cooldown.
#[derive(Debug, Default)]
pub struct LookControl {
    pub(crate) y_max_rot_speed: f32,
    pub(crate) x_max_rot_angle: f32,
    pub(crate) look_at_cooldown: i32,
    pub(crate) wanted: Vec3,
}
impl LookControl {
    /// Port of LookControl#setLookAt's explicit rotation-limit overload.
    pub fn set_look_at(&mut self, wanted: Vec3, y_max_rot_speed: f32, x_max_rot_angle: f32) {
        self.wanted = wanted;
        self.y_max_rot_speed = y_max_rot_speed;
        self.x_max_rot_angle = x_max_rot_angle;
        self.look_at_cooldown = 2;
    }
    /// Port of LookControl#isLookingAtTarget.
    pub fn is_looking_at_target(&self) -> bool {
        self.look_at_cooldown > 0
    }
    /// Port of LookControl#getWantedX/getWantedY/getWantedZ.
    pub fn wanted_position(&self) -> Vec3 {
        self.wanted
    }
    /// Port of LookControl#getYRotD with the current Mob position.
    pub fn y_rot_d(&self, position: Vec3) -> Option<f32> {
        let xd = self.wanted.x - position.x;
        let zd = self.wanted.z - position.z;
        if !(zd.abs() > 9.999999747378752E-6 || xd.abs() > 9.999999747378752E-6) {
            None
        } else {
            Some((mth::atan2(zd, xd) * 57.2957763671875) as f32 - 90.0)
        }
    }
    /// Port of LookControl#getXRotD with the current Mob eye position.
    pub fn x_rot_d(&self, eye: Vec3) -> Option<f32> {
        let xd = self.wanted.x - eye.x;
        let yd = self.wanted.y - eye.y;
        let zd = self.wanted.z - eye.z;
        let sd = (xd * xd + zd * zd).sqrt();
        if !(yd.abs() > 9.999999747378752E-6 || sd.abs() > 9.999999747378752E-6) {
            None
        } else {
            Some(-(mth::atan2(yd, sd) * 57.2957763671875) as f32)
        }
    }
}
/// Port of JumpControl's retained request. The live tick clears it after the
/// derived Mob#setJumping callback, never before that synchronous call.
#[derive(Debug, Default)]
pub struct JumpControl {
    pub(crate) jump: bool,
}
impl JumpControl {
    /// Port of JumpControl#jump.
    pub fn jump(&mut self) {
        self.jump = true;
    }
}
/// Port of BodyRotationControl's constructor state.
#[derive(Debug, Default)]
pub struct BodyRotationControl {
    pub(crate) head_stable_time: i32,
    pub(crate) last_stable_y_head_rot: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    const VECTORS: &str = include_str!("../../../../tests/java/ai/control/vectors.tsv");
    #[test]
    fn native_control_rotation_keeps_single_revolution_and_distinct_result_normalization() {
        let mut checked = 0;
        for row in VECTORS.lines().filter(|r| r.starts_with("rotation\t")) {
            let fields: Vec<_> = row.split('\t').collect();
            let values: Vec<_> = fields[1..]
                .iter()
                .map(|s| f32::from_bits(u32::from_str_radix(s, 16).unwrap()))
                .collect();
            assert_eq!(
                rotlerp(values[0], values[1], values[2]).to_bits(),
                values[3].to_bits(),
                "{row}"
            );
            assert_eq!(
                rotate_towards(values[0], values[1], values[2]).to_bits(),
                values[4].to_bits(),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, 63);
    }
}
