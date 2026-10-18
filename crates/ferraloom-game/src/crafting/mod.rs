//! Vanilla 26.3 recipes. Inputs are owned snapshots; matching never mutates menus.

mod book;
mod display;
mod ingredient;
mod input;
mod manager;
mod recipe;
mod special;
mod stack;
mod stacked_contents;
mod sync;

pub use book::{RecipeBookAddition, ServerRecipeBook};
pub use ingredient::Ingredient;
pub use input::{CraftingInput, PositionedCraftingInput, RecipeInput, SmithingRecipeInput};
pub use manager::{CachedCheck, RecipeCache, RecipeHolder, RecipeManager};
pub use recipe::{CraftingContext, Recipe, RecipeKind, RecipeType, ShapedRecipePattern};
pub use stacked_contents::{PlacementInfo, StackedItemContents};
pub use sync::{RecipeSynchronization, ServerDisplayInfo};

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    ContextRequired(&'static str),
    Protocol(ferraloom_protocol::Error),
    Datapack(ferraloom_data::datapack::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::ContextRequired(name) => write!(f, "recipe requires {name}"),
            Self::Protocol(error) => error.fmt(f),
            Self::Datapack(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<ferraloom_protocol::Error> for Error {
    fn from(value: ferraloom_protocol::Error) -> Self {
        Self::Protocol(value)
    }
}
impl From<ferraloom_data::datapack::Error> for Error {
    fn from(value: ferraloom_data::datapack::Error) -> Self {
        Self::Datapack(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
