//! Rust port of Mojang Brigadier 1.3.11.
//!
//! Cursors and ranges count UTF-16 code units, as in Brigadier's Java API.
//! The dispatcher owns an arena: [`NodeId`] handles stay stable across registration
//! and represent redirect cycles without reference cycles or locks.

pub mod arguments;
pub mod dispatcher;
pub mod exceptions;
pub mod reader;
pub mod suggestion;
pub mod tree;

pub use arguments::*;
pub use dispatcher::*;
pub use exceptions::*;
pub use reader::StringReader;
pub use suggestion::*;
pub use tree::*;

#[cfg(test)]
mod completion_tests;
#[cfg(test)]
mod dispatcher_tests;
#[cfg(test)]
mod reader_tests;
#[cfg(test)]
mod usage_tests;
