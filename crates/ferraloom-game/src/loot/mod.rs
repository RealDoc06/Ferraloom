//! Vanilla 26.3 loot registries, context providers and deterministic evaluation.
//!
//! Callers own the random stream and live facts. Unsupported facts are errors,
//! never fabricated world state. Named streams are managed by `RandomSequences`.
mod binding;
mod bonus;
pub(crate) mod codec;
mod component_function;
mod condition;
mod container_function;
mod context;
mod enchantment;
mod entry;
mod environment;
mod function;
mod item_function;
mod list_function;
mod list_operation;
mod nbt;
mod nbt_function;
mod number;
mod registry;
mod runtime;
mod sequence;
mod slot;
mod table;
mod validation;
mod world_function;

pub use binding::{
    RegistryBindable, RegistryHolder, RegistryKey, RegistryLookup, RegistryReference,
};
pub use bonus::BonusFormula;
pub use component_function::{ComponentFunction, ComponentSource};
pub use condition::LootItemCondition;
pub use container_function::{ContainerComponent, ContainerFunction};
pub use context::*;
pub use enchantment::{EnchantmentOperation, LevelBasedValue};
pub use entry::{EntryKind, LootPoolEntryContainer};
pub use environment::EnvironmentValue;
pub use function::{FunctionKind, LootItemFunction};
pub use item_function::{ItemFunction, LootAttributeModifier};
pub use list_function::ListFunction;
pub use list_operation::{ListEdit, ListOperation};
pub use nbt::{NbtProvider, NbtSource};
pub use nbt_function::{CopyCustomData, CopyOperation, NbtMergeStrategy};
pub use number::ScoreboardNameProvider;
pub use number::{ContextFloatProvider, ContextIntProvider};
pub use registry::{Holder, HolderSet, LootData};
pub use runtime::{LootRandomSelection, LootRuntime, NameableName};
pub use sequence::{DirtyMarkingRandomSource, RandomSequences};
pub use slot::{SlotRange, SlotSource, SlotTarget};
pub use table::{LootPool, LootRandomSources, LootTable};
pub use validation::{ValidationContext, ValidationProblem};
pub use world_function::{ExplorationMapOptions, WorldFunction};

pub use ferraloom_protocol::Identifier;
use std::fmt;
pub type Result<T> = std::result::Result<T, LootError>;

#[derive(Debug)]
pub enum LootError {
    Decode(String),
    MissingReference {
        registry: &'static str,
        id: Identifier,
    },
    MissingParameter(ContextParam),
    InvalidParameters {
        missing: Vec<ContextParam>,
        extra: Vec<ContextParam>,
    },
    Unsupported(String),
    CommandStorage(crate::commands::storage::StorageReadError),
    Arithmetic,
    Validation(Vec<ValidationProblem>),
}
impl fmt::Display for LootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(message) => write!(f, "loot codec: {message}"),
            Self::MissingReference { registry, id } => write!(f, "unknown {registry} {id}"),
            Self::MissingParameter(param) => write!(f, "missing loot parameter {param:?}"),
            Self::InvalidParameters { missing, extra } => write!(
                f,
                "invalid loot parameters: missing {missing:?}, extra {extra:?}"
            ),
            Self::Unsupported(fact) => write!(f, "unsupported loot fact: {fact}"),
            Self::CommandStorage(error) => fmt::Display::fmt(error, f),
            Self::Arithmetic => f.write_str("invalid number provider arithmetic"),
            Self::Validation(problems) => {
                write!(f, "{} fatal loot validation problem(s)", problems.len())
            }
        }
    }
}
impl std::error::Error for LootError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CommandStorage(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ferraloom_protocol::Error> for LootError {
    fn from(value: ferraloom_protocol::Error) -> Self {
        Self::Decode(value.to_string())
    }
}
impl From<ferraloom_data::datapack::Error> for LootError {
    fn from(value: ferraloom_data::datapack::Error) -> Self {
        Self::Decode(value.to_string())
    }
}

#[cfg(test)]
mod tests;
