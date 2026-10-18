//! ServerEntity movement encoding and per-player entity tracking.
pub mod codec;
mod server_entity;
pub use server_entity::{ServerEntity, TrackingAudience, TrackingError, TrackingPacket};
mod tracked_entity;
pub use tracked_entity::{DirectedTrackingMessage, PlayerTrackingView, TrackedEntity};
