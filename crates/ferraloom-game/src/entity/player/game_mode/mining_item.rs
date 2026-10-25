//! The ItemStack/Item mining path; full durability and enchantment effects are
//! supplied by the shared item owner at the existing hurtAndBreak effect seam.
use super::{BlockBreakError, inventory};
use crate::{
    block::UnportedOperation,
    entity::{EntityRef, player::award_stat},
    inventory::StackRef,
    level::Level,
    stats::Stat,
};
use ferraloom_data::{
    BlockState, Item, registries::data_component_type::Id as ComponentType, tags::BlockTags,
};
use ferraloom_protocol::component::{DataComponent, EquipmentSlot};

/// Port of ItemStack#mineBlock, Item#mineBlock and ShearsItem#mineBlock. The
/// caller roots this exact receiver across wear, criteria and stat callbacks;
/// ITEM_USED uses the item captured before wear could empty or replace it.
pub fn mine_block(
    player: EntityRef,
    receiver: StackRef,
    state: BlockState,
    level: &mut Level,
) -> Result<(), BlockBreakError> {
    let stack = level.stack(receiver)?;
    let used_item = stack.item();
    let Some(DataComponent::Tool(tool)) = inventory::component(stack, ComponentType::TOOL) else {
        return Ok(());
    };
    let damage = tool.damage_per_block.0;
    let damages_block = if used_item == Item::SHEARS {
        !level
            .block_tags()?
            .contains_key(BlockTags::FIRE, state.block())
    } else {
        state.data().destroy_time != 0.0
    };
    if damages_block && damage > 0 {
        level.unported_operation(UnportedOperation::ItemHurtAndBreak {
            item: used_item,
            amount: damage,
            slot: EquipmentSlot::Mainhand,
        })?;
    }
    award_stat(player, Stat::Used(used_item), 1, level)?;
    Ok(())
}
