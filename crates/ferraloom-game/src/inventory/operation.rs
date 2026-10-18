//! Shared synchronization over retained live receivers or standalone adapters.
use super::*;

pub trait MenuSynchronizationContext {
    type Receiver;
    fn menu(&self) -> Result<&AbstractContainerMenu>;
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu>;
    fn capture_slot(&mut self, slot: usize) -> Result<Self::Receiver>;
    fn capture_carried(&mut self) -> Result<Self::Receiver>;
    fn current_stack<'a>(&'a self, receiver: &'a Self::Receiver) -> Result<&'a ItemStack>;
    fn release_current(&mut self, _receiver: Self::Receiver) {}
    fn listener_copies(&self) -> Result<&ListenerCopyArena>;
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena>;
    fn data_at(&mut self, index: usize) -> Result<i32>;
    /// Constructor-installed class listeners precede ServerPlayer's listener.
    fn internal_slot_changed(&mut self, _slot: usize) -> Result<()> {
        Ok(())
    }
    fn slot_changed(&mut self, slot: usize, copy: ListenerCopyRef) -> Result<()> {
        record_slot_change(self, slot, copy)
    }
    fn data_changed(&mut self, index: usize, value: i32) -> Result<()> {
        self.menu_mut()?
            .changes
            .push(ContainerChange::Data(index, value));
        Ok(())
    }
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        self.menu_mut()?.updates.push(update);
        Ok(())
    }
}
pub(crate) struct PureMenuSynchronization<'a, 'b, 'c> {
    pub menu: &'a mut AbstractContainerMenu,
    pub context: &'b MenuContext<'c>,
    pub copies: Option<&'a mut ListenerCopyArena>,
}
impl MenuSynchronizationContext for PureMenuSynchronization<'_, '_, '_> {
    type Receiver = ItemStack;
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        Ok(self.menu)
    }
    fn capture_slot(&mut self, slot: usize) -> Result<ItemStack> {
        Ok(self.menu.get_item(self.context, slot))
    }
    fn capture_carried(&mut self) -> Result<ItemStack> {
        Ok(self.menu.carried.clone())
    }
    fn current_stack<'a>(&'a self, receiver: &'a ItemStack) -> Result<&'a ItemStack> {
        Ok(receiver)
    }
    fn listener_copies(&self) -> Result<&ListenerCopyArena> {
        self.copies
            .as_deref()
            .or(self.menu.standalone_listener_copies.as_ref())
            .ok_or(MenuError::Unsupported(
                "live listener arena requires a menu context",
            ))
    }
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena> {
        match self.copies.as_deref_mut() {
            Some(arena) => Ok(arena),
            None => self
                .menu
                .standalone_listener_copies
                .as_mut()
                .ok_or(MenuError::Unsupported(
                    "live listener arena requires a menu context",
                )),
        }
    }
    fn data_at(&mut self, index: usize) -> Result<i32> {
        Ok(self.menu.data[index])
    }
}

