//! InventoryMenu and CraftingMenu route retained sources through the same transfer
//! kernel. Native explicit copies are rooted too, including onQuickCraft's input.
use super::*;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, EquipmentSlot};

/// Ports of InventoryMenu/CraftingMenu#quickMoveStack.
pub(super) fn quick_move_crafting_menu_item<C: CraftingResultContext>(
    context: &mut C,
    index: usize,
    hooks: &mut dyn MenuHooks,
) -> Result<C::Stack> {
    let kind = context.menu()?.kind;
    let empty = |context: &mut C| context.own_stack(ItemStack::default());
    let source = context.slot_stack(index)?;
    if context.stack(source)?.is_empty() {
        return empty(context);
    }
    if index == 0 {
        context.validate_crafting_take(hooks)?;
    }
    let source = context.slot_stack(index)?;
    let original = context.own_stack(context.stack(source)?)?;
    let boundary = if kind == MenuKind::Inventory { 9 } else { 10 };
    let moved = if index == 0 {
        if kind == MenuKind::Crafting {
            context.crafted_item_only(source, hooks)?;
        }
        let moved = move_menu_item_stack_to(context, source, boundary, boundary + 36, true, hooks)?;
        if moved {
            context.quick_craft_slot(index, source, original, hooks)?;
        }
        moved
    } else if kind == MenuKind::Inventory {
        if index < 9 {
            move_menu_item_stack_to(context, source, 9, 45, false, hooks)?
        } else {
            let equipment = match crate::entity::player::inventory::component(
                &context.stack(original)?,
                ComponentTypeId::EQUIPPABLE,
            ) {
                Some(DataComponent::Equippable(value)) => value.slot,
                _ => EquipmentSlot::Mainhand,
            };
            let equipment_index = match equipment {
                EquipmentSlot::Feet => Some(8),
                EquipmentSlot::Legs => Some(7),
                EquipmentSlot::Chest => Some(6),
                EquipmentSlot::Head => Some(5),
                EquipmentSlot::Offhand => Some(45),
                _ => None,
            };
            let target = match equipment_index {
                Some(target) => {
                    let current = context.slot_stack(target)?;
                    context.stack(current)?.is_empty().then_some(target)
                }
                None => None,
            };
            if let Some(target) = target {
                move_menu_item_stack_to(context, source, target, target + 1, false, hooks)?
            } else if index < 36 {
                move_menu_item_stack_to(context, source, 36, 45, false, hooks)?
            } else if index < 45 {
                move_menu_item_stack_to(context, source, 9, 36, false, hooks)?
            } else {
                move_menu_item_stack_to(context, source, 9, 45, false, hooks)?
            }
        }
    } else if (10..46).contains(&index) {
        move_menu_item_stack_to(context, source, 1, 10, false, hooks)?
            || if index < 37 {
                move_menu_item_stack_to(context, source, 37, 46, false, hooks)?
            } else {
                move_menu_item_stack_to(context, source, 10, 37, false, hooks)?
            }
    } else {
        move_menu_item_stack_to(context, source, 10, 46, false, hooks)?
    };
    if !moved {
        return empty(context);
    }
    if context.stack(source)?.is_empty() {
        let new = empty(context)?;
        let previous = (kind == MenuKind::Inventory).then_some(original);
        context.set_slot_stack(index, new, previous, hooks)?;
    } else {
        context.slot_changed(index)?;
    }
    if count(&context.stack(source)?) == count(&context.stack(original)?) {
        return empty(context);
    }
    context.take_slot_stack(index, source, hooks)?;
    if index == 0 {
        context.drop_stack(source, false)?;
    }
    Ok(original)
}
