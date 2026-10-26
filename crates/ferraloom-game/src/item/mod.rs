//! Canonical item behavior shared by entities, menus and interaction callers.
mod behavior;
pub mod block_item;
pub mod block_transformer;
pub mod bone_meal;
pub mod damage;
pub mod durability;
pub mod enchantment;
pub mod name;
mod placement;
mod pov;
pub mod slot;
pub use pov::get_player_pov_hit_result;
pub mod stack;
pub use behavior::{ItemBehavior, ported_behavior, required_behavior};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod placement_tests;