impl MenuSynchronizationContext for LiveMenuContext<'_> {
    type Receiver = StackRef;
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
    fn capture_slot(&mut self, index: usize) -> Result<StackRef> {
        let address = self.slot_address(index)?;
        self.level.pin_stack(address)
    }
    fn capture_carried(&mut self) -> Result<StackRef> {
        self.level.pin_stack(ContainerSlot {
            container: ContainerHandle::MenuCarried {
                player: self.player,
                menu: self.menu,
            },
            slot: 0,
        })
    }
    fn current_stack<'a>(&'a self, receiver: &'a StackRef) -> Result<&'a ItemStack> {
        self.level.stack(*receiver)
    }
    fn release_current(&mut self, receiver: StackRef) {
        self.level.release_stack(receiver);
    }
    fn listener_copies(&self) -> Result<&ListenerCopyArena> {
        Ok(&self
            .level
            .entity::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .listener_copies)
    }
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena> {
        Ok(&mut self
            .level
            .entity_mut::<crate::entity::player::ServerPlayer>(self.player)?
            .menus
            .listener_copies)
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
    fn internal_slot_changed(&mut self, _slot: usize) -> Result<()> {
        if self.menu()?.crafter_listener {
            refresh_crafter_result(self.player, self.menu, self.level)?;
        }
        Ok(())
    }
    fn slot_changed(&mut self, index: usize, copy: ListenerCopyRef) -> Result<()> {
        let notify = {
            let menu = self.menu()?;
            let slot = &menu.slots[index];
            menu.player_listener
                && slot.kind != SlotKind::Result(ResultKind::Crafting)
                && slot.container == ContainerId::Player
        };
        if notify {
            // ServerPlayer's listener receives the retained lastSlots copy,
            // not a fresh lookup of the slot that nested rewards may replace.
            crate::advancements::criteria::inventory_changed(
                self.level,
                self.player,
                StackRef {
                    container: ContainerHandle::MenuListenerCopies {
                        player: self.player,
                    },
                    slot: copy.slot,
                    generation: copy.generation,
                },
            )?;
        }
        record_slot_change(self, index, copy)
    }
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        flush_container_packets(self.player, self.level)?;
        for packet in update.into_packets() {
            self.level
                .emit(crate::level::effects::LevelEffect::PlayerPacket {
                    recipients: vec![self.player],
                    packet: Box::new(packet),
                });
        }
        Ok(())
    }
}

fn record_slot_change<C: MenuSynchronizationContext + ?Sized>(
    context: &mut C,
    index: usize,
    copy: ListenerCopyRef,
) -> Result<()> {
    let stack = context
        .listener_copies()?
        .get(copy)
        .ok_or(MenuError::MissingListenerCopy(copy))?
        .clone();
    context
        .menu_mut()?
        .changes
        .push(ContainerChange::Slot(index, stack));
    Ok(())
}

/// Port of ItemStack#copy, including its shared EMPTY result.
fn copy_stack(stack: &ItemStack) -> ItemStack {
    if stack.is_empty() {
        ItemStack::default()
    } else {
        stack.clone()
    }
}
fn trigger_slot_listeners<C: MenuSynchronizationContext>(
    context: &mut C,
    index: usize,
    current: &C::Receiver,
) -> Result<Option<ListenerCopyRef>> {
    let previous = context.menu()?.last_slots[index];
    let equal = match previous {
        Some(previous) => matches(
            context
                .listener_copies()?
                .get(previous)
                .ok_or(MenuError::MissingListenerCopy(previous))?,
            context.current_stack(current)?,
        ),
        None => context.current_stack(current)?.is_empty(),
    };
    if equal {
        return Ok(None);
    }
    let item = copy_stack(context.current_stack(current)?);
    let copy = context.listener_copies_mut()?.insert(item);
    context.menu_mut()?.last_slots[index] = Some(copy);
    if let Some(previous) = previous {
        context.listener_copies_mut()?.release(previous);
    }
    assert!(context.listener_copies_mut()?.retain(copy));
    if let Err(error) = context
        .internal_slot_changed(index)
        .and_then(|()| context.slot_changed(index, copy))
    {
        context.listener_copies_mut()?.release(copy);
        return Err(error);
    }
    Ok(Some(copy))
}

