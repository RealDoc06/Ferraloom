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
    fn slot_changed(&mut self, index: usize) -> Result<()>;
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
    fn slot_changed(&mut self, index: usize) -> Result<()> {
        let container = self.menu.slots[index].container;
        self.menu
            .container_mut(self.context, container)
            .set_changed();
        Ok(())
    }
}

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
            context.source_generations.release(held);
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
}
