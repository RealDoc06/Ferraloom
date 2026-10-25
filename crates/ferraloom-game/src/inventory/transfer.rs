//! AbstractContainerMenu transfers hold stack identities while each container
//! operation finishes its callbacks. Read values never get written back.
use super::*;

pub trait MenuTransferContext {
    type Stack: Copy;

    /// Slot#getItem, including lazy container work, then retain its receiver.
    fn slot_stack(&mut self, index: usize) -> Result<Self::Stack>;
    fn stack(&self, reference: Self::Stack) -> Result<ItemStack>;
    /// ItemStack#setCount on that exact receiver, with no container callback.
    fn set_stack_count(&mut self, reference: Self::Stack, count: i32) -> Result<()>;
    fn split_stack(&mut self, reference: Self::Stack, amount: i32) -> Result<ItemStack>;
    fn stack_limit(&mut self, index: usize, stack: Self::Stack) -> Result<i32>;
    fn may_place(
        &mut self,
        index: usize,
        stack: Self::Stack,
        hooks: &dyn MenuHooks,
    ) -> Result<bool>;
    /// Slot#setByPlayer: read previous, setItem, then Slot#setChanged.
    fn set_by_player(
        &mut self,
        index: usize,
        stack: ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()>;
    /// Slot#setByPlayer with its captured existing target, preserving identity.
    fn reassign_by_player(
        &mut self,
        _index: usize,
        _stack: Self::Stack,
        _hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        Err(MenuError::Unsupported("existing slot receiver setter"))
    }
    fn slot_changed(&mut self, index: usize) -> Result<()>;
}

/// Port of Slot#safeInsert. Count edits finish before the existing-target setter
/// and its callbacks; doClick receives the original input object's identity.
pub fn safe_insert_menu_item_stack<C: MenuTransferContext>(
    context: &mut C,
    index: usize,
    input: C::Stack,
    amount: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<C::Stack> {
    if context.stack(input)?.is_empty() || !context.may_place(index, input, hooks)? {
        return Ok(input);
    }
    let target = context.slot_stack(index)?;
    let transferable = amount.min(count(&context.stack(input)?));
    let available = context
        .stack_limit(index, input)?
        .wrapping_sub(count(&context.stack(target)?));
    let transferable = transferable.min(available);
    if transferable <= 0 {
        return Ok(input);
    }
    if context.stack(target)?.is_empty() {
        let split = context.split_stack(input, transferable)?;
        context.set_by_player(index, split, hooks)?;
    } else if is_same_item_same_components(&context.stack(target)?, &context.stack(input)?) {
        let remaining = count(&context.stack(input)?).wrapping_sub(transferable);
        context.set_stack_count(input, remaining)?;
        let total = count(&context.stack(target)?).wrapping_add(transferable);
        context.set_stack_count(target, total)?;
        context.reassign_by_player(index, target, hooks)?;
    }
    Ok(input)
}

/// Legacy Slot#safeInsert remains an adapter over the shared insertion kernel.
pub(crate) fn safe_insert_container_item_stack(
    slot: &Slot,
    container: &mut dyn Container,
    mut input: ItemStack,
    amount: i32,
    hooks: &dyn MenuHooks,
) -> Result<ItemStack> {
    struct PlacementHooks<'a>(&'a dyn MenuHooks);
    impl MenuHooks for PlacementHooks<'_> {
        fn may_place(&self, kind: SlotKind, stack: &ItemStack) -> Result<bool> {
            self.0.may_place(kind, stack)
        }
    }
    struct Insertion<'a, 'b> {
        slot: &'a Slot,
        container: &'b mut dyn Container,
        input: &'b mut ItemStack,
    }
    impl MenuTransferContext for Insertion<'_, '_> {
        type Stack = PureStackRef;
        fn slot_stack(&mut self, index: usize) -> Result<PureStackRef> {
            Ok(PureStackRef::Slot(index))
        }
        fn stack(&self, reference: PureStackRef) -> Result<ItemStack> {
            Ok(match reference {
                PureStackRef::Source => self.input.clone(),
                PureStackRef::Slot(_) => self.container.get_item(self.slot.container_slot),
            })
        }
        fn set_stack_count(&mut self, reference: PureStackRef, amount: i32) -> Result<()> {
            match reference {
                PureStackRef::Source => {
                    set_count(self.input, amount);
                    Ok(())
                }
                PureStackRef::Slot(_) => self
                    .container
                    .set_item_count(self.slot.container_slot, amount),
            }
        }
        fn split_stack(&mut self, reference: PureStackRef, amount: i32) -> Result<ItemStack> {
            match reference {
                PureStackRef::Source => Ok(split(self.input, amount)),
                PureStackRef::Slot(_) => Err(MenuError::Unsupported("split insertion target")),
            }
        }
        fn stack_limit(&mut self, _: usize, reference: PureStackRef) -> Result<i32> {
            Ok(self
                .slot
                .max_stack_size(self.container, &self.stack(reference)?))
        }
        fn may_place(
            &mut self,
            _: usize,
            reference: PureStackRef,
            hooks: &dyn MenuHooks,
        ) -> Result<bool> {
            self.slot.may_place(&self.stack(reference)?, hooks)
        }
        fn set_by_player(
            &mut self,
            _: usize,
            stack: ItemStack,
            _: &mut dyn MenuHooks,
        ) -> Result<()> {
            let _previous = self.container.get_item(self.slot.container_slot);
            self.slot.set_by_player(self.container, stack);
            Ok(())
        }
        fn reassign_by_player(
            &mut self,
            _: usize,
            _: PureStackRef,
            _: &mut dyn MenuHooks,
        ) -> Result<()> {
            let _previous = self.container.get_item(self.slot.container_slot);
            self.container.reassign_item(self.slot.container_slot)?;
            self.container.set_changed();
            Ok(())
        }
        fn slot_changed(&mut self, _: usize) -> Result<()> {
            self.container.set_changed();
            Ok(())
        }
    }
    safe_insert_menu_item_stack(
        &mut Insertion {
            slot,
            container,
            input: &mut input,
        },
        0,
        PureStackRef::Source,
        amount,
        &mut PlacementHooks(hooks),
    )?;
    Ok(input)
}

