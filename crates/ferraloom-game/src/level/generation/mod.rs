//! Vanilla's status graph and owned generation tasks. Runtime owns executors;
//! no job borrows a live Level or chooses integration order by completion time.
mod context;
mod proto;
mod pyramid;
mod task;
pub use context::WorldGenContext;
pub use ferraloom_worldgen::region::ChunkStatus;
pub use proto::{ProtoChunk, promote};
pub use pyramid::{
    ChunkDependencies, ChunkPyramid, ChunkStatusTask, ChunkStep, STATUSES, parse_status,
    status_name,
};
pub use task::{ChunkGenerationTask, GenerationChunkHolder, GenerationError, GenerationLayer};
