//! Bukkit/Spigot plugins in Ferraloom (docs/ferraloom/plugins.md).
//!
//! The server's `plugins/` folder holding a jar starts a JVM in the process
//! (libjvm found and loaded at run time, [`jvm`]) with Ferraloom's bridge
//! (`java/bridge/`), which implements the Bukkit API over the game through a
//! [`PluginHost`]. Without a plugin jar nothing here loads: no JVM, no thread,
//! and every event site in the game is one branch on [`listens`].
//!
//! All calls but [`chat`] and [`listens`] come from the server thread, which
//! is Bukkit's main thread. A call that can reach plugin code takes the host,
//! and the Bukkit API answers from it during the call ([`natives`]).
//! The crate knows nothing of `horizon-client`: the game implements the host
//! (`horizon-client/src/game/plugins.rs`).
//!
//! One module per API area, each with its host trait, its natives and its
//! event calls: [`players`], [`worlds`], [`server`], [`entities`]. [`bridge`] holds the JVM
//! and the lifecycle.

mod bridge;
mod commands;
mod entities;
mod events;
mod jvm;
mod lent;
mod natives;
mod players;
mod server;
mod worlds;

pub use bridge::{Config, WorldSpec, console_command, disable, enable, running, start, tick};
pub use commands::command_tree;
pub use entities::{
    Actor, Change, Damage, EntityHost, EntityRef, EntityState, Named, Reach, SpawnReason, Targeted,
    creature_spawn, entity_damage, entity_death, entity_explode, entity_target,
    flags as entity_flags, interact_entity,
};
pub use events::{Event, latch_for_tests, listens};
pub use lent::{borrow_lent, lend};
pub use players::{Joined, PlayerHost, chat, player_command, player_join, player_quit};
pub use server::ServerHost;
pub use worlds::{WorldHost, block_break};

/// A player as the bridge names them: their UUID.
pub type PlayerId = u128;

/// Where a player is: a world (its index in [`Config::worlds`]), a position and a rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    pub world: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

/// What the Bukkit API reads and changes of the game: one trait per area.
/// The game implements them over whatever it holds at the call (a level and
/// a player swapped in, the hub in or out), and lends the host to every call
/// into plugins that can call back.
///
/// Host methods are leaves: they never call into plugins themselves. A host
/// method that would fire an event lends itself to that dispatch, never the
/// host it was called through.
pub trait PluginHost: PlayerHost + WorldHost + ServerHost + EntityHost {}

impl<T: PlayerHost + WorldHost + ServerHost + EntityHost + ?Sized> PluginHost for T {}
