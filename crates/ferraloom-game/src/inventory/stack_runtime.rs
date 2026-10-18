//! Canonical stack receiver access. An operation owns roots, never inventories.
use super::{
    Container, ContainerHandle, ContainerSlot, ItemStack, MenuError, Result, StackForward,
    StackGenerations, StackRef, StackSlotRef, StackValue,
};
use crate::{
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};

impl Level {
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
            ContainerHandle::Entity(_) => {
                Err(MenuError::Unsupported("entity stack generation storage"))
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
            ContainerHandle::Entity(_) => {
                Err(MenuError::Unsupported("entity stack generation storage"))
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
            ContainerHandle::PlayerInventory(_) => {}
            ContainerHandle::Entity(_) => {
                return Err(MenuError::Unsupported("entity stack generation storage"));
            }
        }
        Ok(())
    }
    fn release_stack_owner(&mut self, owner: ContainerHandle) {
        match owner {
            ContainerHandle::BlockEntity(handle) => self.release_block_entity(handle),
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
        let (generations, item, index) = self.stack_parts(slot)?;
        Ok(!item.is_empty() && generations.is_retained(generations.current(index).unwrap()))
    }
    pub fn retain_stack(&mut self, reference: StackRef) -> Result<()> {
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
    /// Replace/move the object atomically, before the container invokes any
    /// native callbacks. The caller keeps the Existing receiver rooted.
    pub fn replace_stack_object(&mut self, slot: ContainerSlot, value: StackValue) -> Result<()> {
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
                    let (generations, current, index) = self.stack_parts_mut(source.address())?;
                    let local = StackSlotRef {
                        slot: index,
                        generation: source.generation,
                    };
                    let item = generations
                        .take_for_move(local, current)
                        .ok_or(MenuError::MissingStack(original))?;
                    let (generations, current, index) = self.stack_parts_mut(slot)?;
                    generations.replace(index, current, item);
                    let destination = self.stack_ref(slot)?;
                    self.retain_stack(destination)?;
                    let (generations, _, _) = self.stack_parts_mut(source.address())?;
                    generations.forward_to(local, destination);
                    Ok(())
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
    /// Player inventories have a direct getItem. Block/menu callers must finish
    /// their native lazy getItem first, then retain that returned receiver.
    pub fn pin(&mut self, slot: ContainerSlot) -> Result<StackRef> {
        self.level.retain_stack_owner(slot.container)?;
        let result: Result<StackRef> = (|| {
            if let ContainerHandle::BlockEntity(handle) = slot.container {
                crate::block::containers::access::get_item(handle, slot.slot, self.level)?;
            }
            let reference = self.level.stack_ref(slot)?;
            self.level.retain_stack(reference)?;
            Ok(reference)
        })();
        self.level.release_stack_owner(slot.container);
        let reference = result?;
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
        if self.level.stack(reference)?.is_empty() {
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
    fn hand_exchange_preserves_outer_roots_and_replaced_moved_receivers() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let player = EntityRef(1);
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
        let player = EntityRef(1);
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
