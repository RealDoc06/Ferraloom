//! One click algorithm over live Level handles or the standalone object arena.
use super::*;
use ferraloom_protocol::play::ContainerInput;

pub trait MenuClickContext: MenuTransferContext {
    fn quick_craft_slot(
        &mut self,
        index: usize,
        picked: Self::Stack,
        original: Self::Stack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()>;
    fn menu(&self) -> Result<&AbstractContainerMenu>;
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu>;
    fn infinite_materials(&self) -> Result<bool>;
    fn carried_stack(&mut self) -> Result<Self::Stack>;
    fn own_stack(&mut self, stack: ItemStack) -> Result<Self::Stack>;
    fn set_carried_stack(&mut self, stack: Self::Stack) -> Result<()>;
    fn inventory_stack(&mut self, slot: usize) -> Result<Self::Stack>;
    fn set_inventory_stack(&mut self, slot: usize, stack: Self::Stack) -> Result<()>;
    fn add_to_inventory(&mut self, stack: Self::Stack) -> Result<bool>;
    fn drop_stack(&mut self, stack: Self::Stack, thrown_from_hand: bool) -> Result<()>;
    /// Validate an unported result effect before the first native SWAP write.
    fn prepare_swap_craft(&self, index: usize, hooks: &dyn MenuHooks) -> Result<()>;
    fn swap_craft(&mut self, index: usize, amount: i32) -> Result<()>;
    fn slot_count(&self) -> Result<usize>;
    fn can_take_for_pick_all(&self, index: usize) -> Result<bool>;
    fn can_drop_items(&self) -> Result<bool>;
    fn clone_slot_stack(&mut self, index: usize, hooks: &mut dyn MenuHooks) -> Result<Self::Stack>;
    fn quick_move_stack(&mut self, index: usize, hooks: &mut dyn MenuHooks) -> Result<Self::Stack>;
    /// The explicit previous receiver covers native Slot#setByPlayer's overload
    /// used by tryRemove, and callback-free swaps whose objects have moved.
    fn set_slot_stack(
        &mut self,
        index: usize,
        stack: Self::Stack,
        previous: Option<Self::Stack>,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()>;
    fn may_pickup(&mut self, index: usize, hooks: &dyn MenuHooks) -> Result<bool>;
    fn remove_slot_stack(
        &mut self,
        index: usize,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<Self::Stack>;
    fn take_slot_stack(
        &mut self,
        index: usize,
        stack: Self::Stack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()>;
}

/// Port of AbstractContainerMenu#doClick. The same dispatcher runs over live
/// positional/generation handles and the standalone retained object context.
pub fn click_menu<C: MenuClickContext>(
    context: &mut C,
    slot: i32,
    button: i32,
    input: ContainerInput,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    if input == ContainerInput::QuickCraft {
        return quick_craft_menu_items(context, slot, button, hooks);
    }
    if context.menu()?.quickcraft_status != 0 {
        context.menu_mut()?.reset_quick_craft();
        return Ok(());
    }
    match input {
        ContainerInput::Pickup | ContainerInput::QuickMove if button == 0 || button == 1 => {
            if slot == -999 {
                drop_carried_menu_item(context, button)?;
            } else if slot >= 0 {
                let index = slot as usize;
                if index >= context.slot_count()? {
                    return Err(MenuError::InvalidSlot(slot));
                }
                if input == ContainerInput::QuickMove {
                    if !context.may_pickup(index, hooks)? {
                        return Ok(());
                    }
                    let mut moved = context.quick_move_stack(index, hooks)?;
                    while !context.stack(moved)?.is_empty() {
                        let current = context.slot_stack(index)?;
                        if !same_item(&context.stack(current)?, &context.stack(moved)?) {
                            break;
                        }
                        moved = context.quick_move_stack(index, hooks)?;
                    }
                } else {
                    pickup_menu_item(context, index, button, hooks)?;
                }
            }
        }
        ContainerInput::Swap if (0..9).contains(&button) || button == 40 => {
            let index = usize::try_from(slot).map_err(|_| MenuError::InvalidSlot(slot))?;
            swap_menu_item(context, index, button as usize, hooks)?;
        }
        ContainerInput::Clone if context.infinite_materials()? => {
            let carried = context.carried_stack()?;
            if context.stack(carried)?.is_empty() && slot >= 0 {
                let index = slot as usize;
                let current = context.slot_stack(index)?;
                if !context.stack(current)?.is_empty() {
                    let cloned = context.clone_slot_stack(index, hooks)?;
                    context.set_carried_stack(cloned)?;
                }
            }
        }
        ContainerInput::Throw => {
            let carried = context.carried_stack()?;
            if context.stack(carried)?.is_empty() && slot >= 0 {
                throw_menu_item(context, slot as usize, button, hooks)?;
            }
        }
        ContainerInput::PickupAll if slot >= 0 => {
            pickup_all_menu_items(context, slot as usize, button, hooks)?;
        }
        _ => {}
    }
    Ok(())
}

/// Live click frame. Individual unported slot/recipe/entity virtuals fail at
/// their own native call sites; handlers do not snapshot menu or player state.
pub fn clicked_live_menu(
    player: crate::entity::EntityRef,
    menu: MenuRef,
    slot: i32,
    button: i32,
    input: ContainerInput,
    hooks: &mut dyn MenuHooks,
    level: &mut crate::level::Level,
) -> Result<()> {
    with_menu_callback(player, menu, level, |context| {
        click_menu(
            &mut super::transfer::LiveMenuTransfer {
                operation: StackOperation::new(context.level),
                player,
                menu,
            },
            slot,
            button,
            input,
            hooks,
        )
    })
}

/// Ports of ChestMenu, HopperMenu, ShulkerBoxMenu and DispenserMenu#quickMoveStack.
/// The returned stack is vanilla's explicit copy; the source remains a retained
/// receiver through transfer callbacks, empty replacement and onTake.
pub fn quick_move_container_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    hooks: &mut dyn MenuHooks,
) -> Result<ItemStack> {
    let kind = context.menu()?.kind;
    let boundary = match kind {
        MenuKind::Chest { rows } => usize::from(rows) * 9,
        MenuKind::Hopper => 5,
        MenuKind::ShulkerBox => 27,
        MenuKind::Dispenser | MenuKind::Crafter => 9,
        _ => return Err(MenuError::Unsupported("retained quick-move menu family")),
    };
    let source = context.slot_stack(index)?;
    if context.stack(source)?.is_empty() {
        return Ok(ItemStack::default());
    }
    let source = context.slot_stack(index)?;
    let original = context.stack(source)?;
    let (start, end, backwards) = if index < boundary {
        (
            boundary,
            if kind == MenuKind::Crafter {
                45
            } else {
                context.slot_count()?
            },
            true,
        )
    } else {
        (0, boundary, false)
    };
    if !move_menu_item_stack_to(context, source, start, end, backwards, hooks)? {
        return Ok(ItemStack::default());
    }
    if context.stack(source)?.is_empty() {
        let empty = context.own_stack(ItemStack::default())?;
        context.set_slot_stack(index, empty, None, hooks)?;
    } else {
        context.slot_changed(index)?;
    }
    if matches!(kind, MenuKind::Dispenser | MenuKind::Crafter) {
        if count(&context.stack(source)?) == count(&original) {
            return Ok(ItemStack::default());
        }
        context.take_slot_stack(index, source, hooks)?;
    }
    Ok(original)
}

/// Live simple-container QUICK_MOVE shares the same source/slot algorithm as
/// the standalone adapter; the menu root survives every synchronous callback.
pub fn quick_move_live_container_menu_item(
    player: crate::entity::EntityRef,
    menu: MenuRef,
    index: usize,
    hooks: &mut dyn MenuHooks,
    level: &mut crate::level::Level,
) -> Result<ItemStack> {
    with_menu_callback(player, menu, level, |context| {
        quick_move_container_menu_item(
            &mut super::transfer::LiveMenuTransfer {
                operation: StackOperation::new(context.level),
                player,
                menu,
            },
            index,
            hooks,
        )
    })
}

/// Port of AbstractContainerMenu#doClick's QUICK_CRAFT state machine. Only the
/// native source copy is new; every current-carried and slot access is live.
pub fn quick_craft_menu_items<C: MenuClickContext>(
    context: &mut C,
    index: i32,
    button: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    let expected = context.menu()?.quickcraft_status;
    let status = button & 3;
    context.menu_mut()?.quickcraft_status = status;
    if (expected != 1 || status != 2) && expected != status {
        context.menu_mut()?.reset_quick_craft();
        return Ok(());
    }
    let carried = context.carried_stack()?;
    if context.stack(carried)?.is_empty() {
        context.menu_mut()?.reset_quick_craft();
        return Ok(());
    }
    match status {
        0 => {
            let kind = (button >> 2) & 3;
            context.menu_mut()?.quickcraft_type = kind;
            if kind == 0 || kind == 1 || kind == 2 && context.infinite_materials()? {
                let menu = context.menu_mut()?;
                menu.quickcraft_status = 1;
                menu.clear_quick_craft_slots();
            } else {
                context.menu_mut()?.reset_quick_craft();
            }
        }
        1 => {
            let index = usize::try_from(index).map_err(|_| MenuError::InvalidSlot(index))?;
            if index >= context.slot_count()? {
                return Err(MenuError::InvalidSlot(index as i32));
            }
            let carried = context.carried_stack()?;
            if can_quick_replace_menu_item(context, index, carried, true)?
                && context.may_place(index, carried, hooks)?
                && (context.menu()?.quickcraft_type == 2
                    || count(&context.stack(carried)?)
                        > context.menu()?.quickcraft_slots.len() as i32)
                && context.menu()?.can_drag_to(&context.menu()?.slots[index])
            {
                let menu = context.menu_mut()?;
                if !menu.quickcraft_slots.contains(&index) {
                    menu.quickcraft_slots.push(index);
                    menu.quickcraft_mod_count = menu.quickcraft_mod_count.wrapping_add(1);
                }
            }
        }
        2 => {
            let size = context.menu()?.quickcraft_slots.len();
            if size == 1 {
                let index = context.menu()?.quickcraft_slots[0];
                context.menu_mut()?.reset_quick_craft();
                let button = context.menu()?.quickcraft_type;
                if button == 0 || button == 1 {
                    pickup_menu_item(context, index, button, hooks)?;
                }
                return Ok(());
            }
            if size != 0 {
                let carried = context.carried_stack()?;
                // AbstractContainerMenu explicitly copies the carried source.
                let source = context.own_stack(context.stack(carried)?)?;
                if context.stack(source)?.is_empty() {
                    context.menu_mut()?.reset_quick_craft();
                    return Ok(());
                }
                let carried = context.carried_stack()?;
                let mut remaining = count(&context.stack(carried)?);
                let expected_mod_count = context.menu()?.quickcraft_mod_count;
                for offset in 0..size {
                    // HashSet's iterator checks on next(), before each entry;
                    // a mutation during the last callback has no next entry.
                    if context.menu()?.quickcraft_mod_count != expected_mod_count {
                        return Err(MenuError::ConcurrentQuickCraftModification);
                    }
                    let index = context.menu()?.quickcraft_slots[offset];
                    let carried = context.carried_stack()?;
                    if can_quick_replace_menu_item(context, index, carried, true)?
                        && context.may_place(index, carried, hooks)?
                        && (context.menu()?.quickcraft_type == 2
                            || count(&context.stack(carried)?)
                                >= context.menu()?.quickcraft_slots.len() as i32)
                        && context.menu()?.can_drag_to(&context.menu()?.slots[index])
                    {
                        let target = context.slot_stack(index)?;
                        let carry = if context.stack(target)?.is_empty() {
                            0
                        } else {
                            let target = context.slot_stack(index)?;
                            count(&context.stack(target)?)
                        };
                        let max = max_stack_size(&context.stack(source)?)
                            .min(context.stack_limit(index, source)?);
                        let place_count = match context.menu()?.quickcraft_type {
                            0 => (count(&context.stack(source)?) as f32
                                / context.menu()?.quickcraft_slots.len() as f32)
                                .floor() as i32,
                            1 => 1,
                            2 => max_stack_size(&context.stack(source)?),
                            _ => count(&context.stack(source)?),
                        };
                        let total = place_count.wrapping_add(carry).min(max);
                        remaining = remaining.wrapping_sub(total.wrapping_sub(carry));
                        let copy = with_count(&context.stack(source)?, total);
                        context.set_by_player(index, copy, hooks)?;
                    }
                }
                context.set_stack_count(source, remaining)?;
                context.set_carried_stack(source)?;
            }
            context.menu_mut()?.reset_quick_craft();
        }
        _ => context.menu_mut()?.reset_quick_craft(),
    }
    Ok(())
}

/// Port of Slot#safeTake. onTake runs before its caller grows carried or drops
/// the returned receiver, and the receiver stays rooted across that callback.
pub fn safe_take_menu_item_stack<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    amount: i32,
    max_amount: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<C::Stack> {
    match try_remove_menu_item_stack(context, index, amount, max_amount, hooks)? {
        Some(taken) => {
            context.take_slot_stack(index, taken, hooks)?;
            Ok(taken)
        }
        None => context.own_stack(ItemStack::default()),
    }
}

/// Port of AbstractContainerMenu#canItemQuickReplace, preserving all three
/// Slot#getItem points instead of caching a receiver across those calls.
pub fn can_quick_replace_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    input: C::Stack,
    ignore_size: bool,
) -> Result<bool> {
    let target = context.slot_stack(index)?;
    let empty = context.stack(target)?.is_empty();
    if !empty {
        let target = context.slot_stack(index)?;
        if is_same_item_same_components(&context.stack(input)?, &context.stack(target)?) {
            let target = context.slot_stack(index)?;
            let added = if ignore_size {
                0
            } else {
                count(&context.stack(input)?)
            };
            return Ok(count(&context.stack(target)?).wrapping_add(added)
                <= max_stack_size(&context.stack(input)?));
        }
    }
    Ok(empty)
}

/// Port of AbstractContainerMenu#doClick's PICKUP_ALL branch. The original
/// carried receiver remains the accumulator if an onTake callback replaces it.
pub fn pickup_all_menu_items<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    button: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    if index >= context.slot_count()? {
        return Err(MenuError::InvalidSlot(index as i32));
    }
    let carried = context.carried_stack()?;
    if context.stack(carried)?.is_empty() {
        return Ok(());
    }
    let clicked = context.slot_stack(index)?;
    if !context.stack(clicked)?.is_empty() && context.may_pickup(index, hooks)? {
        return Ok(());
    }
    let size = context.slot_count()?;
    for pass in 0..2 {
        for offset in 0..size {
            if count(&context.stack(carried)?) >= max_stack_size(&context.stack(carried)?) {
                break;
            }
            let index = if button == 0 {
                offset
            } else {
                size - 1 - offset
            };
            let target = context.slot_stack(index)?;
            if !context.stack(target)?.is_empty()
                && can_quick_replace_menu_item(context, index, carried, true)?
                && context.may_pickup(index, hooks)?
                && context.can_take_for_pick_all(index)?
            {
                let target = context.slot_stack(index)?;
                if pass != 0
                    || count(&context.stack(target)?) != max_stack_size(&context.stack(target)?)
                {
                    let amount = count(&context.stack(target)?);
                    let available = max_stack_size(&context.stack(carried)?)
                        .wrapping_sub(count(&context.stack(carried)?));
                    let taken =
                        safe_take_menu_item_stack(context, index, amount, available, hooks)?;
                    let total =
                        count(&context.stack(carried)?).wrapping_add(count(&context.stack(taken)?));
                    context.set_stack_count(carried, total)?;
                }
            }
        }
    }
    Ok(())
}

/// Port of the outside-slot PICKUP drop. The entity adapter moves Existing at
/// construction; the carried EMPTY assignment follows immediately afterward.
pub fn drop_carried_menu_item<C: MenuClickContext>(context: &mut C, button: i32) -> Result<()> {
    let carried = context.carried_stack()?;
    if context.stack(carried)?.is_empty() {
        return Ok(());
    }
    if button == 0 {
        context.drop_stack(carried, true)?;
        let empty = context.own_stack(ItemStack::default())?;
        context.set_carried_stack(empty)
    } else {
        let split = context.split_stack(carried, 1)?;
        let split = context.own_stack(split)?;
        context.drop_stack(split, true)
    }
}

/// Port of AbstractContainerMenu#doClick's THROW branch. Player's creative-drop
/// callback is source-proven empty on the server; all repeated drops stay here.
pub fn throw_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    button: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    let amount = if button == 0 {
        1
    } else {
        let stack = context.slot_stack(index)?;
        count(&context.stack(stack)?)
    };
    if !context.can_drop_items()? {
        return Ok(());
    }
    let mut dropped = safe_take_menu_item_stack(context, index, amount, i32::MAX, hooks)?;
    context.drop_stack(dropped, true)?;
    if button == 1 {
        while !context.stack(dropped)?.is_empty() {
            let current = context.slot_stack(index)?;
            if !same_item(&context.stack(current)?, &context.stack(dropped)?) {
                break;
            }
            if !context.can_drop_items()? {
                return Ok(());
            }
            dropped = safe_take_menu_item_stack(context, index, amount, i32::MAX, hooks)?;
            context.drop_stack(dropped, true)?;
        }
    }
    Ok(())
}

