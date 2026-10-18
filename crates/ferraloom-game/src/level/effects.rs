//! Ordered integration points for subsystem ports. Effects are captured at the
//! vanilla call site and consumed at its phase boundary, never from hash order.
use super::events::BlockEvent;
use ferraloom_data::BlockState;
use ferraloom_util::{BlockPos, ChunkPos};

#[derive(Clone, Debug, PartialEq)]
pub enum LevelEffect {
    SectionEmptiness {
        chunk: ChunkPos,
        section_y: i32,
        empty: bool,
    },
    CheckLight(BlockPos),
    PoiChanged {
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    },
    BlockUpdated {
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    },
    DropResources {
        pos: BlockPos,
        state: BlockState,
    },
    DestroyParticles {
        pos: BlockPos,
        state: BlockState,
    },
    BlockDestroyed {
        pos: BlockPos,
        state: BlockState,
    },
    FallingBlock {
        pos: BlockPos,
        state: BlockState,
    },
    LevelEvent {
        pos: BlockPos,
        id: i32,
        data: i32,
    },
    BlockEvent(BlockEvent),
    WeatherEvent {
        event: u8,
        param: f32,
    },
    PendingPrecipitation(BlockPos),
}
