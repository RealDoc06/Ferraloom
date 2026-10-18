//! One click algorithm over live Level handles or the standalone object arena.
use super::*;

pub trait MenuClickContext: MenuTransferContext {
    fn carried_stack(&mut self) -> Result<Self::Stack>;
    fn own_stack(&mut self, stack: ItemStack) -> Result<Self::Stack>;
    fn set_carried_stack(&mut self, stack: Self::Stack) -> Result<()>;
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
    fn set_slot_stack(
        &mut self,
        index: usize,
        stack: StackRef,
        previous: Option<StackRef>,
        _: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.set_by_player_value(index, StackValue::Existing(stack), previous)
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
        _: &mut dyn MenuHooks,
    ) -> Result<StackRef> {
        let (slot, address) = self.validate_storage_mutation(index)?;
        if matches!(slot.kind, SlotKind::Result(_)) {
            return Err(MenuError::Unsupported("live result-slot achievements"));
        }
        let removed = self.context().remove_item(address, amount)?;
        self.operation.own(self.player, removed)
    }
    fn take_slot_stack(&mut self, index: usize, _: StackRef, _: &mut dyn MenuHooks) -> Result<()> {
        let slot = self.slot(index)?;
        if matches!(slot.kind, SlotKind::Result(_)) {
            return Err(MenuError::Unsupported("live result-slot achievements"));
        }
        let mut context = self.context();
        let address = context.slot_address(index)?;
        context.set_changed(address.container)
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
