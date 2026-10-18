//! Region-owned simulation. See `docs/design/game-core.md` for porting rules.
//!
pub mod block;
pub mod entity;
pub mod level;
pub mod ticks;

pub use block::dispatch::Behaviors;

#[cfg(test)]
mod tests;
