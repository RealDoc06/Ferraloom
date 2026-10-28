//! Bell state is transient; ringing, brain memories and resonance are unported.
use super::{BlockEntity, BlockEntityData, BlockEntityRef};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_util::Direction;

/// Port of BellBlockEntity. It inherits BlockEntity's empty load/save codecs.
#[derive(Clone, Debug)]
pub struct BellBlockEntity {
    pub parent: BlockEntityData,
    pub last_ring_timestamp: i64,
    pub ticks: i32,
    pub shaking: bool,
    pub click_direction: Option<Direction>,
    pub nearby_entities: Option<Vec<EntityRef>>,
    pub resonating: bool,
    pub resonation_ticks: i32,
}
impl BellBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            last_ring_timestamp: 0,
            ticks: 0,
            shaking: false,
            click_direction: None,
            nearby_entities: None,
            resonating: false,
            resonation_ticks: 0,
        }
    }
}
pub(super) struct BellBehavior;
pub(super) static BELL_ENTITY: BellBehavior = BellBehavior;
impl BlockEntity for BellBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, entity: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(entity)?.data().block_state.block(),
        ))
    }
}
