//! Region-owned simulation. See `docs/design/game-core.md` for porting rules.
//!
pub mod block;
pub mod crafting;
pub mod entity;
pub mod inventory;
pub mod level;
pub mod loot;
pub mod predicates;
pub mod stats;
pub mod ticks;

pub use block::dispatch::Behaviors;

#[cfg(test)]
mod tests;
