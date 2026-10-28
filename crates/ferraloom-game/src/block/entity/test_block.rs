//! Test-block receiver data. Trigger execution belongs to the GameTest runtime.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_data::properties as p;
use ferraloom_nbt::{CompoundTag, NbtString, Tag};
use ferraloom_protocol::play::TestBlockMode;

/// Port of TestBlockEntity. `triggered` is transient and survives loadAdditional.
#[derive(Clone, Debug)]
pub struct TestBlockEntity {
    pub parent: BlockEntityData,
    pub mode: TestBlockMode,
    pub message: NbtString,
    pub powered: bool,
    pub triggered: bool,
}
impl TestBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        let mode = match parent.block_state.get(p::MODE).expect("test-block mode") {
            p::Mode::Start => TestBlockMode::Start,
            p::Mode::Log => TestBlockMode::Log,
            p::Mode::Accept => TestBlockMode::Accept,
            p::Mode::Fail => TestBlockMode::Fail,
            _ => unreachable!("test-block mode"),
        };
        Self {
            parent,
            mode,
            message: "".into(),
            powered: false,
            triggered: false,
        }
    }
    fn load(&mut self, tag: &CompoundTag) {
        // loadAdditional's codec default is FAIL, independently of the block state.
        self.mode = match tag
            .get_string("mode")
            .and_then(|s| s.to_string_checked().ok())
            .as_deref()
        {
            Some("start") => TestBlockMode::Start,
            Some("log") => TestBlockMode::Log,
            Some("accept") => TestBlockMode::Accept,
            _ => TestBlockMode::Fail,
        };
        self.message = tag.get_string("message").cloned().unwrap_or_default();
        self.powered = tag.get_boolean_or("powered", false);
    }
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            TestBlockMode::Start => "start",
            TestBlockMode::Log => "log",
            TestBlockMode::Fail => "fail",
            TestBlockMode::Accept => "accept",
        }
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert("mode", Tag::String(self.mode_name().into()));
        tag.insert("message", Tag::String(self.message.clone()));
        tag.insert("powered", Tag::Byte(i8::from(self.powered)));
        tag
    }
}
pub(super) struct TestBlockBehavior;
pub(super) static TEST_BLOCK_ENTITY: TestBlockBehavior = TestBlockBehavior;
impl BlockEntity for TestBlockBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::TestBlock(entity) = state else {
            unreachable!()
        };
        entity.load(tag);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::TestBlock(entity) = state else {
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
}

#[cfg(test)]
mod tests;
