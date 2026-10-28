//! SlotAccess receivers for Entity#getSlot. A receiver identifies the getter,
//! not a captured stack; callers pin each get result for their operation.
use super::stack::{item_data_error, receiver_error};
#[cfg(test)]
mod tests;
use crate::{
    entity::{
        EntityRef, EntityState, LivingEntity,
        item::ItemEntity,
        player::{ServerPlayer, equipment::SLOTS},
        synched_data::SynchedValue,
    },
    inventory::{Container, ContainerHandle, ContainerSlot, StackOperation, StackRef, StackValue},
    level::{Level, LevelError},
};
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, EquipmentSlot, ItemStack};

/// Port of SlotAccess. The provider's owner must remain retained while this
/// receiver is held; StackOperation pins only the particular get() results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotAccess {
    entity: EntityRef,
    receiver: Receiver,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Receiver {
    Inventory(usize),
    EnderChest(usize),
    Equipment(EquipmentSlot),
    Carried,
    Crafting(usize),
    Item,
}
impl SlotAccess {
    fn address(self, level: &Level) -> Result<ContainerSlot, LevelError> {
        let (container, slot) = match self.receiver {
            Receiver::Inventory(slot) => (ContainerHandle::PlayerInventory(self.entity), slot),
            Receiver::EnderChest(slot) => (ContainerHandle::PlayerEnderChest(self.entity), slot),
            Receiver::Equipment(slot) => {
                if slot == EquipmentSlot::Mainhand
                    && let EntityState::Player(player) = level.entity_state(self.entity)?
                {
                    (
                        ContainerHandle::PlayerInventory(self.entity),
                        player.inventory.selected_slot(),
                    )
                } else {
                    (
                        ContainerHandle::LivingEquipment(self.entity),
                        SLOTS
                            .iter()
                            .position(|&s| s == slot)
                            .expect("native equipment slot"),
                    )
                }
            }
            Receiver::Carried => (
                ContainerHandle::MenuCarried {
                    player: self.entity,
                    menu: level
                        .entity::<ServerPlayer>(self.entity)?
                        .menus
                        .current_ref(),
                },
                0,
            ),
            Receiver::Crafting(slot) => (
                ContainerHandle::MenuLocal {
                    player: self.entity,
                    menu: level
                        .entity::<ServerPlayer>(self.entity)?
                        .menus
                        .inventory_menu
                        .reference(),
                    container: 0,
                },
                slot,
            ),
            Receiver::Item => (ContainerHandle::Entity(self.entity), 0),
        };
        Ok(ContainerSlot { container, slot })
    }
    /// Port of SlotAccess#get. Player cursor and main hand resolve the current
    /// menu and selection at this call, including changes since getSlot.
    pub fn get(self, level: &Level) -> Result<StackRef, LevelError> {
        level
            .stack_ref(self.address(level)?)
            .map_err(receiver_error)
    }
    /// Port of SlotAccess#set. Rejections leave storage untouched. Native
    /// container/equipment callbacks finish before this operation returns.
    pub fn set(
        self,
        value: StackValue,
        operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        operation.level.retain_entity(self.entity)?;
        let result = self.set_retained(value, operation);
        operation.level.release_entity(self.entity);
        result
    }
    fn set_retained(
        self,
        value: StackValue,
        operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        if let StackValue::Existing(reference) = &value {
            operation.pin_existing(*reference).map_err(receiver_error)?;
        }
        if let Receiver::Equipment(slot) = self.receiver {
            let input = match &value {
                StackValue::New(stack) => stack,
                StackValue::Existing(reference) => {
                    operation.level.stack(*reference).map_err(receiver_error)?
                }
            };
            if !matches!(
                slot,
                EquipmentSlot::Head | EquipmentSlot::Mainhand | EquipmentSlot::Offhand
            ) && !input.is_empty()
                && equipment_slot_for_item(self.entity, input, operation.level)? != slot
            {
                return Ok(false);
            }
            operation
                .level
                .entity_behavior(self.entity)?
                .set_item_slot(self.entity, slot, value, operation)?;
            return Ok(true);
        }
        let address = self.address(operation.level)?;
        let old = operation.pin(address).map_err(receiver_error)?;
        if self.receiver == Receiver::Item {
            // SynchedEntityData#set keeps the original object for equal values.
            let current = operation.level.stack(old).map_err(receiver_error)?;
            let input = match &value {
                StackValue::New(stack) => stack,
                StackValue::Existing(reference) => {
                    operation.level.stack(*reference).map_err(receiver_error)?
                }
            };
            if current.same_value(input) {
                return Ok(true);
            }
            match value {
                StackValue::New(stack) => operation
                    .level
                    .entity_mut::<ItemEntity>(self.entity)?
                    .set_item(stack),
                StackValue::Existing(source) => {
                    let stack = operation
                        .level
                        .take_stack_for_move(source)
                        .map_err(receiver_error)?;
                    operation
                        .level
                        .entity_mut::<ItemEntity>(self.entity)?
                        .set_item(stack);
                    let destination = operation.level.stack_ref(address).map_err(receiver_error)?;
                    operation
                        .level
                        .forward_moved_stack(source, destination)
                        .map_err(receiver_error)?;
                }
            }
            return Ok(true);
        }
        operation
            .level
            .replace_stack_object(address, value)
            .map_err(receiver_error)?;
        match self.receiver {
            Receiver::Equipment(_) => unreachable!("equipment virtual handled above"),
            Receiver::EnderChest(slot) => operation
                .level
                .entity_mut::<ServerPlayer>(self.entity)?
                .get_ender_chest_inventory_mut()
                .contents_mut()
                .reassign_item(slot)
                .map_err(receiver_error)?,
            Receiver::Crafting(slot) => {
                let ContainerHandle::MenuLocal { menu, .. } = address.container else {
                    unreachable!("crafting container")
                };
                operation
                    .level
                    .entity_mut::<ServerPlayer>(self.entity)?
                    .menus
                    .menu_mut(menu)
                    .map_err(receiver_error)?
                    .local[0]
                    .reassign_item(slot)
                    .map_err(receiver_error)?;
                let limited = operation.level.rules().limited_crafting;
                crate::inventory::refresh_live_crafting_result(
                    self.entity,
                    menu,
                    limited,
                    operation.level,
                )
                .map_err(receiver_error)?;
                // TransientCraftingContainer#setItem calls slotsChanged, then
                // Player#getSlot's setter explicitly calls it a second time.
                let limited = operation.level.rules().limited_crafting;
                crate::inventory::refresh_live_crafting_result(
                    self.entity,
                    menu,
                    limited,
                    operation.level,
                )
                .map_err(receiver_error)?;
            }
            Receiver::Inventory(_) | Receiver::Carried => {}
            Receiver::Item => unreachable!("item metadata setter handled above"),
        }
        Ok(true)
    }
}

