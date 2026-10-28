//! Conduit persistence and transient server/client fields; effects stay required.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{BlockPos, Uuid};

/// Port of ConduitBlockEntity. The serialized EntityReference is unresolved;
/// acquiring a live target belongs to the still-required conduit ticker.
#[derive(Clone, Debug)]
pub struct ConduitBlockEntity {
    pub parent: BlockEntityData,
    pub tick_count: i32,
    pub active_rotation: f32,
    pub is_active: bool,
    pub is_hunting: bool,
    pub effect_blocks: Vec<BlockPos>,
    pub destroy_target: Option<Uuid>,
    pub next_ambient_sound_activation: i64,
}
impl ConduitBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            tick_count: 0,
            active_rotation: 0.0,
            is_active: false,
            is_hunting: false,
            effect_blocks: Vec::new(),
            destroy_target: None,
            next_ambient_sound_activation: 0,
        }
    }
}
pub(super) struct ConduitBehavior;
pub(super) static CONDUIT_ENTITY: ConduitBehavior = ConduitBehavior;
impl BlockEntity for ConduitBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Conduit(entity) = state else {
            unreachable!()
        };
        entity.destroy_target = tag.get("Target").and_then(|tag| {
            let words = Vec::<i32>::from_nbt(tag).ok()?;
            Some(Uuid::from_int_array(words.get(..4)?.try_into().ok()?))
        });
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Conduit(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if let Some(target) = entity.destroy_target {
            tag.insert("Target", target.to_nbt()?);
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(handle)?.data().block_state.block(),
        ))
    }
}
