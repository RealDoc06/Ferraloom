//! Vanilla 26.3 loot registries, context providers and deterministic evaluation.
//!
//! Callers own the random stream and live facts. Unsupported facts are errors,
//! never fabricated world state. Named streams are managed by `RandomSequences`.
mod bonus;
pub(crate) mod codec;
mod component_function;
mod condition;
mod context;
mod enchantment;
mod entry;
mod function;
mod nbt;
mod number;
mod registry;
mod sequence;
mod table;
mod validation;
mod world_function;

pub use bonus::BonusFormula;
pub use component_function::{ComponentFunction, ComponentSource};
pub use condition::LootItemCondition;
pub use context::*;
pub use enchantment::{EnchantmentOperation, LevelBasedValue};
pub use entry::{EntryKind, LootPoolEntryContainer};
pub use function::{FunctionKind, LootItemFunction};
pub use nbt::{NbtProvider, NbtSource};
pub use number::ScoreboardNameProvider;
pub use number::{ContextFloatProvider, ContextIntProvider};
pub use registry::{Holder, HolderSet, LootData};
pub use sequence::RandomSequences;
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
            Self::Arithmetic => f.write_str("invalid number provider arithmetic"),
            Self::Validation(problems) => {
                write!(f, "{} fatal loot validation problem(s)", problems.len())
            }
        }
    }
}
impl std::error::Error for LootError {}
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
