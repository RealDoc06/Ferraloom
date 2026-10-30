//! Shared resident section storage and live gameplay adapters.
//! No rendering, audio, input or network dependencies belong here.
#![forbid(unsafe_code)]
pub mod chunk;
pub mod collisions;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
