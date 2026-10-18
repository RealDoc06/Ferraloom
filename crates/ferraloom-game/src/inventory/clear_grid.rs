//! ServerPlaceRecipe#clearGrid temporarily lends a dropped copy back to its
//! physical container. The entity is not read in this window; normal container
//! readers, including synchronous comparator callbacks, see the real stack.
use super::*;

pub(super) trait ClearGridContext: MenuClickContext {
    type DroppedAddress: Copy;
    fn clearing_slots(&self) -> Result<Vec<usize>>;
    fn inventory_return_slot(&self, stack: Self::Stack) -> Result<Option<usize>>;
    fn add_at_slot(&mut self, slot: usize, stack: Self::Stack) -> Result<bool>;
    fn drop_for_clear(&mut self, stack: Self::Stack) -> Result<Self::DroppedAddress>;
    /// Callback-free move; ItemEntity ITEM is restored without setItem dirtiness.
    fn return_lent(&mut self, stack: Self::Stack, destination: Self::DroppedAddress) -> Result<()>;
    fn clear_result_content(&mut self) -> Result<()>;
    fn clear_grid_content(&mut self) -> Result<()>;
}
struct Loan<C: ClearGridContext> {
    index: usize,
    stack: C::Stack,
    destination: C::DroppedAddress,
}
/// Constructed only by clear_grid. Each loan is recorded before the setter can
/// call back, and all error exits return the actual object to its rooted entity.
struct ClearGridScope<'a, C: ClearGridContext> {
    context: &'a mut C,
    loans: Vec<Loan<C>>,
}
impl<C: ClearGridContext> ClearGridScope<'_, C> {
    fn return_slot(&mut self, index: usize) -> Result<()> {
        if let Some(position) = self.loans.iter().position(|loan| loan.index == index) {
            let loan = &self.loans[position];
            self.context.return_lent(loan.stack, loan.destination)?;
            self.loans.remove(position);
        }
        Ok(())
    }
    fn run(&mut self, hooks: &mut dyn MenuHooks) -> Result<()> {
        let indices = self.context.clearing_slots()?;
        for &index in &indices {
            let source = self.context.slot_stack(index)?;
            // This is vanilla's explicit copy, not an inventory snapshot.
            let value = self.context.stack(source)?;
            let copy = self.context.own_stack(if value.is_empty() {
                ItemStack::default()
            } else {
                value
            })?;
            while !self.context.stack(copy)?.is_empty() {
                let Some(slot) = self.context.inventory_return_slot(copy)? else {
                    let destination = self.context.drop_for_clear(copy)?;
                    self.loans.push(Loan {
                        index,
                        stack: copy,
                        destination,
                    });
                    break;
                };
                let target = self.context.inventory_stack(slot)?;
                let capacity = max_stack_size(&self.context.stack(copy)?)
                    - count(&self.context.stack(target)?);
                let split = self.context.split_stack(copy, capacity)?;
                let split = self.context.own_stack(split)?;
                // send=false: the native add and pop time still happen, with
                // no inventory SetSlot packet between this split and the next.
                self.context.add_at_slot(slot, split)?;
            }
            self.context.set_slot_stack(index, copy, None, hooks)?;
        }
        if self.context.menu()?.crafting_grid_size().is_some() {
            self.context.clear_result_content()?;
            for &index in &indices {
                self.return_slot(index)?;
            }
            self.context.clear_grid_content()?;
        } else {
            // Furnace slotsToClear are input then output. Return just this loan
            // before set(EMPTY), keeping the other physical slot visible to the
            // two setter/setChanged comparator reads at each source point.
            for index in indices {
                self.return_slot(index)?;
                self.context
                    .set_by_player(index, ItemStack::default(), hooks)?;
            }
        }
        Ok(())
    }
}
impl<C: ClearGridContext> Drop for ClearGridScope<'_, C> {
    fn drop(&mut self) {
        for loan in self.loans.drain(..).rev() {
            self.context
                .return_lent(loan.stack, loan.destination)
                .expect("rooted clear-grid loan must return without callbacks");
        }
    }
}
pub(super) fn clear_grid<C: ClearGridContext>(
    context: &mut C,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    ClearGridScope {
        context,
        loans: Vec::new(),
    }
    .run(hooks)
}

