//! Canonical creaking-heart data. Protector resolution/spawning and its ticker
//! remain required Unsupported until the creaking mob behavior is ported.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::properties as p;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{Uuid, Vec3};

/// Port of CreakingHeartBlockEntity. `creaking_uuid` is the unresolved UUID arm
/// of native creakingInfo; resolving it to a live Creaking is not yet ported.
/// The counters, emitter target and output signal are transient.
#[derive(Clone, Debug)]
pub struct CreakingHeartBlockEntity {
    pub parent: BlockEntityData,
    pub creaking_uuid: Option<Uuid>,
    pub ticks_existed: i64,
    pub ticker: i32,
    pub emitter: i32,
    pub emitter_target: Option<Vec3>,
    pub output_signal: i32,
}
impl CreakingHeartBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            creaking_uuid: None,
            ticks_existed: 0,
            ticker: 0,
            emitter: 0,
            emitter_target: None,
            output_signal: 0,
        }
    }
    /// Port of CreakingHeartBlockEntity#loadAdditional, setCreakingInfo(UUID)
    /// and clearCreakingInfo. The live dispatcher performs their setChanged.
    fn load(&mut self, tag: &CompoundTag) {
        self.creaking_uuid = tag.get("creaking").and_then(|tag| Uuid::from_nbt(tag).ok());
        if self.creaking_uuid.is_some() {
            self.ticks_existed = 0;
        }
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        if let Some(uuid) = self.creaking_uuid {
            tag.insert("creaking", Tag::IntArray(uuid.to_int_array().to_vec()));
        }
        tag
    }
    /// Port of CreakingHeartBlock#getTicker's server-side state gate.
    pub fn has_server_ticker(&self) -> bool {
        self.parent
            .block_state
            .get(p::CREAKING_HEART_STATE)
            .expect("creaking heart state")
            != p::CreakingHeartState::Uprooted
    }
}

pub(super) struct CreakingHeartBehavior;
pub(super) static CREAKING_HEART_ENTITY: CreakingHeartBehavior = CreakingHeartBehavior;
impl BlockEntity for CreakingHeartBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::CreakingHeart(entity) = state else {
            unreachable!()
        };
        entity.load(tag);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        self.load_additional(level.block_entity_state_mut(handle)?, tag);
        handle.set_changed(level)
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::CreakingHeart(entity) = state else {
            unreachable!()
        };
        Ok(entity.save())
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        state.save_custom_only()
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn tick(&self, handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(handle)?.data().block_state.block(),
        ))
    }
}

#[cfg(test)]
mod tests;
