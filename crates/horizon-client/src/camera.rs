/// `Math.min(float, float)`: NaN wins and -0.0 is below 0.0.
pub fn java_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        f32::from_bits(a.to_bits() | b.to_bits())
    } else if a <= b {
        a
    } else {
        b
    }
}
/// `Math.max(float, float)`.
pub fn java_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        f32::from_bits(a.to_bits() & b.to_bits())
    } else if a >= b {
        a
    } else {
        b
    }
}
/// `Mth.wrapDegrees(float)`.
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut normalized = angle % 360.0;
    if normalized >= 180.0 {
        normalized -= 360.0;
    }
    if normalized < -180.0 {
        normalized += 360.0;
    }
    normalized
}

/// JOML's float trigonometry (`org.joml.Math` without fast math).
pub mod joml {}

/// `ClientAvatarState`: the walked distance and view bob of a player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AvatarBob {
    pub walk_dist: f32,
    pub walk_dist_o: f32,
    pub bob: f32,
    pub bob_o: f32,
}
impl AvatarBob {
    /// `ClientAvatarState.tick` (at the start of `AbstractClientPlayer.tick`).
    pub fn tick(&mut self) {
        self.walk_dist_o = self.walk_dist;
    }
    /// `LocalPlayer.move`: `addWalkedDistance(Mth.length(dx, dz) * 0.6F)`
    /// with the float horizontal displacement.
    pub fn add_walked(&mut self, dx: f32, dz: f32) {
        self.walk_dist += (dx * dx + dz * dz).sqrt() * 0.6;
    }
    /// `AbstractClientPlayer.updateBob`: toward the horizontal speed (at most
    /// 0.1) on the ground, toward 0 otherwise.
    pub fn update_bob(&mut self, on_ground_alive_not_swimming: bool, velocity: [f64; 3]) {
        let target = if on_ground_alive_not_swimming {
            java_min(
                0.1,
                (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt() as f32,
            )
        } else {
            0.0
        };
        self.bob_o = self.bob;
        self.bob += (target - self.bob) * 0.4;
    }
    /// `ClientAvatarState.resetBob` (`AbstractClientPlayer.rideTick`).
    pub fn reset_bob(&mut self) {
        self.bob_o = self.bob;
        self.bob = 0.0;
    }
}
