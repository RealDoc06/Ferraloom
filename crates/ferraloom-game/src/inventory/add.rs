//! Inventory#add uses the same receiver algorithm for owned values and live
//! handles. Native copies create new objects; count edits resolve the held one.
use super::*;
use crate::{
    entity::{
        EntityRef,
        player::{ServerPlayer, equipment::EntityEquipment, inventory::Inventory},
    },
    level::Level,
};

pub trait InventoryAddContext {
    type Stack: Copy;
    fn stack(&self, reference: Self::Stack) -> Result<&ItemStack>;
    fn set_stack_count(&mut self, reference: Self::Stack, amount: i32) -> Result<()>;
    fn inventory_stack(&mut self, slot: usize) -> Result<Self::Stack>;
    fn set_inventory_stack(&mut self, slot: usize, stack: ItemStack) -> Result<Self::Stack>;
    fn set_inventory_pop_time(&mut self, slot: usize, ticks: i32) -> Result<()>;
    fn selected_slot(&self) -> Result<usize>;
    fn infinite_materials(&self) -> Result<bool>;
}

fn free_slot<C: InventoryAddContext>(context: &mut C) -> Result<Option<usize>> {
    for slot in 0..36 {
        let reference = context.inventory_stack(slot)?;
        if context.stack(reference)?.is_empty() {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

fn remaining_slot<C: InventoryAddContext>(
    context: &mut C,
    input: C::Stack,
) -> Result<Option<usize>> {
    let selected = context.selected_slot()?;
    for slot in [selected, 40].into_iter().chain(0..36) {
        let target = context.inventory_stack(slot)?;
        let target = context.stack(target)?;
        if !target.is_empty()
            && is_same_item_same_components(target, context.stack(input)?)
            && is_stackable(target)
            && count(target) < max_stack_size(target).min(99)
        {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Port of Inventory#addResource. Capture the input count before the target
/// getter and mutate the held target before the caller writes the remainder.
fn add_resource<C: InventoryAddContext>(
    context: &mut C,
    slot: usize,
    input: C::Stack,
) -> Result<i32> {
    let amount = count(context.stack(input)?);
    let mut target = context.inventory_stack(slot)?;
    if context.stack(target)?.is_empty() {
        let copy = with_count(context.stack(input)?, 0);
        target = context.set_inventory_stack(slot, copy)?;
    }
    let target_value = context.stack(target)?;
    let max = max_stack_size(target_value)
        .min(99)
        .wrapping_sub(count(target_value));
    let added = amount.min(max);
    if added == 0 {
        return Ok(amount);
    }
    let total = count(context.stack(target)?).wrapping_add(added);
    context.set_stack_count(target, total)?;
    context.set_inventory_pop_time(slot, 5)?;
    Ok(amount.wrapping_sub(added))
}

/// Port of both Inventory#add overloads, including the last-iteration return
/// value when an inventory fills after accepting only part of the input.
pub fn add_inventory_stack<C: InventoryAddContext>(
    context: &mut C,
    input: C::Stack,
    slot: Option<usize>,
) -> Result<bool> {
    if context.stack(input)?.is_empty() {
        return Ok(false);
    }
    if crate::entity::player::inventory::is_damaged(context.stack(input)?) {
        let destination = match slot {
            Some(slot) => Some(slot),
            None => free_slot(context)?,
        };
        if let Some(slot) = destination {
            if slot >= 36 {
                return Err(MenuError::InvalidSlot(slot as i32));
            }
            // ItemStack#copyAndClear clears its original before List#set.
            let copy = context.stack(input)?.clone();
            context.set_stack_count(input, 0)?;
            context.set_inventory_stack(slot, copy)?;
            context.set_inventory_pop_time(slot, 5)?;
            return Ok(true);
        }
        if context.infinite_materials()? {
            context.set_stack_count(input, 0)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let last_size = loop {
        let previous = count(context.stack(input)?);
        let destination = match slot {
            Some(slot) => Some(slot),
            None => match remaining_slot(context, input)? {
                Some(slot) => Some(slot),
                None => free_slot(context)?,
            },
        };
        let remaining = match destination {
            Some(slot) => add_resource(context, slot, input)?,
            None => count(context.stack(input)?),
        };
        context.set_stack_count(input, remaining)?;
        if context.stack(input)?.is_empty() || count(context.stack(input)?) >= previous {
            break previous;
        }
    };
    if count(context.stack(input)?) == last_size && context.infinite_materials()? {
        context.set_stack_count(input, 0)?;
        Ok(true)
    } else {
        Ok(count(context.stack(input)?) < last_size)
    }
}

#[derive(Clone, Copy)]
enum ValueStack {
    Input,
    Inventory(usize),
}
struct ValueInventoryAdd<'a> {
    inventory: &'a mut Inventory,
    equipment: &'a mut EntityEquipment,
    input: &'a mut ItemStack,
    infinite_materials: bool,
}
impl InventoryAddContext for ValueInventoryAdd<'_> {
    type Stack = ValueStack;
    fn stack(&self, reference: ValueStack) -> Result<&ItemStack> {
        Ok(match reference {
            ValueStack::Input => self.input,
            ValueStack::Inventory(slot) => self.inventory.get_item(slot, self.equipment),
        })
    }
    fn set_stack_count(&mut self, reference: ValueStack, amount: i32) -> Result<()> {
        let stack = match reference {
            ValueStack::Input => &mut *self.input,
            ValueStack::Inventory(slot) => self.inventory.get_item_mut(slot, self.equipment),
        };
        set_count(stack, amount);
        Ok(())
    }
    fn inventory_stack(&mut self, slot: usize) -> Result<ValueStack> {
        Ok(ValueStack::Inventory(slot))
    }
    fn set_inventory_stack(&mut self, slot: usize, stack: ItemStack) -> Result<ValueStack> {
        self.inventory.set_item(slot, stack, self.equipment);
        Ok(ValueStack::Inventory(slot))
    }
    fn set_inventory_pop_time(&mut self, slot: usize, ticks: i32) -> Result<()> {
        self.inventory.set_pop_time(slot, ticks);
        Ok(())
    }
    fn selected_slot(&self) -> Result<usize> {
        Ok(self.inventory.selected_slot())
    }
    fn infinite_materials(&self) -> Result<bool> {
        Ok(self.infinite_materials)
    }
}

/// Value-only compatibility adapter; held canonical receivers use the live path.
pub(crate) fn add_inventory_value(
    inventory: &mut Inventory,
    equipment: &mut EntityEquipment,
    input: &mut ItemStack,
    slot: Option<usize>,
    infinite_materials: bool,
) -> bool {
    add_inventory_stack(
        &mut ValueInventoryAdd {
            inventory,
            equipment,
            input,
            infinite_materials,
        },
        ValueStack::Input,
        slot,
    )
    .expect("value-only Inventory#add has valid canonical slots")
}

struct LiveInventoryAdd<'a> {
    operation: StackOperation<'a>,
    player: EntityRef,
}
impl InventoryAddContext for LiveInventoryAdd<'_> {
    type Stack = StackRef;
    fn stack(&self, reference: StackRef) -> Result<&ItemStack> {
        self.operation.level.stack(reference)
    }
    fn set_stack_count(&mut self, reference: StackRef, amount: i32) -> Result<()> {
        set_count(self.operation.level.stack_mut(reference)?, amount);
        Ok(())
    }
    fn inventory_stack(&mut self, slot: usize) -> Result<StackRef> {
        self.operation.pin(ContainerSlot {
            container: ContainerHandle::PlayerInventory(self.player),
            slot,
        })
    }
    fn set_inventory_stack(&mut self, slot: usize, stack: ItemStack) -> Result<StackRef> {
        self.operation.level.replace_stack_object(
            ContainerSlot {
                container: ContainerHandle::PlayerInventory(self.player),
                slot,
            },
            StackValue::New(stack),
        )?;
        self.inventory_stack(slot)
    }
    fn set_inventory_pop_time(&mut self, slot: usize, ticks: i32) -> Result<()> {
        self.operation
            .level
            .entity_mut::<ServerPlayer>(self.player)?
            .inventory
            .set_pop_time(slot, ticks);
        Ok(())
    }
    fn selected_slot(&self) -> Result<usize> {
        Ok(self
            .operation
            .level
            .entity::<ServerPlayer>(self.player)?
            .inventory
            .selected_slot())
    }
    fn infinite_materials(&self) -> Result<bool> {
        Ok(self
            .operation
            .level
            .entity::<ServerPlayer>(self.player)?
            .instabuild())
    }
}

/// Port of Player#addItem for a held stack. Replaced source slots stay detached;
/// input and destination count edits never take an inventory out of the level.
pub fn add_item_stack(player: EntityRef, input: StackRef, level: &mut Level) -> Result<bool> {
    add_item_stack_at(player, input, None, level)
}
pub(super) fn add_item_stack_at(
    player: EntityRef,
    input: StackRef,
    slot: Option<usize>,
    level: &mut Level,
) -> Result<bool> {
    let mut context = LiveInventoryAdd {
        operation: StackOperation::new(level),
        player,
    };
    context.operation.pin_existing(input)?;
    add_inventory_stack(&mut context, input, slot)
}

#[cfg(test)]
mod tests;
