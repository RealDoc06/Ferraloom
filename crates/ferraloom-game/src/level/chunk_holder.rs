use ferraloom_util::ChunkPos;
/// Port of ChunkStatus's status chain. Carvers, features and structures are
/// explicit passthrough stages until their generation ports land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChunkStatus {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Noise,
    Surface,
    Carvers,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}
impl ChunkStatus {
    pub fn is_passthrough(self) -> bool {
        matches!(
            self,
            Self::StructureStarts
                | Self::StructureReferences
                | Self::Carvers
                | Self::Features
                | Self::Spawn
        )
    }
}
/// Port of GenerationChunkHolder's request identity and status visibility.
#[derive(Debug)]
pub struct ChunkHolder {
    pub position: ChunkPos,
    pub status: ChunkStatus,
    pub request_id: u64,
    pub epoch: u64,
    pub ticket_level: i32,
}
impl ChunkHolder {
    pub fn new(position: ChunkPos, request_id: u64) -> Self {
        Self {
            position,
            status: ChunkStatus::Empty,
            request_id,
            epoch: 0,
            ticket_level: 33,
        }
    }
}
