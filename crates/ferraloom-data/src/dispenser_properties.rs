//! Immutable dispenser registry facts and ProjectileItem constructor parameters.
use crate::Item;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectileDispensePosition {
    Default,
    BlockFace,
    Firework,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileDispenseConfig {
    pub power: f32,
    pub uncertainty: f32,
    pub override_dispense_event: Option<i32>,
    pub position: ProjectileDispensePosition,
}
#[derive(Clone, Copy, Debug)]
pub struct DispenserProperties {
    pub spawn_egg: bool,
    pub registered_behavior: Option<DispenseBehaviorKind>,
    pub projectile: Option<ProjectileDispenseConfig>,
}
include!(concat!(env!("OUT_DIR"), "/dispenser_properties.rs"));
impl Item {
    pub const fn dispenser_properties(self) -> &'static DispenserProperties {
        &DISPENSER_PROPERTIES[self.protocol_id() as usize]
    }
}
