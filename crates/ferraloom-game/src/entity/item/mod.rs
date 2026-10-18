//! Vanilla's first nonliving entity implementations.
pub mod item_entity;
mod primed_tnt;
pub use item_entity::{ItemEntity, ItemEntityBehavior};
pub use primed_tnt::{PrimedTnt, PrimedTntBehavior};