/// Port of AbstractContainerMenu#moveItemStackTo. The occupied pass deliberately
/// bypasses mayPlace. Source and target mutations precede each setChanged call.
pub fn move_menu_item_stack_to<C: MenuTransferContext>(
    context: &mut C,
    source: C::Stack,
    start: usize,
    end: usize,
    backwards: bool,
    hooks: &mut dyn MenuHooks,
) -> Result<bool> {
    let indices = || {
        (0..end.saturating_sub(start)).map(|offset| {
            if backwards {
                end - 1 - offset
            } else {
                start + offset
            }
        })
    };
    let mut changed = false;
    if is_stackable(&context.stack(source)?) {
        for index in indices() {
            if context.stack(source)?.is_empty() {
                break;
            }
            let target = context.slot_stack(index)?;
            let target_value = context.stack(target)?;
            if !target_value.is_empty()
                && is_same_item_same_components(&context.stack(source)?, &target_value)
            {
                let total = count(&target_value).wrapping_add(count(&context.stack(source)?));
                let max = context.stack_limit(index, target)?;
                if total <= max {
                    context.set_stack_count(source, 0)?;
                    context.set_stack_count(target, total)?;
                    context.slot_changed(index)?;
                    changed = true;
                } else if count(&context.stack(target)?) < max {
                    let amount = max.wrapping_sub(count(&context.stack(target)?));
                    let left = count(&context.stack(source)?).wrapping_sub(amount);
                    context.set_stack_count(source, left)?;
                    context.set_stack_count(target, max)?;
                    context.slot_changed(index)?;
                    changed = true;
                }
            }
        }
    }
    if !context.stack(source)?.is_empty() {
        for index in indices() {
            let target = context.slot_stack(index)?;
            if context.stack(target)?.is_empty() && context.may_place(index, source, hooks)? {
                let max = context.stack_limit(index, source)?;
                let amount = count(&context.stack(source)?).min(max);
                let split = context.split_stack(source, amount)?;
                context.set_by_player(index, split, hooks)?;
                context.slot_changed(index)?;
                changed = true;
                break;
            }
        }
    }
    Ok(changed)
}