/// LivingEntity#setItemSlot's parent assignment, including PlayerEquipment's
/// selected main hand. The caller retains the owner throughout this callback.
pub(crate) fn set_equipment_value(
    entity: EntityRef,
    slot: EquipmentSlot,
    value: StackValue,
    operation: &mut StackOperation<'_>,
) -> Result<(), LevelError> {
    operation.level.entity::<LivingEntity>(entity)?;
    let address = SlotAccess {
        entity,
        receiver: Receiver::Equipment(slot),
    }
    .address(operation.level)?;
    if let StackValue::Existing(source) = &value {
        operation.pin_existing(*source).map_err(receiver_error)?;
    }
    let old = operation.pin(address).map_err(receiver_error)?;
    operation
        .level
        .replace_stack_object(address, value)
        .map_err(receiver_error)?;
    let new = operation.pin(address).map_err(receiver_error)?;
    operation
        .level
        .entity_behavior(entity)?
        .on_equip_item(entity, slot, old, new, operation.level)
}

/// Port of LivingEntity#getEquipmentSlotForItem. allowedEntities is deliberately
/// not part of this native query or createEquipmentSlotAccess's validator.
fn equipment_slot_for_item(
    entity: EntityRef,
    stack: &ItemStack,
    level: &Level,
) -> Result<EquipmentSlot, LevelError> {
    if let Some(DataComponent::Equippable(equippable)) = stack
        .component(ComponentTypeId::EQUIPPABLE)
        .map_err(|e| item_data_error(stack.item(), e))?
        && level
            .entity_behavior(entity)?
            .can_use_slot(entity, equippable.slot, level)?
    {
        return Ok(equippable.slot);
    }
    Ok(EquipmentSlot::Mainhand)
}
/// Port of LivingEntity#getSlot and its native command slot IDs, independent
/// of the equipment wire IDs and the inventory's equipment-slot projection.
pub(crate) fn living_access(
    entity: EntityRef,
    slot: i32,
    level: &Level,
) -> Result<Option<SlotAccess>, LevelError> {
    if level.entity_state(entity)?.as_living().is_none() {
        return Ok(None);
    }
    let equipment = match slot {
        98 => EquipmentSlot::Mainhand,
        99 => EquipmentSlot::Offhand,
        100 => EquipmentSlot::Feet,
        101 => EquipmentSlot::Legs,
        102 => EquipmentSlot::Chest,
        103 => EquipmentSlot::Head,
        105 => EquipmentSlot::Body,
        106 => EquipmentSlot::Saddle,
        _ => return Ok(None),
    };
    Ok(Some(SlotAccess {
        entity,
        receiver: Receiver::Equipment(equipment),
    }))
}
pub(crate) fn player_access(
    entity: EntityRef,
    slot: i32,
    level: &Level,
) -> Result<Option<SlotAccess>, LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    let receiver = if slot == 499 {
        Receiver::Carried
    } else if (500..504).contains(&slot) {
        Receiver::Crafting((slot - 500) as usize)
    } else if slot >= 0 && (slot as usize) < player.inventory.items.len() {
        Receiver::Inventory(slot as usize)
    } else if slot >= 200
        && ((slot - 200) as usize) < player.get_ender_chest_inventory().contents().size()
    {
        Receiver::EnderChest((slot - 200) as usize)
    } else {
        return living_access(entity, slot, level);
    };
    Ok(Some(SlotAccess { entity, receiver }))
}
pub(crate) fn item_access(
    entity: EntityRef,
    slot: i32,
    level: &Level,
) -> Result<Option<SlotAccess>, LevelError> {
    level.entity::<ItemEntity>(entity)?;
    Ok((slot == 0).then_some(SlotAccess {
        entity,
        receiver: Receiver::Item,
    }))
}
