//! Typed CriteriaTriggers seam. The advancements port will evaluate listeners
//! here. Until then each trigger is a whole effect under UnportedBehavior;
//! skipping a trigger never substitutes a predicate result or skips its caller.
use crate::{
    block::UnportedOperation,
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_protocol::Identifier;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::BlockPos;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Trigger {
    GenerateLoot,
    ItemUsedOnBlock,
    DefaultBlockUse,
    AnyBlockUse,
}
impl Trigger {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GenerateLoot => "generate_loot",
            Self::ItemUsedOnBlock => "item_used_on_block",
            Self::DefaultBlockUse => "default_block_use",
            Self::AnyBlockUse => "any_block_use",
        }
    }
}

/// Port of CriteriaTriggers.ITEM_USED_ON_BLOCK.trigger, with the pre-use stack.
pub fn item_used_on_block(
    level: &mut Level,
    _player: EntityRef,
    _pos: BlockPos,
    _stack: &ItemStack,
) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(
        Trigger::ItemUsedOnBlock,
    ))
}
/// Port of CriteriaTriggers.DEFAULT_BLOCK_USE.trigger.
pub fn default_block_use(
    level: &mut Level,
    _player: EntityRef,
    _pos: BlockPos,
) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(
        Trigger::DefaultBlockUse,
    ))
}
/// Port of CriteriaTriggers.ANY_BLOCK_USE.trigger, after a consumed interaction.
pub fn any_block_use(
    level: &mut Level,
    _player: EntityRef,
    _pos: BlockPos,
    _stack: &ItemStack,
) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(
        Trigger::AnyBlockUse,
    ))
}

/// Port of CriteriaTriggers.GENERATE_LOOT.trigger, before the table is cleared.
pub fn generate_loot(
    level: &mut Level,
    _player: EntityRef,
    _table: &Identifier,
) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(
        Trigger::GenerateLoot,
    ))
}
