//! EnderChestBlockEntity owns opener/lid state, not the player's inventory.
use super::{BlockEntity, BlockEntityData, BlockEntityState};

/// Port of EnderChestBlockEntity and its transient ChestLidController and
/// ContainerOpenersCounter fields. None of these fields is persistent.
#[derive(Clone, Debug)]
pub struct EnderChestBlockEntity {
    pub parent: BlockEntityData,
    pub open_count: i32,
    pub max_interaction_range: f64,
    pub should_be_open: bool,
    pub openness: f32,
    pub old_openness: f32,
}
impl EnderChestBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            open_count: 0,
            max_interaction_range: 0.0,
            should_be_open: false,
            openness: 0.0,
            old_openness: 0.0,
        }
    }
}
pub(super) struct EnderChestBehavior;
pub(super) static ENDER_CHEST_ENTITY: EnderChestBehavior = EnderChestBehavior;
impl BlockEntity for EnderChestBehavior {
    // Native load/save, components and server ticker are inherited base methods.
    fn trigger_event(&self, state: &mut BlockEntityState, event: i32, value: i32) -> bool {
        let BlockEntityState::EnderChest(entity) = state else {
            unreachable!()
        };
        if event == 1 {
            entity.should_be_open = value > 0;
            true
        } else {
            false
        }
    }
}
