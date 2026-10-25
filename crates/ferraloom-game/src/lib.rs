//! Region-owned simulation. See `docs/design/game-core.md` for porting rules.
//!
pub mod advancements;
pub mod block;
pub mod bossbar;
pub mod commands;
pub mod crafting;
pub mod entity;
pub mod gamerules;
pub mod inventory;
pub mod item;
pub mod language;
pub mod level;
pub mod loot;
pub mod particles;
pub mod predicates;
pub mod scoreboard;
pub mod stats;
pub mod ticks;

pub use block::dispatch::Behaviors;
pub use scoreboard::ServerShared;

#[cfg(test)]
mod tests;
