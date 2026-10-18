//! ServerEntity movement encoding and per-player entity tracking.
pub mod codec;
mod server_entity;
pub use server_entity::{ServerEntity, TrackingAudience, TrackingError, TrackingPacket};
