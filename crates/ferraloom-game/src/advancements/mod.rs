//! Vanilla advancement data and player-local engine. World adapters supply facts
//! and apply returned effects synchronously at the triggering call site.
mod data;
mod player;
mod progress;
mod tree;
pub mod triggers;
pub use data::*;
pub use player::*;
pub use progress::*;
pub use tree::*;

pub use crate::loot::{Identifier, LootError as AdvancementError, Result};

#[cfg(test)]
mod tests;

/// JsonOps optional fields treat JSON null as absent. Required fields and map
/// entries retain their ordinary codec errors.
pub(crate) fn optional<'a, T>(
    v: &'a serde_json::Value,
    name: &str,
    decode: impl FnOnce(&'a serde_json::Value) -> Result<T>,
) -> Result<Option<T>> {
    crate::loot::codec::object(v)?
        .get(name)
        .filter(|v| !v.is_null())
        .map(decode)
        .transpose()
}