/// Port of AbstractContainerMenu#doClick's SWAP branch. Both Java receivers
/// remain held through the setters, onTake and possible overflow insertion.
pub fn swap_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    button: usize,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    let source = context.inventory_stack(button)?;
    let target = context.slot_stack(index)?;
    if context.stack(source)?.is_empty() && context.stack(target)?.is_empty() {
        return Ok(());
    }
    if context.stack(source)?.is_empty() {
        if context.may_pickup(index, hooks)? {
            context.prepare_swap_craft(index, hooks)?;
            context.set_inventory_stack(button, target)?;
            context.swap_craft(index, count(&context.stack(target)?))?;
            let empty = context.own_stack(ItemStack::default())?;
            context.set_slot_stack(index, empty, Some(target), hooks)?;
            context.take_slot_stack(index, target, hooks)?;
        }
    } else if context.stack(target)?.is_empty() {
        if context.may_place(index, source, hooks)? {
            let max = context.stack_limit(index, source)?;
            if count(&context.stack(source)?) > max {
                let split = context.split_stack(source, max)?;
                context.set_by_player(index, split, hooks)?;
            } else {
                let empty = context.own_stack(ItemStack::default())?;
                context.set_inventory_stack(button, empty)?;
                context.set_slot_stack(index, source, Some(target), hooks)?;
            }
        }
    } else if context.may_pickup(index, hooks)? && context.may_place(index, source, hooks)? {
        let max = context.stack_limit(index, source)?;
        if count(&context.stack(source)?) > max {
            let split = context.split_stack(source, max)?;
            context.set_by_player(index, split, hooks)?;
            context.take_slot_stack(index, target, hooks)?;
            if !context.add_to_inventory(target)? {
                context.drop_stack(target, true)?;
            }
        } else {
            context.set_inventory_stack(button, target)?;
            context.set_slot_stack(index, source, Some(target), hooks)?;
            context.take_slot_stack(index, target, hooks)?;
        }
    }
    Ok(())
}

