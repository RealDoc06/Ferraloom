use super::{Level, Region, chunk_map::ChunkMap, tickets::TicketStorage};
/// Port of ServerLevel's single-region ownership. Runtime jobs live in server.
pub struct ServerLevel {
    pub level: Level,
    pub chunks: ChunkMap,
    pub tickets: TicketStorage,
    pub distance_manager: super::distance_manager::DistanceManager,
    pub game_time: i64,
    pub dimension: String,
}
impl ServerLevel {
    pub fn new(region: Region, dimension: impl Into<String>) -> Self {
        Self {
            level: Level::new(region),
            chunks: ChunkMap::default(),
            tickets: TicketStorage::default(),
            distance_manager: Default::default(),
            game_time: 0,
            dimension: dimension.into(),
        }
    }
}
