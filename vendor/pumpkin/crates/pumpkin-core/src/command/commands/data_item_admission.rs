//! Item fields consumed by /data's live loaders and /summon's entity loader.
//! Command storage and arbitrary compounds inside custom data remain opaque.

use crate::{
    block::entities::BlockEntity,
    command::errors::{
        command_syntax_error::CommandSyntaxError, error_types::LiteralCommandErrorType,
    },
    entity::EntityBase,
};
use pumpkin_data::{
    data_component_impl::EquipmentSlot,
    item_stack::{ItemStack, admission::validate_runtime_item_stack},
};
use pumpkin_nbt::compound::NbtCompound;

const ERROR: LiteralCommandErrorType =
    LiteralCommandErrorType::new("This item cannot currently be persisted without data loss.");

fn item(nbt: &NbtCompound) -> Result<(), CommandSyntaxError> {
    // Match the consumer's treatment of absent/unreadable items. This gate
    // checks the runtime persistence projection, not raw NBT fidelity. In
    // particular, accepted namespace aliases must not become new refusals.
    let Some(stack) = ItemStack::read_item_stack(nbt) else {
        return Ok(());
    };
    validate_runtime_item_stack(&stack).map_err(|_| ERROR.create_without_context())
}

fn list(nbt: &NbtCompound, key: &str, slots: Option<usize>) -> Result<(), CommandSyntaxError> {
    for entry in nbt.get_list(key).unwrap_or_default() {
        if let Some(entry) = entry.extract_compound() {
            if let Some(size) = slots
                && !entry
                    .get_byte("Slot")
                    .is_some_and(|slot| (slot as usize) < size)
            {
                continue;
            }
            item(entry)?;
        }
    }
    Ok(())
}

pub(super) fn block(block: &dyn BlockEntity, nbt: &NbtCompound) -> Result<(), CommandSyntaxError> {
    // Only chest-family entities implement item-bearing in-place /data edits.
    // Their loader ignores Items when it has a valid deferred loot-table key.
    if matches!(
        block.resource_location(),
        "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel"
    ) && !nbt
        .get_string("LootTable")
        .is_some_and(|key| pumpkin_util::identifier::Identifier::parse(key).is_ok())
    {
        list(nbt, "Items", Some(27))?;
    }
    Ok(())
}

pub(in crate::command::commands) fn entity(
    entity: &dyn EntityBase,
    nbt: &NbtCompound,
) -> Result<(), CommandSyntaxError> {
    if entity.get_living_entity().is_some()
        && let Some(equipment) = nbt.get_compound("equipment")
    {
        for (name, value) in &equipment.child_tags {
            if EquipmentSlot::get_from_name(name).is_some()
                && let Some(value) = value.extract_compound()
            {
                item(value)?;
            }
        }
    }
    let kind = entity.get_entity().entity_type.resource_name;
    let single = match kind {
        "item" | "item_frame" | "glow_item_frame" => Some("Item"),
        _ => None,
    };
    if let Some(value) = single.and_then(|key| nbt.get_compound(key)) {
        item(value)?;
    }
    // ArrowEntity loads its payload with count one, including a supplied zero.
    if matches!(kind, "arrow" | "spectral_arrow")
        && let Some(stack) = nbt
            .get_compound("item")
            .and_then(ItemStack::read_item_stack)
    {
        validate_runtime_item_stack(&stack.copy_with_count(1))
            .map_err(|_| ERROR.create_without_context())?;
    }
    if matches!(kind, "villager" | "piglin" | "pillager") {
        list(nbt, "Inventory", None)?;
    }
    if matches!(kind, "chest_minecart" | "hopper_minecart") && nbt.get_string("LootTable").is_none()
    {
        list(
            nbt,
            "Items",
            Some(if kind == "hopper_minecart" { 5 } else { 27 }),
        )?;
    }
    if matches!(kind, "villager" | "wandering_trader")
        && let Some(recipes) = nbt
            .get_compound("Offers")
            .and_then(|offers| offers.get_list("Recipes"))
    {
        for recipe in recipes {
            if let Some(recipe) = recipe.extract_compound() {
                let buy = recipe
                    .get_compound("buy")
                    .and_then(ItemStack::read_item_stack);
                let buy_b = recipe
                    .get_compound("buyB")
                    .and_then(ItemStack::read_item_stack);
                let sell = recipe
                    .get_compound("sell")
                    .and_then(ItemStack::read_item_stack);
                // Merchant readers ignore recipes without a usable pair.
                if let (Some(buy), Some(sell)) = (buy, sell)
                    && !buy.is_empty()
                    && !sell.is_empty()
                    && buy_b.as_ref().is_none_or(|stack| !stack.is_empty())
                {
                    for stack in [&buy, &sell].into_iter().chain(buy_b.as_ref()) {
                        validate_runtime_item_stack(stack)
                            .map_err(|_| ERROR.create_without_context())?;
                    }
                }
            }
        }
    }
    Ok(())
}
