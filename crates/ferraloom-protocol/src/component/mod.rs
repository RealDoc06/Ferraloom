//! Vanilla 26.3 item data components, patches and stack representations.
pub use ferraloom_util::java::{JavaF32, JavaF64};
mod map;
mod patch;
pub use map::*;
mod persistent;
mod predicate;
pub use predicate::*;
mod stack;
mod values;
pub use patch::*;
pub use persistent::{Persistent, PersistentValue};
pub use stack::*;
pub use values::*;

use crate::Identifier;
pub(crate) fn registry(name: &str) -> Identifier {
    Identifier::parse(name).expect("vanilla registry identifier")
}
pub(crate) fn invalid(kind: &'static str) -> crate::Error {
    crate::Error::invalid(kind, "invalid value")
}

/// Static vanilla data-component registry ID, generated from registry reports.
pub type ComponentTypeId = ferraloom_data::registries::data_component_type::Id;
#[cfg(test)]
pub(crate) mod tests;
