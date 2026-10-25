//! Canonical promotion and lighting completion, shared by ticks and commands.
use super::*;
use crate::world::Terrain;
use ferraloom_game::level::chunk::LevelChunk;

/// Owned FULL continuation, retained through later publication errors. Entity
/// callbacks commit once; reconstructing a chunk must never replay postLoad.
pub(crate) struct PromotionContinuation {
    pub identity: crate::generation::GenerationIdentity,
    pub input: Option<Terrain>,
    pub(crate) light_revision: crate::generation::GenerationRevision,
    pub(crate) prepared_chunk: Option<LevelChunk>,
    pub(crate) entities_registered: bool,
    pub(crate) entity_failure: Option<ferraloom_game::level::entity::WorldgenEntityLoadError>,
    pub(crate) complete: bool,
    pub(crate) center_light_installed: bool,
    pub(crate) stage: Option<ferraloom_game::level::publication::FullChunkStage>,
    pub(crate) finalized: bool,
    pub(crate) post_load_active: bool,
    pub(crate) promotion_active: bool,
    pub(crate) publication_failure:
        Option<Box<ferraloom_game::level::publication::ChunkPublicationError>>,
    pub(crate) finalization_failure:
        Option<ferraloom_game::level::publication::ChunkFinalizationError>,
}

impl MinecraftServer {
    /// ServerLevel#setChunkForced acquires FULL synchronously. FULL includes the
    /// canonical center light but never waits on prepareTickingChunk callbacks.
    pub(super) async fn await_generation_chunk(&mut self, position: ChunkPos) -> Result<()> {
        self.update_chunk_tickets()?;
        self.level
            .level
            .get_chunk_mut(position, ChunkStatus::Full, true)
            .map_err(game_error)?
            .context("required FULL acquisition returned unloaded")?;
        Ok(())
    }
}
