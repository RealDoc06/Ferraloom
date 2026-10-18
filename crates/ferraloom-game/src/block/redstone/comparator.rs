//! Port of ComparatorBlockEntity's persisted output signal.
use crate::block::entity::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::{CompoundTag, Tag};

#[derive(Clone, Debug)]
pub struct ComparatorBlockEntity {
    pub parent: BlockEntityData,
    pub output: i32,
}
impl ComparatorBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self { parent, output: 0 }
    }
}

pub struct ComparatorEntityBehavior;
pub static COMPARATOR_ENTITY: ComparatorEntityBehavior = ComparatorEntityBehavior;
impl BlockEntity for ComparatorEntityBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        if let Some(entity) =
            <ComparatorBlockEntity as crate::block::entity::BlockEntityDataView>::view_mut(state)
        {
            entity.output = tag.get_int_or("OutputSignal", 0);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = CompoundTag::new();
        if let Some(entity) =
            <ComparatorBlockEntity as crate::block::entity::BlockEntityDataView>::view(state)
        {
            tag.insert("OutputSignal", Tag::Int(entity.output));
        }
        Ok(tag)
    }
}
