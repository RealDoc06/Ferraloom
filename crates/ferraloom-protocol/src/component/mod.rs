//! Vanilla 26.3 item data components, patches and stack representations.
mod patch;
mod stack;
mod values;
pub use patch::*;
pub use stack::*;
pub use values::*;

use crate::{Identifier, Result};
pub(crate) fn registry(name: &str) -> Identifier {
    Identifier::parse(name).expect("vanilla registry identifier")
}
pub(crate) fn invalid(kind: &'static str) -> crate::Error {
    crate::Error::invalid(kind, "invalid value")
}

/// Static vanilla data-component registry ID, generated from registry reports.
pub type ComponentTypeId = ferraloom_data::registries::minecraft_data_component_type::Id;
#[cfg(test)]
pub(crate) mod tests;
