//! A server's mobs as a client animates them. The server thread, a LAN host
//! and a dedicated server run each mob's animation state (`ai::tick`); a
//! client of a server only gets synched data and entity events, and runs the
//! animation steps vanilla runs on the client (`Entity.onSyncedDataUpdated`,
//! `handleEntityEvent` and the per tick animation states). The server's half
//! is `game/lan.rs` (`mob_meta`, with `pose_id` here for index 6) and
//! `MobEvent::EntityEvent`; the client's half is `client_tick` here (the
//! client that played it was removed with the rest of the client).
//!
//! To animate another species for a client:
//! - send its synched data in `mob_meta` and read it in
//!   `remote_mob_data::apply`;
//! - send its poses in `pose_id` and read them in `set_pose`;
//! - broadcast its entity events with `MobEvent::EntityEvent` and handle
//!   them in `entity_event`;
//! - run its client side animation in `client_tick`, sharing the function
//!   with the server's `ai_step` where the server computes the same.
use super::{Mob, Species, breeze, camel, villager, warden};

// `Pose.id` of the poses mobs take.
pub const STANDING: u8 = 0;
pub const SLEEPING: u8 = 2;
pub const LONG_JUMPING: u8 = 6;
pub const CROAKING: u8 = 8;
pub const USING_TONGUE: u8 = 9;
pub const SITTING: u8 = 10;

/// `Entity.DATA_POSE` (index 6) of a mob of a server: the pose its species
/// keeps in its own fields.
pub fn pose_id(m: &Mob) -> u8 {
    match m.species() {
        Species::Warden => warden::pose_id(m),
        Species::Breeze => breeze::pose_id(m),
        Species::Camel | Species::CamelHusk if camel::sitting(m) => SITTING,
        Species::Frog => m.extra.pose,
        // LivingEntity.startSleeping: the pose, with the bed in index 14.
        Species::Villager if villager::sleeping(m) => SLEEPING,
        _ => STANDING,
    }
}