#[derive(Clone, Copy)]
pub(crate) enum PureStackRef {
    Source,
    Slot(usize),
}
pub(crate) struct PureMenuTransfer<'a, 'b, 'c> {
    pub menu: &'a mut AbstractContainerMenu,
    pub context: &'b mut MenuContext<'c>,
    pub source: &'a mut ItemStack,
}
impl MenuTransferContext for PureMenuTransfer<'_, '_, '_> {
    type Stack = PureStackRef;

    fn slot_stack(&mut self, index: usize) -> Result<Self::Stack> {
        Ok(PureStackRef::Slot(index))
    }
    fn stack(&self, reference: Self::Stack) -> Result<ItemStack> {
        Ok(match reference {
            PureStackRef::Source => self.source.clone(),
            PureStackRef::Slot(index) => self.menu.get_item(self.context, index),
        })
    }
    fn set_stack_count(&mut self, reference: Self::Stack, amount: i32) -> Result<()> {
        match reference {
            PureStackRef::Source => {
                set_count(self.source, amount);
                Ok(())
            }
            PureStackRef::Slot(index) => {
                let slot = self.menu.slots[index].clone();
                self.menu
                    .container_mut(self.context, slot.container)
                    .set_item_count(slot.container_slot, amount)
            }
        }
    }
    fn split_stack(&mut self, reference: Self::Stack, amount: i32) -> Result<ItemStack> {
        let value = self.stack(reference)?;
        let amount = amount.min(count(&value));
        let result = with_count(&value, amount);
        self.set_stack_count(reference, count(&value).wrapping_sub(amount))?;
        Ok(result)
    }
    fn stack_limit(&mut self, index: usize, reference: Self::Stack) -> Result<i32> {
        let slot = &self.menu.slots[index];
        Ok(slot.max_stack_size(
            self.menu.container(self.context, slot.container),
            &self.stack(reference)?,
        ))
    }
    fn may_place(
        &mut self,
        index: usize,
        reference: Self::Stack,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        self.menu.slots[index].may_place(&self.stack(reference)?, hooks)
    }
    fn set_by_player(
        &mut self,
        index: usize,
        stack: ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let _previous = self.menu.get_item(self.context, index);
        self.menu.set_item_live(self.context, index, stack, hooks)
    }
    fn reassign_by_player(
        &mut self,
        index: usize,
        reference: PureStackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let PureStackRef::Slot(target) = reference else {
            return Err(MenuError::Unsupported("pure cross-slot assignment"));
        };
        assert_eq!(index, target);
        let slot = self.menu.slots[index].clone();
        // Slot#setByPlayer reads previous before Container#setItem.
        let _previous = self.menu.get_item(self.context, index);
        self.menu
            .container_mut(self.context, slot.container)
            .reassign_item(slot.container_slot)?;
        if self.menu.crafting_grid_size().is_some() && slot.container == ContainerId::Local(0) {
            self.menu.crafting_slots_changed(hooks, None)?;
        }
        self.menu
            .container_mut(self.context, slot.container)
            .set_changed();
        Ok(())
    }
    fn slot_changed(&mut self, index: usize) -> Result<()> {
        let container = self.menu.slots[index].container;
        self.menu
            .container_mut(self.context, container)
            .set_changed();
        Ok(())
    }
}

