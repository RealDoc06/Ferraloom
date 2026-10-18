//! Region-local entity outputs consumed by the concrete server integration.
use super::EntityRef;
use ferraloom_util::{Uuid, Vec3};

#[derive(Clone, Debug)]
pub enum EntityEffect {
    /// PrimedTnt#explode's synchronous explosion call. The explosion port owns
    /// damage and block destruction; used_portal selects its native calculator.
    Explosion {
        entity: EntityRef,
        position: Vec3,
        power: f32,
        owner: Option<Uuid>,
        used_portal: bool,
    },
}
