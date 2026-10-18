//! Canonical item behavior shared by entities, menus and interaction callers.
mod behavior;
pub mod block_item;
pub mod name;
pub mod stack;
pub use behavior::{ItemBehavior, ported_behavior, required_behavior};

#[cfg(test)]
mod tests;
