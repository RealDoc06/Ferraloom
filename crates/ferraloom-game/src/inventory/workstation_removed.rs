//! Workstation removed callbacks over the same retained-object algorithm in the
//! live Level and controlled standalone menu adapters.
use super::inventory_return::{InventoryReturnContext, place_item_back};
use super::*;
use crate::entity::{EntityData, RemovalReason};

enum RemovalEffect<D> {
    Slot(usize),
    Drop(D),
}
trait WorkstationRemovalContext: InventoryReturnContext {
    fn may_return_inventory(&self, disconnected: bool) -> Result<bool>;
    fn removal_access_available(&self) -> Result<bool>;
    fn take_local_no_update(&mut self, container: usize, slot: usize) -> Result<Self::Stack>;
}
fn drop_or_place<C: WorkstationRemovalContext>(
    context: &mut C,
    stack: C::Stack,
    disconnected: bool,
    effects: &mut impl FnMut(&mut C, RemovalEffect<C::DroppedAddress>) -> Result<()>,
) -> Result<()> {
    if context.may_return_inventory(disconnected)? {
        if let Some(destination) = place_item_back(context, stack, |context, slot| {
            effects(context, RemovalEffect::Slot(slot))
        })? {
            effects(context, RemovalEffect::Drop(destination))?;
        }
    } else if !context.read_return_stack(stack)?.is_empty() {
        let destination = context.drop_for_return(stack)?;
        effects(context, RemovalEffect::Drop(destination))?;
    }
    Ok(())
}
/// Port of AbstractContainerMenu#removed, dropOrPlaceInInventory and each
/// workstation's access.execute(clearContainer). No input getter spans a call.
fn removed<C: WorkstationRemovalContext>(
    context: &mut C,
    disconnected: bool,
    mut effects: impl FnMut(&mut C, RemovalEffect<C::DroppedAddress>) -> Result<()>,
) -> Result<()> {
    let carried = context.carried_stack()?;
    if !context.read_return_stack(carried)?.is_empty() {
        drop_or_place(context, carried, disconnected, &mut effects)?;
        let empty = context.own_stack(ItemStack::default())?;
        context.set_carried_stack(empty)?;
    }
    match context.menu()?.kind {
        MenuKind::Stonecutter => {
            context.take_local_no_update(1, 1)?;
        }
        MenuKind::Cartography => {
            context.take_local_no_update(1, 2)?;
        }
        _ => {}
    }
    // The callback tests only captured dimension availability, never whether
    // the workstation block still exists: broken tables must return their grid.
    if context.removal_access_available()? {
        let count = match context.menu()?.kind {
            MenuKind::Crafting => 9,
            MenuKind::Anvil | MenuKind::Grindstone | MenuKind::Cartography => 2,
            MenuKind::Loom | MenuKind::Smithing => 3,
            MenuKind::Stonecutter => 1,
            _ => return Err(MenuError::Unsupported("workstation removed kind")),
        };
        for slot in 0..count {
            let stack = context.take_local_no_update(0, slot)?;
            drop_or_place(context, stack, disconnected, &mut effects)?;
        }
    }
    Ok(())
}
impl WorkstationRemovalContext for super::in_memory::InMemoryMenuOperation<'_, '_, '_> {
    fn may_return_inventory(&self, disconnected: bool) -> Result<bool> {
        Ok(!disconnected)
    }
    fn removal_access_available(&self) -> Result<bool> {
        // Legacy pure methods are adapters over an available in-memory context.
        // Actual runtime NULL/dimension access is resolved by the live adapter.
        Ok(true)
    }
    fn take_local_no_update(&mut self, container: usize, slot: usize) -> Result<Self::Stack> {
        use super::in_memory::MemoryContainer;
        let owner = MemoryContainer::Container(ContainerId::Local(container));
        let held = self.capture(owner, slot)?;
        if self.read(held)?.is_empty() {
            return Ok(self.own(ItemStack::default()));
        }
        let empty = self.own(ItemStack::default());
        self.replace(owner, slot, empty)?;
        Ok(held)
    }
}
impl WorkstationRemovalContext for super::transfer::LiveMenuTransfer<'_> {
    fn may_return_inventory(&self, disconnected: bool) -> Result<bool> {
        let entity = self.operation.level.entity::<EntityData>(self.player)?;
        Ok(!disconnected
            && (!entity.removed || entity.removal_reason == Some(RemovalReason::ChangedDimension)))
    }
    fn removal_access_available(&self) -> Result<bool> {
        let Some(access) = &self.menu()?.level_access else {
            return Ok(false);
        };
        Ok(access
            .evaluate(self.operation.level, |_, _| Ok(true))?
            .unwrap_or(false))
    }
    fn take_local_no_update(&mut self, container: usize, slot: usize) -> Result<StackRef> {
        let address = ContainerSlot {
            container: ContainerHandle::MenuLocal {
                player: self.player,
                menu: self.menu,
                container,
            },
            slot,
        };
        let held = self.operation.pin(address)?;
        if self.operation.level.stack(held)?.is_empty() {
            return self.operation.own(self.player, ItemStack::default());
        }
        self.operation
            .level
            .replace_stack_object(address, StackValue::New(ItemStack::default()))?;
        Ok(held)
    }
}

