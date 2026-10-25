//! Vanilla's status graph and owned generation tasks. Runtime owns executors;
//! no job borrows a live Level or chooses integration order by completion time.
mod context;
mod live_holder;
mod proto;
mod pyramid;
mod range;
mod task;
pub use context::WorldGenContext;
pub use ferraloom_worldgen::region::ChunkStatus;
pub use live_holder::{
    GenerationFutureId, GenerationFutureResult, GenerationSchedule, GenerationTaskChange,
    GenerationTaskToken, HolderGeneration,
};
pub use proto::{CapturedChunkTerrain, ProtoChunk, promote};
pub use pyramid::{
    ChunkDependencies, ChunkPyramid, ChunkStatusTask, ChunkStep, STATUSES, parse_status,
    status_name,
};
pub use range::ChunkRangeFuture;
pub use task::{ChunkGenerationTask, GenerationChunkHolder, GenerationError, GenerationLayer};
