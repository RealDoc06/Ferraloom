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
