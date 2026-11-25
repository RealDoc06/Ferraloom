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
//! event calls: [`players`], [`blocks`], [`worlds`], [`server`]. [`bridge`] holds the JVM
//! and the lifecycle.

mod blocks;
mod bridge;
mod commands;
mod events;
mod jvm;
mod lent;
mod natives;
mod players;
mod server;
mod worlds;

pub use blocks::{
    BlockHost, BreakOutcome, Placing, Simple, block_break, block_explode, block_place, cancelled,
    piston, sign_change, simple, table_line,
};
pub use bridge::{Config, WorldSpec, console_command, disable, enable, running, start, tick};
pub use commands::command_tree;
pub use events::{Event, listens};
pub use lent::{borrow_lent, lend};
pub use players::{Joined, PlayerHost, chat, player_command, player_join, player_quit};
pub use server::{
    Ban, BanTarget, List, PluginBossBar, Profile, ServerHost, ServerLists, WhitelistFlag,
    boss_bars_for, hidden_commands, set_lists, tab_complete,
};
pub use worlds::{Border, Change, Query, Sound, WorldHost, chunk_event, save_event, weather_event};

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
pub trait PluginHost: PlayerHost + BlockHost + WorldHost + ServerHost {}

impl<T: PlayerHost + BlockHost + WorldHost + ServerHost + ?Sized> PluginHost for T {}