/// Port of CrafterMenu#refreshRecipeResult. Replacement retains rooted old results.
pub(crate) fn refresh_crafter_result(
    player: crate::entity::EntityRef,
    menu: MenuRef,
    level: &mut crate::level::Level,
) -> Result<()> {
    let result = level.crafter_menu_result(player, menu)?;
    level.replace_stack_object(
        ContainerSlot {
            container: ContainerHandle::MenuLocal {
                player,
                menu,
                container: 1,
            },
            slot: 0,
        },
        StackValue::New(result),
    )
}
fn synchronize_slot<C: MenuSynchronizationContext>(
    context: &mut C,
    index: usize,
    current: &C::Receiver,
    copy: Option<ListenerCopyRef>,
) -> Result<()> {
    if context.menu()?.suppressed {
        return Ok(());
    }
    let expected = &context.menu()?.remote_slots[index];
    let equal = expected.matches_readonly(context.current_stack(current)?)?;
    let refresh = equal && matches!(expected, RemoteSlot::Hash(_));
    if !equal || refresh {
        let expected = copy_stack(context.current_stack(current)?);
        context.menu_mut()?.remote_slots[index].force_owned(expected);
    }
    if !equal && context.menu()?.synchronized {
        let item = match copy {
            Some(copy) => context
                .listener_copies()?
                .get(copy)
                .ok_or(MenuError::MissingListenerCopy(copy))?
                .clone(),
            None => copy_stack(context.current_stack(current)?),
        };
        let menu = context.menu_mut()?;
        let state_id = menu.increment_state_id();
        let update = ContainerUpdate::Slot {
            container_id: menu.container_id,
            state_id,
            slot: index,
            stack: item,
        };
        context.send_update(update)?;
    }
    Ok(())
}
/// Port of broadcastChanges. The captured current receiver and memoized copy
/// stay rooted through listeners, nested broadcasts and remote synchronization.
pub fn broadcast_menu_changes(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let mut index = 0;
    while index < context.menu()?.slots.len() {
        let current = context.capture_slot(index)?;
        let result: Result<()> = (|| {
            let copy = trigger_slot_listeners(context, index, &current)?;
            let result = synchronize_slot(context, index, &current, copy);
            if let Some(copy) = copy {
                context.listener_copies_mut()?.release(copy);
            }
            result
        })();
        context.release_current(current);
        result?;
        index += 1;
    }
    let carried = context.capture_carried()?;
    let result: Result<()> = (|| {
        if context.menu()?.suppressed {
            return Ok(());
        }
        let expected = &context.menu()?.remote_carried;
        let equal = expected.matches_readonly(context.current_stack(&carried)?)?;
        let refresh = equal && matches!(expected, RemoteSlot::Hash(_));
        if !equal || refresh {
            let expected = copy_stack(context.current_stack(&carried)?);
            context.menu_mut()?.remote_carried.force_owned(expected);
        }
        if !equal && context.menu()?.synchronized {
            let item = copy_stack(context.current_stack(&carried)?);
            context.send_update(ContainerUpdate::Carried(item))?;
        }
        Ok(())
    })();
    context.release_current(carried);
    result?;
    let mut index = 0;
    while index < context.menu()?.data.len() {
        let current = context.data_at(index)?;
        data_listeners(context, index, current)?;
        let menu = context.menu_mut()?;
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
        index += 1;
    }
    Ok(())
}
pub fn broadcast_menu_full_state(context: &mut impl MenuSynchronizationContext) -> Result<()> {
    let mut index = 0;
    while index < context.menu()?.slots.len() {
        let current = context.capture_slot(index)?;
        let result = trigger_slot_listeners(context, index, &current);
        context.release_current(current);
        if let Some(copy) = result? {
            context.listener_copies_mut()?.release(copy);
        }
        index += 1;
    }
    let mut index = 0;
    while index < context.menu()?.data.len() {
        let current = context.data_at(index)?;
        data_listeners(context, index, current)?;
        index += 1;
    }
    send_all_menu_data(context)
}
/// Port of AbstractContainerMenu#setSynchronizer.
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
        let current = context.capture_slot(index)?;
        let result: Result<()> = (|| {
            let item = copy_stack(context.current_stack(&current)?);
            context.menu_mut()?.remote_slots[index].force(&item);
            items.push(item);
            Ok(())
        })();
        context.release_current(current);
        result?;
    }
    let current = context.capture_carried()?;
    let carried = context.current_stack(&current).map(copy_stack);
    context.release_current(current);
    let carried = carried?;
    context.menu_mut()?.remote_carried.force(&carried);
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
fn data_listeners(
    context: &mut impl MenuSynchronizationContext,
    index: usize,
    current: i32,
) -> Result<()> {
    if context.menu()?.last_data[index] != current {
        context.menu_mut()?.last_data[index] = current;
        context.data_changed(index, current)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
