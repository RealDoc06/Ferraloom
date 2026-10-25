use super::DoubleBounds;
use crate::loot::codec::DecodeInput;
use crate::loot::{Result, codec};
use ferraloom_util::Vec3;

/// Port of MovementPredicate. The context supplies getKnownMovement, not delta
/// movement; vanilla scales it by 20 before comparing signed axis bounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovementPredicate {
    pub x: DoubleBounds,
    pub y: DoubleBounds,
    pub z: DoubleBounds,
    pub speed: DoubleBounds,
    pub horizontal_speed: DoubleBounds,
    pub vertical_speed: DoubleBounds,
    pub fall_distance: DoubleBounds,
}
impl MovementPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let bound = |key| -> Result<DoubleBounds> {
            Ok(codec::optional(v, key, DoubleBounds::decode)?.unwrap_or_default())
        };
        Ok(Self {
            x: bound("x")?,
            y: bound("y")?,
            z: bound("z")?,
            speed: bound("speed")?,
            horizontal_speed: bound("horizontal_speed")?,
            vertical_speed: bound("vertical_speed")?,
            fall_distance: bound("fall_distance")?,
        })
    }
    pub fn matches(&self, known_movement: Vec3, fall_distance: f64) -> bool {
        let x = known_movement.x * 20.0;
        let y = known_movement.y * 20.0;
        let z = known_movement.z * 20.0;
        self.x.matches(x)
            && self.y.matches(y)
            && self.z.matches(z)
            && self.speed.matches_sqr(x * x + y * y + z * z)
            && self.horizontal_speed.matches_sqr(x * x + z * z)
            && self.vertical_speed.matches(y.abs())
            && self.fall_distance.matches(fall_distance)
    }
}
