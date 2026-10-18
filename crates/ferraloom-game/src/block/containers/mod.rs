//! Vanilla container block states and their stateless behaviors.
pub mod access;
mod behavior;
pub mod chest;
pub mod crafter;
pub mod drops;
pub mod furnace;
pub mod loot;
pub mod menu;
pub mod runtime;
pub mod sign;
pub mod storage;
pub(crate) use behavior::ported_behavior;
