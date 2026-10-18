//! Canonical stack receiver access. An operation owns roots, never inventories.
use super::{
    ContainerHandle, ContainerSlot, ItemStack, MenuError, Result, StackForward, StackGenerations,
    StackRef, StackSlotRef, StackValue,
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
            _ => Err(MenuError::Unsupported("container stack generation storage")),
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
            _ => Err(MenuError::Unsupported("container stack generation storage")),
        }
    }
    /// Capture identity after the caller's native getItem has finished.
    pub fn stack_ref(&self, slot: ContainerSlot) -> Result<StackRef> {
        let (generations, _, index) = self.stack_parts(slot)?;
        Ok(StackRef {
            container: slot.container,
            slot: slot.slot,
            generation: generations.current(index).unwrap().generation,
        })
    }
    pub fn retain_stack(&mut self, reference: StackRef) -> Result<()> {
        let (generations, _, index) = self.stack_parts_mut(reference.address())?;
        if !generations.retain(StackSlotRef {
            slot: index,
            generation: reference.generation,
        }) {
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
        let reference = self.level.stack_ref(slot)?;
        self.level.retain_stack(reference)?;
        self.roots.push(reference);
        Ok(reference)
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
}