/// Port of Slot#tryRemove. The removed object stays rooted through the empty
/// setter and subsequent onTake; a ResultContainer returns its existing object.
pub fn try_remove_menu_item_stack<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    amount: i32,
    max_amount: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<Option<C::Stack>> {
    if !context.may_pickup(index, hooks)? {
        return Ok(None);
    }
    let allowed = if context.may_pickup(index, hooks)? {
        let current = context.slot_stack(index)?;
        context.may_place(index, current, hooks)?
    } else {
        false
    };
    if !allowed {
        let current = context.slot_stack(index)?;
        if max_amount < count(&context.stack(current)?) {
            return Ok(None);
        }
    }
    let removed = context.remove_slot_stack(index, amount.min(max_amount), hooks)?;
    if context.stack(removed)?.is_empty() {
        return Ok(None);
    }
    let current = context.slot_stack(index)?;
    if context.stack(current)?.is_empty() {
        let empty = context.own_stack(ItemStack::default())?;
        context.set_slot_stack(index, empty, Some(removed), hooks)?;
    }
    Ok(Some(removed))
}

/// Port of AbstractContainerMenu#doClick's ordinary PICKUP branch. Both held
/// receivers survive replacement and are re-read after every synchronous call.
pub fn pickup_menu_item<C: MenuClickContext>(
    context: &mut C,
    index: usize,
    button: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    let clicked = context.slot_stack(index)?;
    let carried = context.carried_stack()?;
    // ServerPlayer's tutorial hook inherits Player's empty implementation.
    // Its slot getter argument is still evaluated before that empty call.
    let _tutorial_target = context.slot_stack(index)?;
    // BundleItem is the only 26.3 item overriding these two click virtuals.
    for (reference, stacked_on_other) in [(carried, true), (clicked, false)] {
        let stack = context.stack(reference)?;
        let item = stack.item();
        if item.is_enabled_in_vanilla()
            && if stacked_on_other {
                item.overrides_stacked_on_other()
            } else {
                item.overrides_other_stacked_on_me()
            }
        {
            return Err(MenuError::Unsupported("item inventory click override"));
        }
    }
    if context.stack(clicked)?.is_empty() {
        if !context.stack(carried)?.is_empty() {
            let amount = if button == 0 {
                count(&context.stack(carried)?)
            } else {
                1
            };
            let remainder = safe_insert_menu_item_stack(context, index, carried, amount, hooks)?;
            context.set_carried_stack(remainder)?;
        }
    } else if context.may_pickup(index, hooks)? {
        if context.stack(carried)?.is_empty() {
            let amount = count(&context.stack(clicked)?);
            let amount = if button == 0 {
                amount
            } else {
                amount.wrapping_add(1) / 2
            };
            if let Some(taken) =
                try_remove_menu_item_stack(context, index, amount, i32::MAX, hooks)?
            {
                context.set_carried_stack(taken)?;
                context.take_slot_stack(index, taken, hooks)?;
            }
        } else if context.may_place(index, carried, hooks)? {
            if is_same_item_same_components(&context.stack(clicked)?, &context.stack(carried)?) {
                let amount = if button == 0 {
                    count(&context.stack(carried)?)
                } else {
                    1
                };
                let remainder =
                    safe_insert_menu_item_stack(context, index, carried, amount, hooks)?;
                context.set_carried_stack(remainder)?;
            } else if count(&context.stack(carried)?) <= context.stack_limit(index, carried)? {
                context.set_carried_stack(clicked)?;
                context.set_slot_stack(index, carried, Some(clicked), hooks)?;
            }
        } else if is_same_item_same_components(&context.stack(clicked)?, &context.stack(carried)?) {
            let carried_value = context.stack(carried)?;
            let max = max_stack_size(&carried_value).wrapping_sub(count(&carried_value));
            let amount = count(&context.stack(clicked)?);
            if let Some(taken) = try_remove_menu_item_stack(context, index, amount, max, hooks)? {
                let total =
                    count(&context.stack(carried)?).wrapping_add(count(&context.stack(taken)?));
                context.set_stack_count(carried, total)?;
                context.take_slot_stack(index, taken, hooks)?;
            }
        }
    }
    context.slot_changed(index)
}