pub(super) struct LiveMenuTransfer<'a> {
    pub operation: StackOperation<'a>,
    pub player: crate::entity::EntityRef,
    pub menu: MenuRef,
}
impl LiveMenuTransfer<'_> {
    pub(super) fn context(&mut self) -> LiveMenuContext<'_> {
        LiveMenuContext {
            level: self.operation.level,
            player: self.player,
            menu: self.menu,
        }
    }
    pub(super) fn slot(&self, index: usize) -> Result<Slot> {
        self.operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)?
            .slots
            .get(index)
            .cloned()
            .ok_or(MenuError::InvalidSlot(index as i32))
    }
    pub(super) fn validate_storage_mutation(
        &mut self,
        index: usize,
    ) -> Result<(Slot, ContainerSlot)> {
        let slot = self.slot(index)?;
        let address = self.context().slot_address(index)?;
        if matches!(slot.kind, SlotKind::Armor(_) | SlotKind::MountEquipment(_))
            || matches!(address.container, ContainerHandle::PlayerInventory(_))
                && address.slot >= 36
        {
            return Err(MenuError::Unsupported(
                "live equipment slot replacement hook",
            ));
        }
        if let ContainerHandle::MenuLocal { player, menu, .. } = address.container {
            let kind = self
                .operation
                .level
                .entity::<crate::entity::player::ServerPlayer>(player)?
                .menus
                .menu(menu)?
                .kind;
            if !matches!(
                kind,
                MenuKind::Chest { .. }
                    | MenuKind::Inventory
                    | MenuKind::Crafting
                    | MenuKind::Dispenser
                    | MenuKind::Hopper
                    | MenuKind::Crafter
                    | MenuKind::ShulkerBox
                    | MenuKind::Furnace
                    | MenuKind::Smoker
                    | MenuKind::BlastFurnace
            ) {
                return Err(MenuError::Unsupported("live recipe menu slotsChanged"));
            }
        }
        Ok((slot, address))
    }
    pub(super) fn set_by_player_value(
        &mut self,
        index: usize,
        value: StackValue,
        previous: Option<StackRef>,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let (_, address) = self.validate_storage_mutation(index)?;
        let grid = matches!(
            address.container,
            ContainerHandle::MenuLocal { container: 0, .. }
        ) && self.menu()?.crafting_grid_size().is_some();
        if grid {
            hooks.prepare_recipe_click(self.menu()?.kind)?;
        }
        if let Some(previous) = previous {
            self.operation.pin_existing(previous)?;
        } else {
            self.operation.pin(address)?;
        }
        match address.container {
            ContainerHandle::BlockEntity(handle) => {
                crate::block::containers::access::set_item_value(
                    handle,
                    address.slot,
                    value,
                    self.operation.level,
                )?;
            }
            ContainerHandle::PlayerInventory(player) => {
                self.operation.level.replace_stack_object(address, value)?;
                let player = self
                    .operation
                    .level
                    .entity_mut::<crate::entity::player::ServerPlayer>(player)?;
                PlayerContainer {
                    inventory: &mut player.inventory,
                    equipment: &mut player.parent.equipment,
                }
                .reassign_item(address.slot)?;
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => {
                self.operation.level.replace_stack_object(address, value)?;
                self.operation
                    .level
                    .entity_mut::<crate::entity::player::ServerPlayer>(player)?
                    .menus
                    .menu_mut(menu)?
                    .local[container]
                    .reassign_item(address.slot)?;
            }
            _ => return Err(MenuError::Unsupported("non-slot object assignment")),
        }
        if grid {
            hooks.live_grid_changed(self.player, self.menu, self.operation.level)?;
        }
        self.slot_changed(index)
    }
}
impl MenuTransferContext for LiveMenuTransfer<'_> {
    type Stack = StackRef;

    fn slot_stack(&mut self, index: usize) -> Result<StackRef> {
        let address = self.context().slot_address(index)?;
        self.operation.pin(address)
    }
    fn stack(&self, reference: StackRef) -> Result<ItemStack> {
        self.operation.level.stack(reference).cloned()
    }
    fn set_stack_count(&mut self, reference: StackRef, amount: i32) -> Result<()> {
        set_count(self.operation.level.stack_mut(reference)?, amount);
        Ok(())
    }
    fn split_stack(&mut self, reference: StackRef, amount: i32) -> Result<ItemStack> {
        Ok(split(self.operation.level.stack_mut(reference)?, amount))
    }
    fn stack_limit(&mut self, index: usize, reference: StackRef) -> Result<i32> {
        let slot = self.slot(index)?;
        let value = self.stack(reference)?;
        let mut context = self.context();
        let address = context.slot_address(index)?;
        slot.max_stack_size_with_limit(&value, || context.max_stack_size(address.container))
    }
    fn may_place(
        &mut self,
        index: usize,
        reference: StackRef,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        let mut slot = self.slot(index)?;
        if slot.kind == SlotKind::Crafter {
            let address = self.context().slot_address(index)?;
            if let ContainerHandle::BlockEntity(handle) = address.container
                && let crate::block::entity::BlockEntityState::Crafter(crafter) =
                    self.operation.level.block_entity_state(handle)?
            {
                slot.disabled = crafter.disabled_slots[address.slot];
            }
        }
        slot.may_place(&self.stack(reference)?, hooks)
    }
    fn set_by_player(
        &mut self,
        index: usize,
        stack: ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.set_by_player_value(index, StackValue::New(stack), None, hooks)
    }
    fn reassign_by_player(
        &mut self,
        index: usize,
        stack: StackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.set_by_player_value(index, StackValue::Existing(stack), None, hooks)
    }
    fn slot_changed(&mut self, index: usize) -> Result<()> {
        let slot = self.slot(index)?;
        let mut context = self.context();
        let address = context.slot_address(index)?;
        context.set_changed(address.container)?;
        if slot.kind == SlotKind::Crafter {
            broadcast_menu_changes(&mut context)?;
        }
        Ok(())
    }
}

