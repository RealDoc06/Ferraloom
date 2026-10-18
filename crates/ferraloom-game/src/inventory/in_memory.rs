//! The standalone adapter retains the same receiver objects as the live level.
//! Its forwarding edges belong to the surrounding MenuContext, so nested menu
//! operations see moves made by each other before their callbacks unwind.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum MemoryContainer {
    Container(ContainerId),
    Carried,
    Operation,
    Dropped,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct MemoryStackRef {
    owner: MemoryContainer,
    slot: usize,
    generation: u64,
}
#[derive(Default)]
pub(super) struct InMemoryStackState {
    objects: super::operation_stacks::OperationStackArena,
    forwards: HashMap<MemoryStackRef, MemoryStackRef>,
    dropped: Option<StackGenerations>,
}
#[cfg(test)]
impl InMemoryStackState {
    pub(super) fn forwards_are_empty(&self) -> bool {
        self.forwards.is_empty()
    }
}
pub(super) struct InMemoryMenuOperation<'a, 'b, 'p> {
    pub menu: &'a mut AbstractContainerMenu,
    pub context: &'b mut MenuContext<'p>,
    roots: Vec<MemoryStackRef>,
}
impl<'a, 'b, 'p> InMemoryMenuOperation<'a, 'b, 'p> {
    pub fn new(menu: &'a mut AbstractContainerMenu, context: &'b mut MenuContext<'p>) -> Self {
        Self {
            menu,
            context,
            roots: Vec::new(),
        }
    }
    fn parts(
        &self,
        owner: MemoryContainer,
        slot: usize,
    ) -> Result<(&StackGenerations, &ItemStack, usize)> {
        let parts = match owner {
            MemoryContainer::Container(container) => self
                .menu
                .container(self.context, container)
                .stack_parts(slot),
            MemoryContainer::Carried if slot == 0 => {
                Some((&self.menu.carried_generations, &self.menu.carried, 0))
            }
            MemoryContainer::Carried => None,
            MemoryContainer::Operation => self.context.in_memory_stacks.objects.parts(slot),
            MemoryContainer::Dropped => self.context.drops.get(slot).map(|dropped| {
                (
                    self.context.in_memory_stacks.dropped.as_ref().unwrap(),
                    &dropped.stack,
                    slot,
                )
            }),
        };
        parts.ok_or(MenuError::InvalidSlot(slot as i32))
    }
    fn parts_mut(
        &mut self,
        owner: MemoryContainer,
        slot: usize,
    ) -> Result<(&mut StackGenerations, &mut ItemStack, usize)> {
        let parts = match owner {
            MemoryContainer::Container(container) => self
                .menu
                .container_mut(self.context, container)
                .stack_parts_mut(slot),
            MemoryContainer::Carried if slot == 0 => Some((
                &mut self.menu.carried_generations,
                &mut self.menu.carried,
                0,
            )),
            MemoryContainer::Carried => None,
            MemoryContainer::Operation => self.context.in_memory_stacks.objects.parts_mut(slot),
            MemoryContainer::Dropped => self.context.drops.get_mut(slot).map(|dropped| {
                (
                    self.context.in_memory_stacks.dropped.as_mut().unwrap(),
                    &mut dropped.stack,
                    slot,
                )
            }),
        };
        parts.ok_or(MenuError::InvalidSlot(slot as i32))
    }
    fn current(&self, owner: MemoryContainer, slot: usize) -> Result<MemoryStackRef> {
        let (generations, _, index) = self.parts(owner, slot)?;
        Ok(MemoryStackRef {
            owner,
            slot,
            generation: generations.current(index).unwrap().generation,
        })
    }
    fn retain_current(&mut self, owner: MemoryContainer, slot: usize) -> Result<MemoryStackRef> {
        let reference = self.current(owner, slot)?;
        let (generations, _, index) = self.parts_mut(owner, slot)?;
        assert!(generations.retain(StackSlotRef {
            slot: index,
            generation: reference.generation
        }));
        Ok(reference)
    }
    pub fn capture(&mut self, owner: MemoryContainer, slot: usize) -> Result<MemoryStackRef> {
        if let MemoryContainer::Container(container) = owner {
            // Invoke the native getter at the capture point; the value is only
            // an observation. All later reads and writes resolve the receiver.
            let _ = self.menu.container(self.context, container).get_item(slot);
        }
        let reference = self.retain_current(owner, slot)?;
        self.roots.push(reference);
        Ok(reference)
    }
    pub fn own(&mut self, stack: ItemStack) -> MemoryStackRef {
        let local = self.context.in_memory_stacks.objects.insert(stack);
        let reference = MemoryStackRef {
            owner: MemoryContainer::Operation,
            slot: local.slot,
            generation: local.generation,
        };
        self.roots.push(reference);
        reference
    }
    fn resolve(&self, mut reference: MemoryStackRef) -> Result<MemoryStackRef> {
        loop {
            if let Some(&next) = self.context.in_memory_stacks.forwards.get(&reference) {
                reference = next;
                continue;
            }
            let (generations, _, index) = self.parts(reference.owner, reference.slot)?;
            match generations.forwarded(StackSlotRef {
                slot: index,
                generation: reference.generation,
            }) {
                Some(StackForward::Local(next)) => {
                    reference.slot = next.slot;
                    reference.generation = next.generation;
                }
                Some(StackForward::External(_)) => {
                    return Err(MenuError::Unsupported(
                        "canonical receiver in standalone menu context",
                    ));
                }
                None => return Ok(reference),
            }
        }
    }
    pub fn read(&self, reference: MemoryStackRef) -> Result<&ItemStack> {
        let reference = self.resolve(reference)?;
        let (generations, item, index) = self.parts(reference.owner, reference.slot)?;
        generations
            .get(
                StackSlotRef {
                    slot: index,
                    generation: reference.generation,
                },
                item,
            )
            .ok_or(MenuError::Unsupported("retired standalone stack receiver"))
    }
    pub fn mutate(&mut self, reference: MemoryStackRef) -> Result<&mut ItemStack> {
        let reference = self.resolve(reference)?;
        let (generations, item, index) = self.parts_mut(reference.owner, reference.slot)?;
        generations
            .get_mut(
                StackSlotRef {
                    slot: index,
                    generation: reference.generation,
                },
                item,
            )
            .ok_or(MenuError::Unsupported("retired standalone stack receiver"))
    }
    pub fn replace(
        &mut self,
        owner: MemoryContainer,
        slot: usize,
        source: MemoryStackRef,
    ) -> Result<()> {
        let source = self.resolve(source)?;
        if self.current(owner, slot)? == source {
            return Ok(());
        }
        self.parts(owner, slot)?;
        let (generations, item, index) = self.parts_mut(source.owner, source.slot)?;
        let item = generations
            .take_for_move(
                StackSlotRef {
                    slot: index,
                    generation: source.generation,
                },
                item,
            )
            .ok_or(MenuError::Unsupported("unrooted standalone stack move"))?;
        let (generations, current, index) = self.parts_mut(owner, slot)?;
        generations.replace(index, current, item);
        let destination = self.retain_current(owner, slot)?;
        // This root belongs to the forwarding edge, rather than this frame.
        self.context
            .in_memory_stacks
            .forwards
            .insert(source, destination);
        Ok(())
    }
    pub fn detach(&mut self, owner: MemoryContainer, slot: usize) -> Result<MemoryStackRef> {
        let reference = self.capture(owner, slot)?;
        let (generations, item, index) = self.parts_mut(owner, slot)?;
        generations.replace(index, item, ItemStack::default());
        Ok(reference)
    }
    fn release(&mut self, reference: MemoryStackRef) {
        let mut chain = vec![reference];
        loop {
            let last = *chain.last().unwrap();
            let Ok((generations, _, index)) = self.parts(last.owner, last.slot) else {
                break;
            };
            if let Some(StackForward::Local(next)) = generations.forwarded(StackSlotRef {
                slot: index,
                generation: last.generation,
            }) {
                chain.push(MemoryStackRef {
                    owner: last.owner,
                    slot: next.slot,
                    generation: next.generation,
                });
            } else {
                break;
            }
        }
        let external = if reference.owner == MemoryContainer::Operation {
            self.context.in_memory_stacks.objects.release(StackSlotRef {
                slot: reference.slot,
                generation: reference.generation,
            })
        } else if let Ok((generations, _, index)) = self.parts_mut(reference.owner, reference.slot)
        {
            generations.release(StackSlotRef {
                slot: index,
                generation: reference.generation,
            })
        } else {
            None
        };
        assert!(
            external.is_none(),
            "canonical roots require a live menu context"
        );
        for reference in chain {
            let retained =
                self.parts(reference.owner, reference.slot)
                    .is_ok_and(|(generations, _, index)| {
                        generations.is_retained(StackSlotRef {
                            slot: index,
                            generation: reference.generation,
                        })
                    });
            if !retained
                && let Some(next) = self.context.in_memory_stacks.forwards.remove(&reference)
            {
                self.release(next);
            }
        }
    }
}
impl Drop for InMemoryMenuOperation<'_, '_, '_> {
    fn drop(&mut self) {
        while let Some(reference) = self.roots.pop() {
            self.release(reference);
        }
    }
}

