//! Shared resident section storage and live gameplay adapters.
//! No rendering, audio, input or network dependencies belong here.
#![forbid(unsafe_code)]
pub mod chunk;
pub mod collisions;
pub mod fast_hash;
pub mod ticket_levels;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