/// The live adapter uses the same transfer algorithm as standalone menus. Its
/// source, targets and menu remain rooted through all synchronous callbacks.
#[expect(
    clippy::too_many_arguments,
    reason = "native transfer range and canonical receiver addresses"
)]
pub fn move_live_menu_item_stack_to(
    level: &mut crate::level::Level,
    player: crate::entity::EntityRef,
    menu: MenuRef,
    source: StackRef,
    start: usize,
    end: usize,
    backwards: bool,
    hooks: &mut dyn MenuHooks,
) -> Result<bool> {
    with_menu_callback(player, menu, level, |context| {
        let mut transfer = LiveMenuTransfer {
            operation: StackOperation::new(context.level),
            player,
            menu,
        };
        transfer.operation.pin_existing(source)?;
        move_menu_item_stack_to(&mut transfer, source, start, end, backwards, hooks)
    })
}

/// Live Slot#safeInsert uses the same kernel as legacy in-memory menus. The
/// returned ref is the original input object, even if a callback replaces it.
/// The caller roots that input for its surrounding clicked operation.
pub fn safe_insert_live_menu_item_stack(
    level: &mut crate::level::Level,
    player: crate::entity::EntityRef,
    menu: MenuRef,
    index: usize,
    input: StackRef,
    amount: i32,
    hooks: &mut dyn MenuHooks,
) -> Result<StackRef> {
    with_menu_callback(player, menu, level, |context| {
        let mut transfer = LiveMenuTransfer {
            operation: StackOperation::new(context.level),
            player,
            menu,
        };
        transfer.operation.pin_existing(input)?;
        safe_insert_menu_item_stack(&mut transfer, index, input, amount, hooks)
    })
}

