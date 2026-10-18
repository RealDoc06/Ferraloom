//! Vanilla container storage, slot rules, click simulation and remote synchronization.
mod container;
mod menu;
mod menus;
mod server_player;
mod slot;
mod sync;

pub use container::*;
pub use menu::*;
pub use menus::*;
pub use server_player::*;
pub use slot::*;
pub use sync::*;

use crate::entity::player::inventory::{
    copy_with_count as with_count, count, is_same_item_same_components, is_stackable,
    max_stack_size, set_count, split,
};
pub use ferraloom_protocol::component::ItemStack;

pub type Result<T> = std::result::Result<T, MenuError>;

#[derive(Debug)]
pub enum MenuError {
    InvalidSlot(i32),
    ContainerTooSmall { actual: usize, expected: usize },
    Unsupported(&'static str),
    Protocol(ferraloom_protocol::Error),
    Level(crate::level::LevelError),
}
impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSlot(slot) => write!(f, "invalid menu slot {slot}"),
            Self::ContainerTooSmall { actual, expected } => {
                write!(f, "container has {actual} slots, needs {expected}")
            }
            Self::Unsupported(operation) => write!(f, "unsupported menu behavior: {operation}"),
            Self::Protocol(error) => error.fmt(f),
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
