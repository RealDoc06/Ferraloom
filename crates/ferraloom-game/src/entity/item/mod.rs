//! Vanilla's first nonliving entity implementations.
pub mod falling_block;
pub mod item_entity;
pub use falling_block::{FallingBlockBehavior, FallingBlockEntity};
mod primed_tnt;
pub use item_entity::{ItemEntity, ItemEntityBehavior};
pub use primed_tnt::{PrimedTnt, PrimedTntBehavior};