impl MenuClickContext for super::transfer::LiveMenuTransfer<'_> {
    fn quick_craft_slot(
        &mut self,
        index: usize,
        picked: StackRef,
        original: StackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let amount = count(&self.stack(original)?) - count(&self.stack(picked)?);
        if amount > 0 {
            if self.slot(index)?.kind == SlotKind::Result(ResultKind::Crafting) {
                let total = self.menu()?.crafting_remove_count.wrapping_add(amount);
                self.menu_mut()?.crafting_remove_count = total;
                check_crafting_take_achievements(self, original, hooks)?;
            } else if self.slot(index)?.kind == SlotKind::Result(ResultKind::Furnace) {
                let total = self.menu()?.furnace_remove_count.wrapping_add(amount);
                self.menu_mut()?.furnace_remove_count = total;
                self.check_furnace_achievements(index, original, hooks)?;
            }
        }
        Ok(())
    }
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        self.operation
            .level
            .entity_mut::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu_mut(self.menu)
    }
    fn infinite_materials(&self) -> Result<bool> {
        Ok(self
            .operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .instabuild())
    }
    fn carried_stack(&mut self) -> Result<StackRef> {
        self.operation.pin(ContainerSlot {
            container: ContainerHandle::MenuCarried {
                player: self.player,
                menu: self.menu,
            },
            slot: 0,
        })
    }
    fn own_stack(&mut self, stack: ItemStack) -> Result<StackRef> {
        self.operation.own(self.player, stack)
    }
    fn set_carried_stack(&mut self, stack: StackRef) -> Result<()> {
        self.operation.level.replace_stack_object(
            ContainerSlot {
                container: ContainerHandle::MenuCarried {
                    player: self.player,
                    menu: self.menu,
                },
                slot: 0,
            },
            StackValue::Existing(stack),
        )
    }
    fn inventory_stack(&mut self, slot: usize) -> Result<StackRef> {
        self.operation.pin(ContainerSlot {
            container: ContainerHandle::PlayerInventory(self.player),
            slot,
        })
    }
    fn set_inventory_stack(&mut self, slot: usize, stack: StackRef) -> Result<()> {
        self.operation.level.replace_stack_object(
            ContainerSlot {
                container: ContainerHandle::PlayerInventory(self.player),
                slot,
            },
            StackValue::Existing(stack),
        )
    }
    fn add_to_inventory(&mut self, stack: StackRef) -> Result<bool> {
        add_item_stack(self.player, stack, self.operation.level)
    }
    fn drop_stack(&mut self, stack: StackRef, thrown_from_hand: bool) -> Result<()> {
        crate::entity::player::drop_item_stack(
            self.player,
            StackValue::Existing(stack),
            thrown_from_hand,
            self.operation.level,
        )?;
        Ok(())
    }
    fn swap_craft(&mut self, index: usize, amount: i32) -> Result<()> {
        if self.slot(index)?.kind == SlotKind::Result(ResultKind::Crafting) {
            let count = self.menu()?.crafting_remove_count.wrapping_add(amount);
            self.menu_mut()?.crafting_remove_count = count;
        } else if matches!(self.slot(index)?.kind, SlotKind::Result(kind) if kind != ResultKind::Furnace)
        {
            return Err(MenuError::Unsupported(
                "live non-crafting result-slot achievements",
            ));
        }
        Ok(())
    }
    fn prepare_swap_craft(&self, index: usize, hooks: &dyn MenuHooks) -> Result<()> {
        if let SlotKind::Result(kind) = self.slot(index)?.kind {
            hooks.validate_live_result(kind)?;
        }
        Ok(())
    }
    fn slot_count(&self) -> Result<usize> {
        Ok(self
            .operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)?
            .slots
            .len())
    }
    fn can_take_for_pick_all(&self, index: usize) -> Result<bool> {
        let menu = self
            .operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)?;
        Ok(menu.can_take_for_pick_all(&self.slot(index)?))
    }
    fn can_drop_items(&self) -> Result<bool> {
        // ServerPlayer inherits Player#canDropItems's true implementation.
        Ok(true)
    }
    fn clone_slot_stack(&mut self, index: usize, _: &mut dyn MenuHooks) -> Result<StackRef> {
        let source = self.slot_stack(index)?;
        let copy = {
            let source = self.operation.level.stack(source)?;
            with_count(source, max_stack_size(source))
        };
        let copy = self.operation.own(self.player, copy)?;
        if self.slot(index)?.kind == SlotKind::Result(ResultKind::Crafting) {
            crafted_item_post_process(self.operation.level, copy)?;
        }
        Ok(copy)
    }
    fn quick_move_stack(&mut self, index: usize, hooks: &mut dyn MenuHooks) -> Result<StackRef> {
        if matches!(self.menu()?.kind, MenuKind::Inventory | MenuKind::Crafting) {
            return super::quick_move_crafting::quick_move_crafting_menu_item(self, index, hooks);
        }
        if matches!(
            self.menu()?.kind,
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace
        ) {
            return super::quick_move_furnace::quick_move_furnace_menu_item(self, index, hooks);
        }
        let copy = quick_move_container_menu_item(self, index, hooks)?;
        self.operation.own(self.player, copy)
    }
    fn set_slot_stack(
        &mut self,
        index: usize,
        stack: StackRef,
        previous: Option<StackRef>,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.set_by_player_value(index, StackValue::Existing(stack), previous, hooks)
    }
    fn may_pickup(&mut self, index: usize, hooks: &dyn MenuHooks) -> Result<bool> {
        let slot = self.slot(index)?;
        let creative = self
            .operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .game_mode
            == crate::entity::player::GameType::Creative;
        if matches!(slot.kind, SlotKind::Armor(_) | SlotKind::Result(_)) {
            let stack = self.slot_stack(index)?;
            slot.may_pickup(self.operation.level.stack(stack)?, creative, hooks)
        } else {
            slot.may_pickup(&ItemStack::default(), creative, hooks)
        }
    }
    fn remove_slot_stack(
        &mut self,
        index: usize,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<StackRef> {
        let (slot, address) = self.validate_storage_mutation(index)?;
        if let SlotKind::Result(kind) = slot.kind {
            hooks.validate_live_result(kind)?;
            if !matches!(kind, ResultKind::Crafting | ResultKind::Furnace) {
                return Err(MenuError::Unsupported(
                    "live non-crafting result-slot achievements",
                ));
            }
            let current = self.slot_stack(index)?;
            if !self.operation.level.stack(current)?.is_empty() {
                let current = self.slot_stack(index)?;
                let taken = amount.min(count(self.operation.level.stack(current)?));
                if kind == ResultKind::Crafting {
                    let total = self.menu()?.crafting_remove_count.wrapping_add(taken);
                    self.menu_mut()?.crafting_remove_count = total;
                } else {
                    let total = self.menu()?.furnace_remove_count.wrapping_add(taken);
                    self.menu_mut()?.furnace_remove_count = total;
                }
            }
            if kind == ResultKind::Crafting {
                let result = self.operation.pin(address)?;
                self.operation
                    .level
                    .replace_stack_object(address, StackValue::New(ItemStack::default()))?;
                return Ok(result);
            }
        }
        let removed = self.context().remove_item(address, amount)?;
        let changed = !removed.is_empty();
        let removed = self.operation.own(self.player, removed)?;
        if changed
            && slot.container == ContainerId::Local(0)
            && self.menu()?.crafting_grid_size().is_some()
        {
            hooks.live_grid_changed(self.player, self.menu, self.operation.level)?;
        }
        Ok(removed)
    }
    fn take_slot_stack(
        &mut self,
        index: usize,
        stack: StackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slot(index)?;
        if slot.kind == SlotKind::Result(ResultKind::Crafting) {
            return take_crafting_result_stack(self, stack, hooks);
        }
        if slot.kind == SlotKind::Result(ResultKind::Furnace) {
            self.check_furnace_achievements(index, stack, hooks)?;
            return self.slot_changed(index);
        }
        if matches!(slot.kind, SlotKind::Result(_)) {
            return Err(MenuError::Unsupported("live result-slot achievements"));
        }
        self.slot_changed(index)
    }
}

/// Live PICKUP entry over the shared click kernel. The surrounding frame keeps
/// menu, container and stack generations rooted across each world callback.
pub fn pickup_live_menu_item(
    player: crate::entity::EntityRef,
    menu: MenuRef,
    index: usize,
    button: i32,
    hooks: &mut dyn MenuHooks,
    level: &mut crate::level::Level,
) -> Result<()> {
    with_menu_callback(player, menu, level, |context| {
        let mut context = super::transfer::LiveMenuTransfer {
            operation: StackOperation::new(context.level),
            player,
            menu,
        };
        pickup_menu_item(&mut context, index, button, hooks)
    })
}

#[cfg(test)]
mod tests;
