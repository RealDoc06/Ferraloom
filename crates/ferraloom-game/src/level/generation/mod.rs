//! Vanilla's status graph and owned generation tasks. Runtime owns executors;
//! no job borrows a live Level or chooses integration order by completion time.
mod context;
mod proto;
mod pyramid;
mod task;
pub use ferraloom_worldgen::region::ChunkStatus;
pub use pyramid::{ChunkDependencies, ChunkPyramid, ChunkStep, ChunkStatusTask, STATUSES, status_name, parse_status};
pub use context::WorldGenContext;
pub use proto::{ProtoChunk, promote};
pub use task::{ChunkGenerationTask, GenerationChunkHolder, GenerationError, GenerationLayer};
