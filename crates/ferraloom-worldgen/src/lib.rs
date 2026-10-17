//! Vanilla 26.3 generation. Seeded graphs are immutable; sampler contexts belong
//! to individual generation jobs and carry all mutable caches.

pub mod codec;
pub mod density;

pub use ferraloom_data::datapack::Identifier;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
impl From<ferraloom_data::datapack::Error> for Error {
    fn from(error: ferraloom_data::datapack::Error) -> Self {
        Self(error.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

pub mod router;

pub mod climate;