impl MenuTransferContext for InMemoryMenuOperation<'_, '_, '_> {
    type Stack = MemoryStackRef;
    fn slot_stack(&mut self, index: usize) -> Result<MemoryStackRef> {
        let slot = self
            .menu
            .slots
            .get(index)
            .ok_or(MenuError::InvalidSlot(index as i32))?
            .clone();
        self.capture(
            MemoryContainer::Container(slot.container),
            slot.container_slot,
        )
    }
    fn stack(&self, reference: MemoryStackRef) -> Result<ItemStack> {
        self.read(reference).cloned()
    }
    fn set_stack_count(&mut self, reference: MemoryStackRef, amount: i32) -> Result<()> {
        set_count(self.mutate(reference)?, amount);
        Ok(())
    }
    fn split_stack(&mut self, reference: MemoryStackRef, amount: i32) -> Result<ItemStack> {
        Ok(split(self.mutate(reference)?, amount))
    }
    fn stack_limit(&mut self, index: usize, reference: MemoryStackRef) -> Result<i32> {
        let slot = &self.menu.slots[index];
        Ok(slot.max_stack_size(
            self.menu.container(self.context, slot.container),
            self.read(reference)?,
        ))
    }
    fn may_place(
        &mut self,
        index: usize,
        reference: MemoryStackRef,
        hooks: &dyn MenuHooks,
    ) -> Result<bool> {
        self.menu.slots[index].may_place(self.read(reference)?, hooks)
    }
    fn set_by_player(
        &mut self,
        index: usize,
        stack: ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let reference = self.own(stack);
        self.set_slot_stack(index, reference, None, hooks)
    }
    fn reassign_by_player(
        &mut self,
        index: usize,
        stack: MemoryStackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.set_slot_stack(index, stack, None, hooks)
    }
    fn slot_changed(&mut self, index: usize) -> Result<()> {
        let container = self.menu.slots[index].container;
        self.menu
            .container_mut(self.context, container)
            .set_changed();
        if self.menu.slots[index].kind == SlotKind::Crafter {
            self.menu.broadcast_changes(self.context)?;
        }
        Ok(())
    }
}
impl MenuClickContext for InMemoryMenuOperation<'_, '_, '_> {
    fn quick_craft_slot(
        &mut self,
        index: usize,
        picked: MemoryStackRef,
        original: MemoryStackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let amount = count(self.read(original)?) - count(self.read(picked)?);
        if amount > 0 {
            if self.menu.slots[index].kind == SlotKind::Result(ResultKind::Crafting) {
                self.menu.crafting_remove_count =
                    self.menu.crafting_remove_count.wrapping_add(amount);
                check_crafting_take_achievements(self, original, hooks)?;
            } else if let SlotKind::Result(kind) = self.menu.slots[index].kind {
                hooks.on_take(kind, self.read(original)?, amount)?;
            }
        }
        Ok(())
    }
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn infinite_materials(&self) -> Result<bool> {
        Ok(self.context.infinite_materials)
    }
    fn carried_stack(&mut self) -> Result<MemoryStackRef> {
        self.capture(MemoryContainer::Carried, 0)
    }
    fn own_stack(&mut self, stack: ItemStack) -> Result<MemoryStackRef> {
        Ok(self.own(stack))
    }
    fn set_carried_stack(&mut self, reference: MemoryStackRef) -> Result<()> {
        self.replace(MemoryContainer::Carried, 0, reference)
    }
    fn inventory_stack(&mut self, slot: usize) -> Result<MemoryStackRef> {
        self.capture(MemoryContainer::Container(ContainerId::Player), slot)
    }
    fn set_inventory_stack(&mut self, slot: usize, stack: MemoryStackRef) -> Result<()> {
        self.replace(MemoryContainer::Container(ContainerId::Player), slot, stack)
    }
    fn add_to_inventory(&mut self, stack: MemoryStackRef) -> Result<bool> {
        add_inventory_stack(self, stack, None)
    }
    fn drop_stack(&mut self, stack: MemoryStackRef, thrown_from_hand: bool) -> Result<()> {
        if self.read(stack)?.is_empty() {
            return Ok(());
        }
        // The standalone drop output owns the moved receiver just as the live
        // ItemEntity does; held refs forward there through the operation.
        let slot = self.context.drops.len();
        match self.context.in_memory_stacks.dropped.as_mut() {
            None => self.context.in_memory_stacks.dropped = Some(StackGenerations::new(slot + 1)),
            Some(generations) => {
                while generations.current(slot).is_none() {
                    generations.append_slot();
                }
            }
        }
        self.context.drops.push(DroppedStack {
            stack: ItemStack::default(),
            thrown_from_hand,
        });
        self.replace(MemoryContainer::Dropped, slot, stack)
    }
    fn swap_craft(&mut self, index: usize, amount: i32) -> Result<()> {
        if self.menu.slots[index].kind == SlotKind::Result(ResultKind::Crafting) {
            self.menu.crafting_remove_count = self.menu.crafting_remove_count.wrapping_add(amount);
        }
        Ok(())
    }
    fn prepare_swap_craft(&self, index: usize, hooks: &dyn MenuHooks) -> Result<()> {
        if self.menu.slots[index].kind == SlotKind::Result(ResultKind::Crafting)
            && let Some(crafting) = hooks.crafting_ref()
        {
            crafting.validate_take_result()?;
        }
        Ok(())
    }
    fn slot_count(&self) -> Result<usize> {
        Ok(self.menu.slots.len())
    }
    fn can_take_for_pick_all(&self, index: usize) -> Result<bool> {
        Ok(self.menu.can_take_for_pick_all(&self.menu.slots[index]))
    }
    fn can_drop_items(&self) -> Result<bool> {
        Ok(self.context.can_drop)
    }
    fn clone_slot_stack(
        &mut self,
        index: usize,
        hooks: &mut dyn MenuHooks,
    ) -> Result<MemoryStackRef> {
        let slot = self.menu.slots[index].clone();
        let source = self.slot_stack(index)?;
        let copy = with_count(self.read(source)?, max_stack_size(self.read(source)?));
        let copy = self.own(copy);
        if slot.kind == SlotKind::Result(ResultKind::Crafting)
            && let Some(crafting) = hooks.crafting()
        {
            let observed = self.read(copy)?.clone();
            crafting.item_crafted(self.menu, self.context, &observed, None)?;
        }
        Ok(copy)
    }
    fn quick_move_stack(
        &mut self,
        index: usize,
        hooks: &mut dyn MenuHooks,
    ) -> Result<MemoryStackRef> {
        if matches!(self.menu.kind, MenuKind::Inventory | MenuKind::Crafting) {
            return super::quick_move_crafting::quick_move_crafting_menu_item(self, index, hooks);
        }
        if matches!(
            self.menu.kind,
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace
        ) {
            return super::quick_move_furnace::quick_move_furnace_menu_item(self, index, hooks);
        }
        let copy = self.menu.quick_move_stack(self.context, index, hooks)?;
        Ok(self.own(copy))
    }
    fn set_slot_stack(
        &mut self,
        index: usize,
        reference: MemoryStackRef,
        previous: Option<MemoryStackRef>,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.menu.slots[index].clone();
        let grid =
            self.menu.crafting_grid_size().is_some() && slot.container == ContainerId::Local(0);
        if grid && hooks.has_crafting() {
            hooks.prepare_recipe_click(self.menu.kind)?;
        }
        if let Some(previous) = previous {
            self.read(previous)?;
        } else {
            self.slot_stack(index)?;
        }
        self.replace(
            MemoryContainer::Container(slot.container),
            slot.container_slot,
            reference,
        )?;
        self.menu
            .container_mut(self.context, slot.container)
            .reassign_item(slot.container_slot)?;
        if grid {
            self.menu.crafting_slots_changed(hooks, None)?;
        }
        self.slot_changed(index)
    }
    fn may_pickup(&mut self, index: usize, hooks: &dyn MenuHooks) -> Result<bool> {
        let slot = self.menu.slots[index].clone();
        if matches!(slot.kind, SlotKind::Armor(_) | SlotKind::Result(_)) {
            let stack = self.slot_stack(index)?;
            slot.may_pickup(self.read(stack)?, self.context.is_creative, hooks)
        } else {
            slot.may_pickup(&ItemStack::default(), self.context.is_creative, hooks)
        }
    }
    fn remove_slot_stack(
        &mut self,
        index: usize,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<MemoryStackRef> {
        let slot = self.menu.slots[index].clone();
        if slot.kind == SlotKind::Result(ResultKind::Crafting) && hooks.has_crafting() {
            let current = self.slot_stack(index)?;
            if !self.read(current)?.is_empty() {
                hooks
                    .crafting_ref()
                    .expect("crafting bridge")
                    .validate_take_result()?;
                let current = self.slot_stack(index)?;
                self.menu.crafting_remove_count = self
                    .menu
                    .crafting_remove_count
                    .wrapping_add(amount.min(count(self.read(current)?)));
            }
        }
        let result = if matches!(slot.container, ContainerId::Local(container) if matches!(self.menu.local[container], MenuContainer::Result(_)))
        {
            self.detach(
                MemoryContainer::Container(slot.container),
                slot.container_slot,
            )?
        } else {
            let result = self
                .menu
                .container_mut(self.context, slot.container)
                .remove_item(slot.container_slot, amount);
            self.own(result)
        };
        if !self.read(result)?.is_empty()
            && self.menu.crafting_grid_size().is_some()
            && slot.container == ContainerId::Local(0)
        {
            self.menu.crafting_slots_changed(hooks, None)?;
        }
        Ok(result)
    }
    fn take_slot_stack(
        &mut self,
        index: usize,
        stack: MemoryStackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        if self.menu.slots[index].kind == SlotKind::Result(ResultKind::Crafting)
            && hooks.has_crafting()
        {
            return take_crafting_result_stack(self, stack, hooks);
        }
        // The legacy callback API borrows an observed value; mutations continue
        // through the canonical menu/context and are never written back.
        let observed = self.read(stack)?.clone();
        self.menu.on_take(self.context, index, &observed, 0, hooks)
    }
}