pub(super) fn removed_pure(
    menu: &mut AbstractContainerMenu,
    context: &mut MenuContext<'_>,
    disconnected: bool,
) -> Vec<crate::entity::player::inventory::InventoryEffect> {
    use crate::entity::player::inventory::InventoryEffect;
    // The value API has always required unrooted values. Canonical callbacks
    // use close and can keep their old menu/stack generations retained.
    assert!(!menu.carried_generations.retains_slot(0));
    for container in &menu.local {
        for slot in 0..container.size() {
            if let Some((generations, _, index)) = container.stack_parts(slot) {
                assert!(!generations.is_retained(generations.current(index).unwrap()));
            }
        }
    }
    enum Output {
        Slot(InventoryEffect),
        Drop(usize),
    }
    let first_drop = context.drops.len();
    let mut output = Vec::new();
    {
        let mut operation = super::in_memory::InMemoryMenuOperation::new(menu, context);
        removed(&mut operation, disconnected, |operation, effect| {
            output.push(match effect {
                RemovalEffect::Slot(slot) => Output::Slot(InventoryEffect::SetSlot {
                    slot,
                    stack: operation
                        .context
                        .player
                        .inventory
                        .get_item(slot, operation.context.player.equipment)
                        .clone(),
                }),
                RemovalEffect::Drop(index) => Output::Drop(index),
            });
            Ok(())
        })
        .expect("standalone workstation removal has valid local slots");
    }
    let result = output
        .into_iter()
        .map(|event| match event {
            Output::Slot(effect) => effect,
            Output::Drop(index) => {
                InventoryEffect::Drop(std::mem::take(&mut context.drops[index].stack))
            }
        })
        .collect();
    context.drops.truncate(first_drop);
    result
}

/// Native close output enters the Level stream before removed callbacks. The
/// original receiver stays rooted; transferState reads current containerMenu
/// again after callbacks, just as ServerPlayer#doCloseContainer does.
pub(super) fn close(
    player: crate::entity::EntityRef,
    level: &mut crate::level::Level,
    send_close: bool,
    disconnected: bool,
    live: bool,
) -> Result<Vec<ferraloom_protocol::play::clientbound::Packet>> {
    use crate::{entity::player::ServerPlayer, level::effects::LevelEffect};
    use ferraloom_protocol::{codec::VarInt, play::clientbound as cb};
    if live {
        super::flush_container_packets(player, level)?;
    }
    let mut packets = Vec::new();
    let mut send = |level: &mut crate::level::Level, packet: cb::Packet| {
        if live {
            level.emit(LevelEffect::PlayerPacket {
                recipients: vec![player],
                packet: Box::new(packet),
            });
        } else {
            packets.push(packet);
        }
    };
    let receiver = level.entity::<ServerPlayer>(player)?.menus.current_ref();
    super::with_menu_callback(player, receiver, level, |context| {
        if send_close {
            send(
                context.level,
                cb::Packet::ContainerClose(cb::ContainerClose {
                    container_id: VarInt(receiver.container_id),
                }),
            );
        }
        let mut transfer = super::transfer::LiveMenuTransfer {
            operation: StackOperation::new(context.level),
            player,
            menu: receiver,
        };
        removed(&mut transfer, disconnected, |transfer, event| {
            if let RemovalEffect::Slot(slot) = event {
                let owner = transfer.operation.level.entity::<ServerPlayer>(player)?;
                let contents = owner
                    .inventory
                    .get_item(slot, &owner.parent.equipment)
                    .clone();
                send(
                    transfer.operation.level,
                    cb::Packet::SetPlayerInventory(cb::SetPlayerInventory {
                        slot: VarInt(slot as i32),
                        contents,
                    }),
                );
            }
            Ok(())
        })?;
        let level = &mut *transfer.operation.level;
        let current = level.entity::<ServerPlayer>(player)?.menus.current_ref();
        let roots = level
            .entity::<ServerPlayer>(player)?
            .menus
            .menu(current)?
            .block_entities
            .clone();
        let menus = &mut level.entity_mut::<ServerPlayer>(player)?.menus;
        menus.transfer_to_inventory_menu(current)?;
        menus.detach_menu(current);
        for root in roots {
            level.release_block_entity(root);
        }
        Ok(())
    })?;
    Ok(packets)
}

#[cfg(test)]
mod tests;
