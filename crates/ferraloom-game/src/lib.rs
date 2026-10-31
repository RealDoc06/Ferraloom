//! Shared resident section storage and live gameplay adapters.
//! No rendering, audio, input or network dependencies belong here.
#![forbid(unsafe_code)]
pub mod block_phase;
pub mod chunk;
pub mod collisions;
pub mod fast_hash;
pub mod random_tick_sections;
pub mod server_level;
pub mod tick_chunk_membership;
pub mod ticket_levels;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
