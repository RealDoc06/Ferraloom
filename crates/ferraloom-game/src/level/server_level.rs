use super::{Level, Region, chunk_map::ChunkMap, tickets::TicketStorage};
/// Port of ServerLevel's single-region ownership. Runtime jobs live in server.
pub struct ServerLevel {
    pub level: Level,
    pub chunks: ChunkMap,
    pub tickets: TicketStorage,
    pub game_time: i64,
    pub dimension: String,
}
impl ServerLevel {
    pub fn new(region: Region, dimension: impl Into<String>) -> Self {
        Self {
            level: Level::new(region),
            chunks: ChunkMap::default(),
            tickets: TicketStorage::default(),
            game_time: 0,
            dimension: dimension.into(),
        }
    }
}

impl ServerLevel {
    /// Ticket half of ServerLevel#setChunkForced. On a changed addition the runtime
    /// must finish getChunk synchronously before returning to the command caller.
    pub fn set_chunk_forced(&mut self, pos: ferraloom_util::ChunkPos, forced: bool) -> bool {
        self.tickets.set_chunk_forced(pos, forced)
    }
    pub fn get_force_loaded_chunks(&self) -> &ferraloom_util::collections::LongOpenHashSet {
        self.tickets.get_force_loaded_chunks()
    }
}
