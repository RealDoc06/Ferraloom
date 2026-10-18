//! Vanilla container storage, slot rules, click simulation and remote synchronization.
mod block_containers;
mod container;
pub use block_containers::{block_menu_still_valid, with_block_menu_context};
mod crafting;
mod crafting_runtime;
mod listener_copies;
mod live;
mod menu;
mod menus;
mod operation;
mod server_player;
mod slot;
mod stack_runtime;
mod stacks;
mod sync;
mod transfer;

pub use container::*;
pub use crafting::*;
pub use crafting_runtime::*;
pub use listener_copies::{ListenerCopyArena, ListenerCopyRef};
pub use live::*;
pub use menu::*;
pub use menus::*;
pub use operation::{
    MenuSynchronizationContext, broadcast_menu_changes, broadcast_menu_full_state,
    send_all_menu_data, set_menu_synchronizer,
};
pub use server_player::*;
pub use slot::*;
pub use stack_runtime::{StackOperation, swap_player_hands};
pub use stacks::{
    StackForward, StackGenerations, StackRef, StackReplacement, StackSlotRef, StackValue,
};
pub use sync::*;
pub use transfer::{MenuTransferContext, move_live_menu_item_stack_to, move_menu_item_stack_to};

use crate::entity::player::inventory::{
    copy_with_count as with_count, count, is_same_item_same_components, is_stackable,
    max_stack_size, set_count, split,
};
pub use ferraloom_protocol::component::ItemStack;

pub type Result<T> = std::result::Result<T, MenuError>;

#[derive(Debug)]
pub enum MenuError {
    MissingStack(StackRef),
    MissingMenu(MenuRef),
    InvalidSlot(i32),
    ContainerTooSmall { actual: usize, expected: usize },
    Unsupported(&'static str),
    Protocol(ferraloom_protocol::Error),
    Crafting(crate::crafting::Error),
    Level(crate::level::LevelError),
}
impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingStack(reference) => write!(f, "missing stack receiver {reference:?}"),
            Self::MissingMenu(reference) => write!(f, "missing menu receiver {reference:?}"),
            Self::InvalidSlot(slot) => write!(f, "invalid menu slot {slot}"),
            Self::ContainerTooSmall { actual, expected } => {
                write!(f, "container has {actual} slots, needs {expected}")
            }
            Self::Unsupported(operation) => write!(f, "unsupported menu behavior: {operation}"),
            Self::Protocol(error) => error.fmt(f),
            Self::Crafting(error) => error.fmt(f),
            Self::Level(error) => write!(f, "{error:?}"),
        }
    }
}
impl std::error::Error for MenuError {}
impl From<ferraloom_protocol::Error> for MenuError {
    fn from(error: ferraloom_protocol::Error) -> Self {
        Self::Protocol(error)
    }
}

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    if a.is_empty() || b.is_empty() {
        return a.is_empty() && b.is_empty();
    }
    match (&a.0, &b.0) {
        (Some(a), Some(b)) => a.item == b.item,
        _ => a.is_empty() && b.is_empty(),
    }
}
fn matches(a: &ItemStack, b: &ItemStack) -> bool {
    (a.is_empty() && b.is_empty()) || (count(a) == count(b) && is_same_item_same_components(a, b))
}

impl From<crate::level::LevelError> for MenuError {
    fn from(error: crate::level::LevelError) -> Self {
        Self::Level(error)
    }
}

#[cfg(test)]
mod tests;

impl From<crate::crafting::Error> for MenuError {
    fn from(error: crate::crafting::Error) -> Self {
        Self::Crafting(error)
    }
}
