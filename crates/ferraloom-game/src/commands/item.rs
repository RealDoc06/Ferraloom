//! ItemCommands support independent of Entity#getSlot's receiver dispatch.
//! Slots are canonical handles; operations read them at each native call site.
mod collection;
pub use collection::{SlotCollection, SlotOperation, SlotUpdate};