impl ClearGridContext for super::in_memory::InMemoryMenuOperation<'_, '_, '_> {
    type DroppedAddress = usize;
    fn clearing_slots(&self) -> Result<Vec<usize>> {
        Ok(match self.menu.crafting_grid_size() {
            Some(width) => (1..=width * width).collect(),
            None => vec![0, 2],
        })
    }
    fn inventory_return_slot(&self, stack: Self::Stack) -> Result<Option<usize>> {
        Ok(self
            .context
            .player
            .inventory
            .get_slot_with_remaining_space(self.read(stack)?, self.context.player.equipment)
            .or_else(|| self.context.player.inventory.get_free_slot()))
    }
    fn add_at_slot(&mut self, slot: usize, stack: Self::Stack) -> Result<bool> {
        add_inventory_stack(self, stack, Some(slot))
    }
    fn drop_for_clear(&mut self, stack: Self::Stack) -> Result<usize> {
        let destination = self.context.drops.len();
        self.drop_stack(stack, false)?;
        Ok(destination)
    }
    fn return_lent(&mut self, stack: Self::Stack, destination: usize) -> Result<()> {
        self.replace(
            super::in_memory::MemoryContainer::Dropped,
            destination,
            stack,
        )
    }
    fn clear_result_content(&mut self) -> Result<()> {
        self.menu.local[1].clear_content();
        Ok(())
    }
    fn clear_grid_content(&mut self) -> Result<()> {
        self.menu.local[0].clear_content();
        Ok(())
    }
}
impl ClearGridContext for super::transfer::LiveMenuTransfer<'_> {
    type DroppedAddress = ContainerSlot;
    fn clearing_slots(&self) -> Result<Vec<usize>> {
        Ok(match self.menu()?.crafting_grid_size() {
            Some(width) => (1..=width * width).collect(),
            None => vec![0, 2],
        })
    }
    fn inventory_return_slot(&self, stack: StackRef) -> Result<Option<usize>> {
        let player = self
            .operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?;
        Ok(player
            .inventory
            .get_slot_with_remaining_space(
                self.operation.level.stack(stack)?,
                &player.parent.equipment,
            )
            .or_else(|| player.inventory.get_free_slot()))
    }
    fn add_at_slot(&mut self, slot: usize, stack: StackRef) -> Result<bool> {
        super::add::add_item_stack_at(self.player, stack, Some(slot), self.operation.level)
    }
    fn drop_for_clear(&mut self, stack: StackRef) -> Result<ContainerSlot> {
        let entity = crate::entity::player::drop_item_stack(
            self.player,
            StackValue::Existing(stack),
            false,
            self.operation.level,
        )?
        .ok_or(MenuError::Unsupported("nonempty recipe grid-clear drop"))?;
        Ok(ContainerSlot {
            container: ContainerHandle::Entity(entity),
            slot: 0,
        })
    }
    fn return_lent(&mut self, stack: StackRef, destination: ContainerSlot) -> Result<()> {
        // stack_parts_mut edits canonical ITEM directly. No setItem or metadata
        // dirty marker is called for this invisible lend/return transaction.
        self.operation
            .level
            .replace_stack_object(destination, StackValue::Existing(stack))
    }
    fn clear_result_content(&mut self) -> Result<()> {
        self.menu_mut()?.local[1].clear_content();
        Ok(())
    }
    fn clear_grid_content(&mut self) -> Result<()> {
        self.menu_mut()?.local[0].clear_content();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{
            EntityState,
            item::ItemEntity,
            player::{GameType, ServerPlayer},
        },
        level::Level,
    };
    use ferraloom_data::Item;
    use ferraloom_util::{Uuid, Vec3};
    struct GridCallback {
        fail: bool,
    }
    impl MenuHooks for GridCallback {
        fn prepare_recipe_click(&mut self, _: MenuKind) -> Result<()> {
            Ok(())
        }
        fn live_grid_changed(
            &mut self,
            _player: crate::entity::EntityRef,
            _menu: MenuRef,
            _level: &mut Level,
        ) -> Result<()> {
            if self.fail {
                Err(MenuError::Unsupported("controlled grid callback"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn scoped_grid_loans_restore_item_metadata_and_identity_on_success_and_error() {
        for fail in [false, true] {
            let mut level = crate::entity::tests::level(false);
            level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
            let player = level.next_entity_ref();
            let state = ServerPlayer::new(
                Uuid::new(1, 2),
                "loan".into(),
                level.next_entity_id(),
                Vec3::ZERO,
                GameType::Survival,
            );
            let menu = state.menus.current_ref();
            level
                .add_entity(player, EntityState::Player(Box::new(state)))
                .unwrap();
            with_menu_callback(player, menu, &mut level, |context| {
                let mut context = super::super::transfer::LiveMenuTransfer {
                    player,
                    menu,
                    operation: StackOperation::new(context.level),
                };
                let stack =
                    context.own_stack(ItemStack::new(Item::STONE, 3, Default::default()))?;
                let destination = context.drop_for_clear(stack)?;
                let ContainerHandle::Entity(entity) = destination.container else {
                    unreachable!()
                };
                // Observe the initial pairing's item field before lending.
                context
                    .operation
                    .level
                    .entity_mut::<ItemEntity>(entity)?
                    .parent
                    .entity_data
                    .pack_dirty();
                {
                    let mut scope = ClearGridScope {
                        context: &mut context,
                        loans: vec![Loan {
                            index: 1,
                            stack,
                            destination,
                        }],
                    };
                    let result =
                        scope
                            .context
                            .set_slot_stack(1, stack, None, &mut GridCallback { fail });
                    assert_eq!(result.is_err(), fail);
                    let address = scope.context.context().slot_address(1)?;
                    let physical = scope.context.operation.level.stack_ref(address)?;
                    assert_eq!(count(scope.context.operation.level.stack(physical)?), 3);
                    assert!(
                        scope
                            .context
                            .operation
                            .level
                            .entity::<ItemEntity>(entity)?
                            .item()
                            .is_empty()
                    );
                    if !fail {
                        scope.context.clear_result_content()?;
                        scope.return_slot(1)?;
                        scope.context.clear_grid_content()?;
                    }
                    // An error unwinds the same private scope without setters.
                }
                let borrowed = context.operation.level.entity::<ItemEntity>(entity)?;
                assert_eq!(count(borrowed.item()), 3);
                assert!(std::ptr::eq(
                    context.operation.level.stack(stack)?,
                    borrowed.item()
                ));
                assert!(
                    context
                        .operation
                        .level
                        .entity_mut::<ItemEntity>(entity)?
                        .parent
                        .entity_data
                        .pack_dirty()
                        .is_none()
                );
                Ok(())
            })
            .unwrap();
        }
    }
}
