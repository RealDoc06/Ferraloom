//! Port of BlockEntityTypes and BlockEntityType#isValid using generated facts.
use ferraloom_data::{Block, BlockState};
pub type BlockEntityType = ferraloom_data::registries::block_entity_type::Id;
pub fn valid_blocks(kind: BlockEntityType) -> &'static [Block] {
    kind.valid_blocks()
}
pub fn is_valid(kind: BlockEntityType, state: BlockState) -> bool {
    kind.is_valid(state)
}
pub fn for_block(block: Block) -> Option<BlockEntityType> {
    block.block_entity_type()
}
pub fn only_op_can_set_nbt(kind: BlockEntityType) -> bool {
    kind.only_op_can_set_nbt()
}

/// Port of the two BlockBehaviour#shouldChangedStateKeepBlockEntity overrides.
/// Tag membership is read from the live datapack snapshot, never extracted as
/// a fixed set: resource packs may extend copper chest/statue membership.
pub fn should_changed_state_keep_block_entity(
    new_state: BlockState,
    old_state: BlockState,
    tags: &ferraloom_data::datapack::TagSet<Block>,
) -> bool {
    use ferraloom_data::{BlockClass, tags::BlockTags};
    match new_state.block().class() {
        BlockClass::CopperChestBlock | BlockClass::WeatheringCopperChestBlock => {
            tags.contains_key(BlockTags::COPPER_CHESTS, old_state.block())
        }
        BlockClass::CopperGolemStatueBlock | BlockClass::WeatheringCopperGolemStatueBlock => {
            tags.contains_key(BlockTags::COPPER_GOLEM_STATUES, old_state.block())
        }
        _ => false,
    }
}