impl InventoryAddContext for InMemoryMenuOperation<'_, '_, '_> {
    type Stack = MemoryStackRef;
    fn stack(&self, reference: MemoryStackRef) -> Result<&ItemStack> {
        self.read(reference)
    }
    fn set_stack_count(&mut self, reference: MemoryStackRef, amount: i32) -> Result<()> {
        set_count(self.mutate(reference)?, amount);
        Ok(())
    }
    fn inventory_stack(&mut self, slot: usize) -> Result<MemoryStackRef> {
        self.capture(MemoryContainer::Container(ContainerId::Player), slot)
    }
    fn set_inventory_stack(&mut self, slot: usize, stack: ItemStack) -> Result<MemoryStackRef> {
        let reference = self.own(stack);
        self.replace(
            MemoryContainer::Container(ContainerId::Player),
            slot,
            reference,
        )?;
        self.capture(MemoryContainer::Container(ContainerId::Player), slot)
    }
    fn set_inventory_pop_time(&mut self, slot: usize, ticks: i32) -> Result<()> {
        self.context.player.inventory.set_pop_time(slot, ticks);
        Ok(())
    }
    fn selected_slot(&self) -> Result<usize> {
        Ok(self.context.player.inventory.selected_slot())
    }
    fn infinite_materials(&self) -> Result<bool> {
        Ok(self.context.infinite_materials)
    }
}
