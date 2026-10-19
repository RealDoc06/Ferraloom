//! Vanilla 26.3 lighting on immutable world snapshots.
//!
//! Queue persisted light with `queue_section_data`, then run `initialize_light`
//! for the initialize_light status and `light_chunk` for the light status.
//! Before a changed snapshot is submitted, update section emptiness and call
//! `check_block` for each changed block. Integrate returned arrays in request
//! order, validating the job's owner epoch and input versions in the caller.
//! Supply `LightWorld::sky_light_sources` for known canonical chunk buffers;
//! raw bulk writes intentionally leave those buffers stale. The section-derived
//! source cache remains only for existing live callers without that snapshot.
//! The crate never owns live chunks, threads, IO, or scheduler state.

mod data_layer;
mod engine;
mod long_set;
mod occlusion;
mod queue;
mod sky_sources;
mod storage;
mod world;

pub use data_layer::DataLayer;
pub use engine::{LevelLightEngine, LightUpdates};
pub use occlusion::{
    has_different_light_properties, is_empty_shape, light_dampening_into, shape_occludes,
};
pub use sky_sources::ChunkSkyLightSources;
pub use storage::{LightLayer, LightSectionUpdate, SectionType};
pub use world::{LightWorld, SectionSnapshots};
