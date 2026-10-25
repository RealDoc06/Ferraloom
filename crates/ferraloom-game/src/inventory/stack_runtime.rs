//! Canonical stack receiver access. An operation owns roots, never inventories.
use super::{
    Container, ContainerHandle, ContainerSlot, ItemStack, ListenerCopyRef, MenuError, Result,
    StackForward, StackGenerations, StackRef, StackSlotRef, StackValue,
};
use crate::{
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};

impl Level {
    pub(crate) fn pin_stack(&mut self, slot: ContainerSlot) -> Result<StackRef> {
        self.retain_stack_owner(slot.container)?;
        let result = (|| {
            if let ContainerHandle::BlockEntity(handle) = slot.container {
                crate::block::containers::access::get_item(handle, slot.slot, self)?;
            }
            let reference = self.stack_ref(slot)?;
            self.retain_stack(reference)?;
            Ok(reference)
        })();
        self.release_stack_owner(slot.container);
        result
    }
    fn stack_parts(&self, slot: ContainerSlot) -> Result<(&StackGenerations, &ItemStack, usize)> {
        match slot.container {
            ContainerHandle::PlayerInventory(entity) => {
                let player = self.entity::<ServerPlayer>(entity)?;
                player
                    .inventory
                    .stack_parts(slot.slot, &player.parent.equipment)
                    .ok_or(MenuError::InvalidSlot(slot.slot as i32))
            }
            ContainerHandle::BlockEntity(handle) => self
                .block_entity::<crate::block::containers::storage::BaseContainerBlockEntity>(
                    handle,
                )?
                .stack_parts(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self
                .entity::<ServerPlayer>(player)?
                .menus
                .menu(menu)?
                .local
                .get(container)
                .and_then(|container| container.stack_parts(slot.slot))
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuCarried { player, menu } if slot.slot == 0 => {
                let menu = self.entity::<ServerPlayer>(player)?.menus.menu(menu)?;
                Ok((&menu.carried_generations, &menu.carried, 0))
            }
            ContainerHandle::MenuCarried { .. } => Err(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuListenerCopies { .. } => Err(MenuError::Unsupported(
                "listener copy objects use the menu arena",
            )),
            ContainerHandle::MenuOperationStacks { player } => self
                .entity::<ServerPlayer>(player)?
                .menus
                .operation_stacks
                .parts(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::Entity(entity) => self
                .entity::<crate::entity::item::ItemEntity>(entity)?
                .stack_parts(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::LivingEquipment(entity) => {
                let equipment_slot = *crate::entity::player::equipment::SLOTS
                    .get(slot.slot)
                    .ok_or(MenuError::InvalidSlot(slot.slot as i32))?;
                Ok(self
                    .entity::<crate::entity::LivingEntity>(entity)?
                    .equipment
                    .stack_parts(equipment_slot))
            }
        }
    }
    fn stack_parts_mut(
        &mut self,
        slot: ContainerSlot,
    ) -> Result<(&mut StackGenerations, &mut ItemStack, usize)> {
        match slot.container {
            ContainerHandle::PlayerInventory(entity) => {
                let player = self.entity_mut::<ServerPlayer>(entity)?;
                player
                    .inventory
                    .stack_parts_mut(slot.slot, &mut player.parent.equipment)
                    .ok_or(MenuError::InvalidSlot(slot.slot as i32))
            }
            ContainerHandle::BlockEntity(handle) => self
                .block_entity_mut::<crate::block::containers::storage::BaseContainerBlockEntity>(
                    handle,
                )?
                .stack_parts_mut(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .menu_mut(menu)?
                .local
                .get_mut(container)
                .and_then(|container| container.stack_parts_mut(slot.slot))
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuCarried { player, menu } if slot.slot == 0 => {
                let menu = self
                    .entity_mut::<ServerPlayer>(player)?
                    .menus
                    .menu_mut(menu)?;
                Ok((&mut menu.carried_generations, &mut menu.carried, 0))
            }
            ContainerHandle::MenuCarried { .. } => Err(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::MenuListenerCopies { .. } => Err(MenuError::Unsupported(
                "listener copy objects use the menu arena",
            )),
            ContainerHandle::MenuOperationStacks { player } => self
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .operation_stacks
                .parts_mut(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::Entity(entity) => self
                .entity_mut::<crate::entity::item::ItemEntity>(entity)?
                .stack_parts_mut(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32)),
            ContainerHandle::LivingEquipment(entity) => {
                let equipment_slot = *crate::entity::player::equipment::SLOTS
                    .get(slot.slot)
                    .ok_or(MenuError::InvalidSlot(slot.slot as i32))?;
                Ok(self
                    .entity_mut::<crate::entity::LivingEntity>(entity)?
                    .equipment
                    .stack_parts_mut(equipment_slot))
            }
        }
    }
    fn retain_stack_owner(&mut self, owner: ContainerHandle) -> Result<()> {
        match owner {
            ContainerHandle::BlockEntity(handle) => self.retain_block_entity(handle)?,
            ContainerHandle::MenuLocal { player, menu, .. }
            | ContainerHandle::MenuCarried { player, menu } => {
                let handles = self
                    .entity::<ServerPlayer>(player)?
                    .menus
                    .menu(menu)?
                    .block_entities
                    .clone();
                for (index, &handle) in handles.iter().enumerate() {
                    if let Err(error) = self.retain_block_entity(handle) {
                        for &retained in &handles[..index] {
                            self.release_block_entity(retained);
                        }
                        return Err(error.into());
                    }
                }
                self.entity_mut::<ServerPlayer>(player)?
                    .menus
                    .retain_menu(menu)?;
            }
            ContainerHandle::PlayerInventory(_)
            | ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => {}
            ContainerHandle::Entity(entity) | ContainerHandle::LivingEquipment(entity) => {
                self.retain_entity(entity)?
            }
        }
        Ok(())
    }
    fn release_stack_owner(&mut self, owner: ContainerHandle) {
        match owner {
            ContainerHandle::BlockEntity(handle) => self.release_block_entity(handle),
            ContainerHandle::Entity(entity) | ContainerHandle::LivingEquipment(entity) => {
                self.release_entity(entity)
            }
            ContainerHandle::MenuLocal { player, menu, .. }
            | ContainerHandle::MenuCarried { player, menu } => {
                if let Ok(player) = self.entity_mut::<ServerPlayer>(player)
                    && let Ok(receiver) = player.menus.menu(menu)
                {
                    let handles = receiver.block_entities.clone();
                    player.menus.release_menu(menu);
                    for handle in handles {
                        self.release_block_entity(handle);
                    }
                }
            }
            _ => {}
        }
    }
    /// Capture identity after the caller's native getItem has finished.
    pub fn stack_ref(&self, slot: ContainerSlot) -> Result<StackRef> {
        if let ContainerHandle::MenuListenerCopies { player } = slot.container {
            let reference = self
                .entity::<ServerPlayer>(player)?
                .menus
                .listener_copies
                .reference(slot.slot)
                .ok_or(MenuError::InvalidSlot(slot.slot as i32))?;
            return Ok(StackRef {
                container: slot.container,
                slot: reference.slot,
                generation: reference.generation,
            });
        }
        let (generations, _, index) = self.stack_parts(slot)?;
        Ok(StackRef {
            container: slot.container,
            slot: if matches!(slot.container, ContainerHandle::PlayerInventory(_)) {
                slot.slot
            } else {
                index
            },
            generation: generations.current(index).unwrap().generation,
        })
    }
    pub(crate) fn stack_is_retained(&self, slot: ContainerSlot) -> Result<bool> {
        let (generations, _, index) = self.stack_parts(slot)?;
        Ok(generations.is_retained(generations.current(index).unwrap()))
    }
    pub fn retain_stack(&mut self, reference: StackRef) -> Result<()> {
        if let ContainerHandle::MenuListenerCopies { player } = reference.container {
            return self
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .listener_copies
                .retain(ListenerCopyRef {
                    slot: reference.slot,
                    generation: reference.generation,
                })
                .then_some(())
                .ok_or(MenuError::MissingStack(reference));
        }
        self.retain_stack_owner(reference.container)?;
        let retained =
            self.stack_parts_mut(reference.address())
                .is_ok_and(|(generations, _, index)| {
                    generations.retain(StackSlotRef {
                        slot: index,
                        generation: reference.generation,
                    })
                });
        if !retained {
            self.release_stack_owner(reference.container);
            return Err(MenuError::MissingStack(reference));
        }
        Ok(())
    }
    pub fn release_stack(&mut self, mut reference: StackRef) {
        loop {
            if let ContainerHandle::MenuListenerCopies { player } = reference.container {
                if let Ok(player) = self.entity_mut::<ServerPlayer>(player) {
                    player.menus.listener_copies.release(ListenerCopyRef {
                        slot: reference.slot,
                        generation: reference.generation,
                    });
                }
                return;
            }
            if let ContainerHandle::MenuOperationStacks { player } = reference.container {
                let forwarded = match self.entity_mut::<ServerPlayer>(player) {
                    Ok(player) => player.menus.operation_stacks.release(reference.local()),
                    Err(_) => return,
                };
                match forwarded {
                    Some(next) => {
                        reference = next;
                        continue;
                    }
                    None => return,
                }
            }
            let Ok((generations, _, index)) = self.stack_parts_mut(reference.address()) else {
                return;
            };
            let forwarded = generations.release(StackSlotRef {
                slot: index,
                generation: reference.generation,
            });
            self.release_stack_owner(reference.container);
            match forwarded {
                Some(next) => reference = next,
                None => return,
            }
        }
    }
    fn resolve_stack(&self, mut reference: StackRef) -> Result<StackRef> {
        loop {
            if matches!(
                reference.container,
                ContainerHandle::MenuListenerCopies { .. }
            ) {
                return Ok(reference);
            }
            let (generations, _, index) = self.stack_parts(reference.address())?;
            match generations.forwarded(StackSlotRef {
                slot: index,
                generation: reference.generation,
            }) {
                Some(StackForward::External(next)) => reference = next,
                Some(StackForward::Local(next)) => {
                    // Local moves in Inventory address its 36 main slots;
                    // equipment moves use full external handles.
                    reference.slot = next.slot;
                    reference.generation = next.generation;
                }
                None => return Ok(reference),
            }
        }
    }
    pub fn stack(&self, reference: StackRef) -> Result<&ItemStack> {
        let resolved = self.resolve_stack(reference)?;
        if let ContainerHandle::MenuListenerCopies { player } = resolved.container {
            return self
                .entity::<ServerPlayer>(player)?
                .menus
                .listener_copies
                .get(ListenerCopyRef {
                    slot: resolved.slot,
                    generation: resolved.generation,
                })
                .ok_or(MenuError::MissingStack(reference));
        }
        let (generations, item, index) = self.stack_parts(resolved.address())?;
        generations
            .get(
                StackSlotRef {
                    slot: index,
                    generation: resolved.generation,
                },
                item,
            )
            .ok_or(MenuError::MissingStack(reference))
    }
    pub fn stack_mut(&mut self, reference: StackRef) -> Result<&mut ItemStack> {
        let resolved = self.resolve_stack(reference)?;
        if let ContainerHandle::MenuListenerCopies { player } = resolved.container {
            return self
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .listener_copies
                .get_mut(ListenerCopyRef {
                    slot: resolved.slot,
                    generation: resolved.generation,
                })
                .ok_or(MenuError::MissingStack(reference));
        }
        let (generations, item, index) = self.stack_parts_mut(resolved.address())?;
        generations
            .get_mut(
                StackSlotRef {
                    slot: index,
                    generation: resolved.generation,
                },
                item,
            )
            .ok_or(MenuError::MissingStack(reference))
    }
    /// Extract a rooted receiver during a callback-free move. The caller must
    /// install it and call forward_moved_stack before invoking any callbacks.
    pub(crate) fn take_stack_for_move(&mut self, reference: StackRef) -> Result<ItemStack> {
        let source = self.resolve_stack(reference)?;
        let (generations, current, index) = self.stack_parts_mut(source.address())?;
        generations
            .take_for_move(
                StackSlotRef {
                    slot: index,
                    generation: source.generation,
                },
                current,
            )
            .ok_or(MenuError::MissingStack(reference))
    }
    /// Finish a callback-free move. The original receiver owns one destination
    /// root until its final holder releases it; later replacements stay visible
    /// through the same forwarding chain.
    pub(crate) fn forward_moved_stack(
        &mut self,
        source: StackRef,
        destination: StackRef,
    ) -> Result<()> {
        let source = self.resolve_stack(source)?;
        let destination = self.resolve_stack(destination)?;
        let (generations, current, index) = self.stack_parts(source.address())?;
        let local = StackSlotRef {
            slot: index,
            generation: source.generation,
        };
        if source == destination
            || !generations.is_retained(local)
            || generations.get(local, current).is_some()
        {
            return Err(MenuError::MissingStack(source));
        }
        self.stack(destination)?;
        self.retain_stack(destination)?;
        let result = self
            .stack_parts_mut(source.address())
            .map(|(generations, _, _)| {
                generations.forward_to(local, destination);
            });
        if result.is_err() {
            self.release_stack(destination);
        }
        result
    }
    /// Replace/move the object atomically, before the container invokes any
    /// native callbacks. The caller keeps the Existing receiver rooted.
    pub fn replace_stack_object(&mut self, slot: ContainerSlot, value: StackValue) -> Result<()> {
        if matches!(slot.container, ContainerHandle::MenuListenerCopies { .. })
            || matches!(&value, StackValue::Existing(reference)
                if matches!(reference.container, ContainerHandle::MenuListenerCopies { .. }))
        {
            return Err(MenuError::Unsupported(
                "listener copy assignment to an inventory slot",
            ));
        }
        match value {
            StackValue::New(item) => {
                let (generations, current, index) = self.stack_parts_mut(slot)?;
                generations.replace(index, current, item);
            }
            StackValue::Existing(original) => {
                let source = self.resolve_stack(original)?;
                if self.stack_ref(slot)? == source {
                    return Ok(());
                }
                self.retain_stack_owner(slot.container)?;
                let result: Result<()> = (|| {
                    // Validate both addresses before the callback-free transaction.
                    self.stack_parts(slot)?;
                    let item = self.take_stack_for_move(source)?;
                    let (generations, current, index) = self.stack_parts_mut(slot)?;
                    generations.replace(index, current, item);
                    let destination = self.stack_ref(slot)?;
                    self.forward_moved_stack(source, destination)
                })();
                self.release_stack_owner(slot.container);
                result?;
            }
        }
        Ok(())
    }
}

/// Stack roots live for one clicked/quickMoveStack/doClick operation, including
/// all synchronous callbacks. Unwinding the operation releases forwarding chains.
pub struct StackOperation<'a> {
    pub level: &'a mut Level,
    roots: Vec<StackRef>,
}
impl<'a> StackOperation<'a> {
    pub fn new(level: &'a mut Level) -> Self {
        Self {
            level,
            roots: Vec::new(),
        }
    }
    /// Own a fresh split/remove/copy result in the player's canonical operation
    /// arena. Its root and forwarding destination retire when this frame ends.
    pub fn own(&mut self, player: EntityRef, stack: ItemStack) -> Result<StackRef> {
        let local = self
            .level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .operation_stacks
            .insert(stack);
        let reference = StackRef {
            container: ContainerHandle::MenuOperationStacks { player },
            slot: local.slot,
            generation: local.generation,
        };
        self.roots.push(reference);
        Ok(reference)
    }
    /// Run native lazy getItem, then pin the receiver it returned. Owner roots
    /// keep a replaced block or closed menu addressable throughout that read.
    pub fn pin(&mut self, slot: ContainerSlot) -> Result<StackRef> {
        let reference = self.level.pin_stack(slot)?;
        self.roots.push(reference);
        Ok(reference)
    }
    pub fn pin_existing(&mut self, reference: StackRef) -> Result<StackRef> {
        self.level.retain_stack(reference)?;
        self.roots.push(reference);
        Ok(reference)
    }
    /// ContainerHelper#takeItem without losing the returned receiver's identity.
    /// Unlike removeItem/split, this returns an Existing object, not a copy.
    pub fn remove_no_update(&mut self, slot: ContainerSlot) -> Result<StackValue> {
        let reference = self.pin(slot)?;
        if self.level.stack(reference)?.is_empty()
            && !matches!(slot.container, ContainerHandle::PlayerInventory(_))
        {
            return Ok(StackValue::New(ItemStack::default()));
        }
        self.level
            .replace_stack_object(slot, StackValue::New(ItemStack::default()))?;
        Ok(StackValue::Existing(reference))
    }
}
impl Drop for StackOperation<'_> {
    fn drop(&mut self) {
        for &reference in self.roots.iter().rev() {
            self.level.release_stack(reference);
        }
    }
}

/// Port of handlePlayerAction's hand exchange. stopUsingItem follows in the
/// listener after this atomic exchange; both old receivers remain addressable.
pub fn swap_player_hands(player: EntityRef, level: &mut Level) -> Result<()> {
    let selected = level
        .entity::<ServerPlayer>(player)?
        .inventory
        .selected_slot();
    let mut operation = StackOperation::new(level);
    let address = |slot| ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot,
    };
    let main = operation.pin(address(selected))?;
    let offhand = operation.pin(address(40))?;
    operation
        .level
        .replace_stack_object(address(40), StackValue::Existing(main))?;
    operation
        .level
        .replace_stack_object(address(selected), StackValue::Existing(offhand))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{
            EntityState,
            player::{
                GameType,
                inventory::{count, set_count},
            },
        },
        level::Region,
    };
    use ferraloom_data::Item;
    use ferraloom_util::{Uuid, Vec3};

    #[test]
    fn item_stack_roots_preserve_removed_owners_and_replaced_receivers() {
        use crate::entity::item::ItemEntity;
        let mut level = crate::entity::tests::level(false);
        let item = ItemEntity::new_at(
            &mut level,
            Vec3::new(8.5, 64.0, 8.5),
            ItemStack::new(Item::STONE, 7, Default::default()),
        );
        let entity = level.next_entity_ref();
        let uuid = item.parent.uuid;
        level
            .add_entity(entity, EntityState::Item(Box::new(item)))
            .unwrap();
        let address = ContainerSlot {
            container: ContainerHandle::Entity(entity),
            slot: 0,
        };
        let held;
        {
            let mut operation = StackOperation::new(&mut level);
            assert!(matches!(
                operation.pin(ContainerSlot { slot: 1, ..address }),
                Err(MenuError::InvalidSlot(1))
            ));
            held = operation.pin(address).unwrap();
            operation.level.retain_stack(held).unwrap();
            operation.level.remove_entity(entity).unwrap();
            operation.level.finish_entity_management();
            assert_eq!(operation.level.entity_by_uuid(uuid), None);
            operation
                .level
                .entity_mut::<ItemEntity>(entity)
                .unwrap()
                .set_item(ItemStack::new(Item::DIRT, 3, Default::default()));
            set_count(operation.level.stack_mut(held).unwrap(), 2);
            assert_eq!(count(operation.level.stack(held).unwrap()), 2);
            assert_eq!(
                count(operation.level.entity::<ItemEntity>(entity).unwrap().item()),
                3
            );
        }
        level.finish_entity_management();
        assert_eq!(count(level.stack(held).unwrap()), 2);
        level.release_stack(held);
        level.finish_entity_management();
        assert!(level.entity_state(entity).is_err());
        assert!(level.stack(held).is_err());
    }

    #[test]
    fn fresh_operation_objects_follow_moves_and_retire_after_the_final_root() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut checked = 0;
        for line in include_str!("../../tests/java/inventory/listeners.tsv")
            .lines()
            .filter(|line| line.starts_with("listener\toperation-"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
            let mut state = ServerPlayer::new(
                Uuid::new(0, 1),
                "operation".into(),
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
            let destination = ContainerSlot {
                container: ContainerHandle::PlayerInventory(player),
                slot: 1,
            };
            let held;
            {
                let mut operation = StackOperation::new(&mut level);
                let source = operation
                    .pin(ContainerSlot {
                        slot: 0,
                        ..destination
                    })
                    .unwrap();
                let fresh = if fields[1] == "operation-copy" {
                    operation.level.stack(source).unwrap().copy_with_count(4)
                } else {
                    super::super::split(operation.level.stack_mut(source).unwrap(), 4)
                };
                held = operation.own(player, fresh).unwrap();
                let describe = |stack: &ItemStack| {
                    format!(
                        "{},{},{}",
                        stack.item().protocol_id(),
                        count(stack),
                        super::super::max_stack_size(stack)
                    )
                };
                let mut events = vec![format!(
                    "source:{}",
                    describe(operation.level.stack(source).unwrap())
                )];
                operation
                    .level
                    .replace_stack_object(destination, StackValue::Existing(held))
                    .unwrap();
                // A count edit through the new slot observes the same object.
                let installed = operation.level.stack_ref(destination).unwrap();
                set_count(operation.level.stack_mut(installed).unwrap(), 5);
                assert_eq!(count(operation.level.stack(held).unwrap()), 5);
                set_count(operation.level.stack_mut(held).unwrap(), 4);
                events.push("alias:true".into());
                operation
                    .level
                    .replace_stack_object(
                        destination,
                        StackValue::New(ItemStack::new(Item::DIRT, 3, Default::default())),
                    )
                    .unwrap();
                set_count(operation.level.stack_mut(held).unwrap(), 6);
                events.push(format!(
                    "destination:{}",
                    describe(
                        operation
                            .level
                            .stack(operation.level.stack_ref(destination).unwrap())
                            .unwrap()
                    )
                ));
                events.push(format!(
                    "held:{}",
                    describe(operation.level.stack(held).unwrap())
                ));
                assert_eq!(events.join(";"), fields[2], "{line}");
                // An enclosing callback can outlive the operation that made it.
                operation.level.retain_stack(held).unwrap();
            }
            assert_eq!(count(level.stack(held).unwrap()), 6);
            level.release_stack(held);
            assert!(level.stack(held).is_err());
            {
                let mut next = StackOperation::new(&mut level);
                let reused = next
                    .own(player, ItemStack::new(Item::DIAMOND, 1, Default::default()))
                    .unwrap();
                assert_eq!(reused.slot, held.slot);
                assert_ne!(reused.generation, held.generation);
                assert!(next.level.stack(held).is_err());
            }
            checked += 1;
        }
        assert_eq!(checked, 2);
    }

    #[test]
    fn hand_exchange_preserves_outer_roots_and_replaced_moved_receivers() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::from_int_array([0, 0, 0, 1]),
                    "stacks".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let address = |slot| ContainerSlot {
            container: ContainerHandle::PlayerInventory(player),
            slot,
        };
        level
            .replace_stack_object(
                address(0),
                StackValue::New(ItemStack::new(Item::STONE, 10, Default::default())),
            )
            .unwrap();
        level
            .replace_stack_object(
                address(40),
                StackValue::New(ItemStack::new(Item::DIAMOND, 7, Default::default())),
            )
            .unwrap();
        let (main, offhand);
        {
            let mut operation = StackOperation::new(&mut level);
            main = operation.pin(address(0)).unwrap();
            offhand = operation.pin(address(40)).unwrap();
            swap_player_hands(player, operation.level).unwrap();
            assert_eq!(
                operation
                    .level
                    .stack(main)
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .item,
                Item::STONE
            );
            assert_eq!(
                operation
                    .level
                    .stack(offhand)
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .item,
                Item::DIAMOND
            );
            set_count(operation.level.stack_mut(main).unwrap(), 8);
            let moved = operation.level.stack_ref(address(40)).unwrap();
            assert_eq!(count(operation.level.stack(moved).unwrap()), 8);
            operation
                .level
                .replace_stack_object(address(40), StackValue::Existing(main))
                .unwrap();
            assert_eq!(operation.level.stack_ref(address(40)).unwrap(), moved);
            operation
                .level
                .replace_stack_object(
                    address(40),
                    StackValue::New(ItemStack::new(Item::DIRT, 5, Default::default())),
                )
                .unwrap();
            set_count(operation.level.stack_mut(main).unwrap(), 2);
            assert_eq!(count(operation.level.stack(main).unwrap()), 2);
            let replacement = operation.level.stack_ref(address(40)).unwrap();
            assert_eq!(
                operation
                    .level
                    .stack(replacement)
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .item,
                Item::DIRT
            );
            assert_eq!(count(operation.level.stack(replacement).unwrap()), 5);
        }
        assert!(matches!(level.stack(main), Err(MenuError::MissingStack(_))));
        assert!(matches!(
            level.stack(offhand),
            Err(MenuError::MissingStack(_))
        ));
        assert_eq!(
            level
                .stack(level.stack_ref(address(0)).unwrap())
                .unwrap()
                .0
                .as_ref()
                .unwrap()
                .item,
            Item::DIAMOND
        );
    }
    #[test]
    fn stack_roots_keep_detached_block_menu_and_cursor_receivers_alive() {
        use crate::{
            block::entity::create,
            inventory::{AbstractContainerMenu, MenuKind},
            level::chunk::LevelChunk,
        };
        use ferraloom_data::Block;
        use ferraloom_util::{BlockPos, ChunkPos};
        use std::sync::Arc;
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::from_int_array([0, 0, 0, 1]),
                    "stacks".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let pos = BlockPos::new(3, 3, 3);
        let section = ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
            )),
            ferraloom_chunk::BiomeId(0),
        );
        let state = Block::CHEST.default_state();
        let mut chunk = LevelChunk::new(ChunkPos::new(0, 0), 0..1, section);
        chunk.set_block_state(pos, state);
        level.add_chunk(chunk).unwrap();
        let mut receiver = create(pos, state).unwrap();
        receiver
            .container_mut()
            .unwrap()
            .set_item(0, ItemStack::new(Item::STONE, 5, Default::default()));
        level.set_block_entity(receiver).unwrap();
        let original = level.block_entity_ref(pos).unwrap().unwrap();
        let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 1 }, 7).unwrap();
        menu.block_entities.push(original);
        menu.local[0].set_item(0, ItemStack::new(Item::DIAMOND, 7, Default::default()));
        menu.set_carried(ItemStack::new(Item::GOLD_INGOT, 3, Default::default()));
        let menu = level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .menus
            .install_container_menu(menu);
        let local = ContainerSlot {
            container: ContainerHandle::MenuLocal {
                player,
                menu,
                container: 0,
            },
            slot: 0,
        };
        let cursor = ContainerSlot {
            container: ContainerHandle::MenuCarried { player, menu },
            slot: 0,
        };
        {
            let mut operation = StackOperation::new(&mut level);
            let block = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::BlockEntity(original),
                    slot: 0,
                })
                .unwrap();
            let diamond = operation.pin(local).unwrap();
            let gold = operation.pin(cursor).unwrap();
            let removed = operation.remove_no_update(block.address()).unwrap();
            operation
                .level
                .replace_stack_object(local, removed)
                .unwrap();
            operation
                .level
                .replace_stack_object(cursor, StackValue::Existing(diamond))
                .unwrap();
            operation
                .level
                .set_block_entity(create(pos, state).unwrap())
                .unwrap();
            operation
                .level
                .entity_mut::<ServerPlayer>(player)
                .unwrap()
                .menus
                .install_container_menu(
                    AbstractContainerMenu::new(MenuKind::Chest { rows: 1 }, 7).unwrap(),
                );
            operation.level.collect_detached_block_entities();
            set_count(operation.level.stack_mut(block).unwrap(), 2);
            set_count(operation.level.stack_mut(gold).unwrap(), 1);
            assert_eq!(count(operation.level.stack(block).unwrap()), 2);
            assert_eq!(count(operation.level.stack(gold).unwrap()), 1);
            assert_eq!(
                operation
                    .level
                    .stack(diamond)
                    .unwrap()
                    .0
                    .as_ref()
                    .unwrap()
                    .item,
                Item::DIAMOND
            );
            assert_eq!(
                count(
                    operation
                        .level
                        .stack(operation.level.stack_ref(local).unwrap())
                        .unwrap()
                ),
                2
            );
            assert_eq!(
                count(
                    operation
                        .level
                        .stack(operation.level.stack_ref(cursor).unwrap())
                        .unwrap()
                ),
                7
            );
            assert!(
                operation
                    .level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .menus
                    .current()
                    .carried
                    .is_empty()
            );
            assert!(operation.level.block_entity_state(original).is_ok());
        }
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(original).is_err());
        assert!(matches!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .menu(menu),
            Err(MenuError::MissingMenu(_))
        ));
    }
}
