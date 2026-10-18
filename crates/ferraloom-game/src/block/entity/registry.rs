//! Port of BlockEntityTypes and BlockEntityType#isValid.
use ferraloom_data::{Block, BlockState, registries::block_entity_type};
use std::sync::OnceLock;

/// Vanilla registry identity; validity is separate from factory availability.
pub type BlockEntityType = block_entity_type::Id;

struct Registry {
    valid_blocks: Vec<Vec<Block>>,
    by_block: Vec<Option<BlockEntityType>>,
}
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut valid_blocks = vec![Vec::new(); block_entity_type::ALL.len()];
        let mut by_block = vec![None; ferraloom_data::registries::block::ALL.len()];
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv").lines() {
            let mut fields = line.split('\t');
            if fields.next() != Some("registry") {
                continue;
            }
            let kind = BlockEntityType::from_name(fields.next().expect("type name"))
                .expect("vanilla block entity type");
            for name in fields.next().expect("valid blocks").split(',') {
                let block = Block::from_name(name).expect("vanilla block");
                valid_blocks[kind.protocol_id() as usize].push(block);
                by_block[block.protocol_id() as usize] = Some(kind);
            }
        }
        Registry {
            valid_blocks,
            by_block,
        }
    })
}
pub fn valid_blocks(kind: BlockEntityType) -> &'static [Block] {
    &registry().valid_blocks[kind.protocol_id() as usize]
}
pub fn is_valid(kind: BlockEntityType, state: BlockState) -> bool {
    for_block(state.block()) == Some(kind)
}
pub fn for_block(block: Block) -> Option<BlockEntityType> {
    registry().by_block[block.protocol_id() as usize]
}
/// Port of BlockEntityTypes.OP_ONLY_CUSTOM_DATA.
pub fn only_op_can_set_nbt(kind: BlockEntityType) -> bool {
    matches!(
        kind,
        BlockEntityType::COMMAND_BLOCK
            | BlockEntityType::LECTERN
            | BlockEntityType::SIGN
            | BlockEntityType::HANGING_SIGN
            | BlockEntityType::MOB_SPAWNER
            | BlockEntityType::TRIAL_SPAWNER
    )
}
