//! CopperGolemStatueBlockEntity has only the base component map.
use super::{BlockEntity, BlockEntityData};

/// Port of CopperGolemStatueBlockEntity. Golem creation remains an entity port;
/// the inherited load/save/components and null ticker are native base behavior.
#[derive(Clone, Debug)]
pub struct CopperGolemStatueBlockEntity {
    pub parent: BlockEntityData,
}
impl CopperGolemStatueBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self { parent }
    }
}
pub(super) struct CopperGolemStatueBehavior;
pub(super) static COPPER_GOLEM_STATUE_ENTITY: CopperGolemStatueBehavior = CopperGolemStatueBehavior;
impl BlockEntity for CopperGolemStatueBehavior {
    fn sends_update_packet(&self) -> bool {
        true
    }
}
