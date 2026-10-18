//! Region-local entity outputs consumed by the concrete server integration.
use super::EntityRef;
use ferraloom_util::{Uuid, Vec3};

#[derive(Clone, Debug)]
pub enum EntityEffect {
    TakeItem {
        entity: EntityRef,
        player: EntityRef,
        count: i32,
    },
    /// Pending statistics/advancement and menu broadcast integration. Inventory
    /// and source stack have already been committed when this hook is emitted.
    ItemPickup {
        player: EntityRef,
        entity: EntityRef,
        item: ferraloom_data::Item,
        count: i32,
    },
    ItemDrop {
        player: EntityRef,
        entity: EntityRef,
        item: ferraloom_data::Item,
        count: i32,
        thrown_from_hand: bool,
    },
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
