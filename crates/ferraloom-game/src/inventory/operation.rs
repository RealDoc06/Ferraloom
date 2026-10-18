//! Shared menu synchronization over short-lived canonical or in-memory views.
use super::*;

pub trait MenuSynchronizationContext {
    fn menu(&self) -> Result<&AbstractContainerMenu>;
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu>;
    fn item_at(&mut self, slot: usize) -> Result<ItemStack>;
    fn data_at(&mut self, index: usize) -> Result<i32>;
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        self.menu_mut()?.updates.push(update);
        Ok(())
    }
}
pub(crate) struct PureMenuSynchronization<'a, 'b, 'c> {
    pub menu: &'a mut AbstractContainerMenu,
    pub context: &'b MenuContext<'c>,
}
impl MenuSynchronizationContext for PureMenuSynchronization<'_, '_, '_> {
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn item_at(&mut self, slot: usize) -> Result<ItemStack> {
        Ok(self.menu.get_item(self.context, slot))
    }
    fn data_at(&mut self, index: usize) -> Result<i32> {
        Ok(self.menu.data[index])
    }
}

impl MenuSynchronizationContext for LiveMenuContext<'_> {
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        self.level
            .entity_mut::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .menu_mut(self.menu)
    }
    fn item_at(&mut self, index: usize) -> Result<ItemStack> {
        self.get_item(self.slot_address(index)?)
    }

    fn data_at(&mut self, index: usize) -> Result<i32> {
        use crate::block::entity::BlockEntityState;
        if let Some(&handle) = self.menu()?.block_entities.first() {
            match self.level.block_entity_state(handle)? {
                BlockEntityState::Furnace(furnace) => return Ok(furnace.data()[index]),
                BlockEntityState::Crafter(crafter) => {
                    return Ok(if index == 9 {
                        crafter.triggered
                    } else {
                        i32::from(crafter.disabled_slots[index])
                    });
                }
                _ => {}
            }
        }
        Ok(self.menu()?.data[index])
    }
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        self.level
            .entity_mut::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .pending_packets
            .extend(update.into_packets());
        Ok(())
    }
}

/// Port of AbstractContainerMenu#broadcastChanges. Resolve each getItem once,
/// then notify listeners and the remote before reading the next slot.
pub fn broadcast_menu_changes(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let size = context.menu()?.slots.len();
    for index in 0..size {
        let current = context.item_at(index)?;
        let menu = context.menu_mut()?;
        slot_listeners(menu, index, &current);
        if !menu.suppressed && !menu.remote_slots[index].matches(&current)? {
            menu.remote_slots[index].force(&current);
            if menu.synchronized {
                let state_id = menu.increment_state_id();
                let update = ContainerUpdate::Slot {
                    container_id: menu.container_id,
                    state_id,
                    slot: index,
                    stack: current,
                };
                context.send_update(update)?;
            }
        }
    }
    let menu = context.menu_mut()?;
    if !menu.suppressed && !menu.remote_carried.matches(&menu.carried)? {
        menu.remote_carried.force(&menu.carried);
        if menu.synchronized {
            let update = ContainerUpdate::Carried(menu.carried.clone());
            context.send_update(update)?;
        }
    }
    let size = context.menu()?.data.len();
    for index in 0..size {
        let current = context.data_at(index)?;
        let menu = context.menu_mut()?;
        data_listeners(menu, index, current);
        if !menu.suppressed && menu.remote_data[index] != current {
            menu.remote_data[index] = current;
            if menu.synchronized {
                let update = ContainerUpdate::Data {
                    container_id: menu.container_id,
                    id: index,
                    value: current,
                };
                context.send_update(update)?;
            }
        }
    }
    Ok(())
}
pub fn broadcast_menu_full_state(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let size = context.menu()?.slots.len();
    for index in 0..size {
        let current = context.item_at(index)?;
        slot_listeners(context.menu_mut()?, index, &current);
    }
    let size = context.menu()?.data.len();
    for index in 0..size {
        let current = context.data_at(index)?;
        data_listeners(context.menu_mut()?, index, current);
    }
    send_all_menu_data(context)
}
/// Port of AbstractContainerMenu#setSynchronizer, using the same live slot
/// reads as subsequent broadcasts.
pub fn set_menu_synchronizer(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let menu = context.menu_mut()?;
    menu.synchronized = true;
    menu.remote_carried = RemoteSlot::Unknown;
    menu.remote_slots.fill(RemoteSlot::Unknown);
    send_all_menu_data(context)
}
pub fn send_all_menu_data(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let size = context.menu()?.slots.len();
    let mut items = Vec::with_capacity(size);
    for index in 0..size {
        let current = context.item_at(index)?;
        context.menu_mut()?.remote_slots[index].force(&current);
        items.push(current);
    }
    let menu = context.menu_mut()?;
    let carried = menu.carried.clone();
    menu.remote_carried.force(&carried);
    let size = context.menu()?.data.len();
    for index in 0..size {
        let current = context.data_at(index)?;
        context.menu_mut()?.remote_data[index] = current;
    }
    let menu = context.menu_mut()?;
    if menu.synchronized {
        let state_id = menu.increment_state_id();
        let update = ContainerUpdate::Initial {
            container_id: menu.container_id,
            state_id,
            items,
            carried,
            data: menu.remote_data.clone(),
        };
        context.send_update(update)?;
    }
    Ok(())
}
fn slot_listeners(menu: &mut AbstractContainerMenu, index: usize, current: &ItemStack) {
    if !matches(&menu.last_slots[index], current) {
        menu.last_slots[index] = current.clone();
        menu.changes
            .push(ContainerChange::Slot(index, current.clone()));
    }
}
fn data_listeners(menu: &mut AbstractContainerMenu, index: usize, current: i32) {
    if menu.last_data[index] != current {
        menu.last_data[index] = current;
        menu.changes.push(ContainerChange::Data(index, current));
    }
}