#[cfg(test)]
mod insertion_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::Item;

    #[derive(Clone, Copy)]
    enum Receiver {
        Source(StackSlotRef),
        Target(usize),
    }
    struct TransferFixture {
        source: ItemStack,
        source_generations: StackGenerations,
        held: StackSlotRef,
        target: SimpleContainer,
        replace: bool,
        events: Vec<String>,
    }
    impl TransferFixture {
        fn source(&self, reference: StackSlotRef) -> &ItemStack {
            self.source_generations
                .get(reference, &self.source)
                .unwrap()
        }
        fn observe(&mut self) {
            self.events.push(format!(
                "{}:{}",
                visible(&self.source),
                count(self.source(self.held))
            ));
            if self.replace && self.events.len() == 1 {
                self.source_generations.replace(
                    0,
                    &mut self.source,
                    ItemStack::new(Item::DIAMOND, 7, Default::default()),
                );
            }
        }
    }
    /// The fixture's actual Slot runs its normal set/setChanged path. Its
    /// container callback can replace the source before the transfer resumes.
    struct ObservedTarget<'a>(&'a mut TransferFixture);
    impl Container for ObservedTarget<'_> {
        fn size(&self) -> usize {
            self.0.target.size()
        }
        fn get_item(&self, slot: usize) -> ItemStack {
            self.0.target.get_item(slot)
        }
        fn set_item(&mut self, slot: usize, stack: ItemStack) {
            self.0.target.set_item(slot, stack);
            self.0.observe();
        }
        fn set_changed(&mut self) {
            self.0.target.set_changed();
            self.0.observe();
        }
        fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
            self.0.target.remove_item_no_update(slot)
        }
    }
    impl MenuTransferContext for TransferFixture {
        type Stack = Receiver;

        fn slot_stack(&mut self, index: usize) -> Result<Receiver> {
            Ok(Receiver::Target(index))
        }
        fn stack(&self, reference: Receiver) -> Result<ItemStack> {
            Ok(match reference {
                Receiver::Source(reference) => self.source(reference).clone(),
                Receiver::Target(index) => self.target.get_item(index),
            })
        }
        fn set_stack_count(&mut self, reference: Receiver, amount: i32) -> Result<()> {
            match reference {
                Receiver::Source(reference) => set_count(
                    self.source_generations
                        .get_mut(reference, &mut self.source)
                        .unwrap(),
                    amount,
                ),
                Receiver::Target(index) => self.target.set_item_count(index, amount)?,
            }
            Ok(())
        }
        fn split_stack(&mut self, reference: Receiver, amount: i32) -> Result<ItemStack> {
            let Receiver::Source(reference) = reference else {
                unreachable!()
            };
            Ok(split(
                self.source_generations
                    .get_mut(reference, &mut self.source)
                    .unwrap(),
                amount,
            ))
        }
        fn stack_limit(&mut self, _: usize, reference: Receiver) -> Result<i32> {
            Ok(self
                .target
                .max_stack_size()
                .min(max_stack_size(&self.stack(reference)?)))
        }
        fn may_place(&mut self, _: usize, _: Receiver, _: &dyn MenuHooks) -> Result<bool> {
            Ok(true)
        }
        fn set_by_player(
            &mut self,
            index: usize,
            stack: ItemStack,
            _: &mut dyn MenuHooks,
        ) -> Result<()> {
            Slot::new(ContainerId::Local(0), index, 0, 0)
                .set_by_player(&mut ObservedTarget(self), stack);
            Ok(())
        }
        fn slot_changed(&mut self, _: usize) -> Result<()> {
            ObservedTarget(self).set_changed();
            Ok(())
        }
    }
    fn visible(stack: &ItemStack) -> String {
        format!(
            "{}:{}",
            if stack.is_empty() {
                Item::AIR.name()
            } else {
                stack.0.as_ref().unwrap().item.name()
            },
            count(stack)
        )
    }
    #[test]
    fn transfer_keeps_original_stack_when_callback_replaces_source_slot() {
        let mut checked = 0;
        for line in include_str!("../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("menu-transfer-source\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut source_generations = StackGenerations::new(1);
            let held = source_generations.current(0).unwrap();
            assert!(source_generations.retain(held));
            let mut target = SimpleContainer::new(9);
            target.set_item(0, ItemStack::new(Item::STONE, 60, Default::default()));
            target.set_item(1, ItemStack::new(Item::STONE, 63, Default::default()));
            let mut context = TransferFixture {
                source: ItemStack::new(Item::STONE, 10, Default::default()),
                source_generations,
                held,
                target,
                replace: fields[1] == "true",
                events: Vec::new(),
            };
            assert!(
                move_menu_item_stack_to(
                    &mut context,
                    Receiver::Source(held),
                    0,
                    9,
                    false,
                    &mut UnsupportedMenuHooks,
                )
                .unwrap()
            );
            assert_eq!(context.events.join(","), fields[2], "{line}");
            assert_eq!(count(context.source(held)).to_string(), fields[3], "{line}");
            assert_eq!(visible(&context.source), fields[4], "{line}");
            assert!(context.source_generations.release(held).is_none());
            assert_eq!(
                context
                    .source_generations
                    .get(held, &context.source)
                    .is_none(),
                context.replace
            );
            checked += 1;
        }
        assert_eq!(checked, 2);
    }
    #[test]
    fn live_transfer_mutates_player_and_menu_receivers_before_notifications() {
        use crate::{
            entity::{
                EntityRef, EntityState,
                player::{GameType, ServerPlayer},
            },
            level::{Level, Region},
        };
        use ferraloom_util::{Uuid, Vec3};
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let mut state = ServerPlayer::new(
            Uuid::from_int_array([0, 0, 0, 1]),
            "transfer".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        state
            .inventory
            .set_selected_item(ItemStack::new(Item::STONE, 10, Default::default()));
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 1 }, 7).unwrap();
        menu.local[0].set_item(0, ItemStack::new(Item::STONE, 60, Default::default()));
        menu.local[0].set_item(1, ItemStack::new(Item::STONE, 63, Default::default()));
        let menu = level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .menus
            .install_container_menu(menu);
        {
            let mut operation = StackOperation::new(&mut level);
            let source = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::PlayerInventory(player),
                    slot: 0,
                })
                .unwrap();
            assert!(
                move_live_menu_item_stack_to(
                    operation.level,
                    player,
                    menu,
                    source,
                    0,
                    9,
                    false,
                    &mut UnsupportedMenuHooks
                )
                .unwrap()
            );
            assert!(operation.level.stack(source).unwrap().is_empty());
        }
        let menu = level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .menu(menu)
            .unwrap();
        assert_eq!(
            (0..3)
                .map(|slot| count(&menu.local[0].get_item(slot)))
                .collect::<Vec<_>>(),
            [64, 64, 5]
        );
        let MenuContainer::Simple(target) = &menu.local[0] else {
            unreachable!()
        };
        assert_eq!(target.times_changed, 7); // two setup writes, then five native notifications
    }
}
