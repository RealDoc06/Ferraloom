//! AbstractFurnaceMenu uses retained input/result objects through transfers and
//! the two native achievement checks (onQuickCraft, then onTake).
use super::*;
use ferraloom_protocol::component::ComponentTypeId;

pub(super) fn quick_move_furnace_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    hooks: &mut dyn MenuHooks,
) -> Result<C::Stack> {
    let source = context.slot_stack(index)?;
    if context.stack(source)?.is_empty() {
        return context.own_stack(ItemStack::default());
    }
    if index == 2 {
        if !hooks.result_available(ResultKind::Furnace, &context.stack(source)?)? {
            return context.own_stack(ItemStack::default());
        }
        context.prepare_swap_craft(index, hooks)?;
    }
    let source = context.slot_stack(index)?;
    let original = context.own_stack(context.stack(source)?)?;
    let moved = if index == 2 {
        let moved = move_menu_item_stack_to(context, source, 3, 39, true, hooks)?;
        if moved {
            context.quick_craft_slot(index, source, original, hooks)?;
        }
        moved
    } else if index < 2 {
        move_menu_item_stack_to(context, source, 3, 39, false, hooks)?
    } else if hooks.can_smelt(&context.stack(source)?)? {
        move_menu_item_stack_to(context, source, 0, 1, false, hooks)?
    } else if context
        .stack(source)?
        .component(ComponentTypeId::COOKING_FUEL)?
        .is_some()
    {
        move_menu_item_stack_to(context, source, 1, 2, false, hooks)?
    } else if index < 30 {
        move_menu_item_stack_to(context, source, 30, 39, false, hooks)?
    } else {
        move_menu_item_stack_to(context, source, 3, 30, false, hooks)?
    };
    if !moved {
        return context.own_stack(ItemStack::default());
    }
    if context.stack(source)?.is_empty() {
        let empty = context.own_stack(ItemStack::default())?;
        context.set_slot_stack(index, empty, None, hooks)?;
    } else {
        context.slot_changed(index)?;
    }
    if count(&context.stack(source)?) == count(&context.stack(original)?) {
        return context.own_stack(ItemStack::default());
    }
    context.take_slot_stack(index, source, hooks)?;
    Ok(original)
}
