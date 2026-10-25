//! Short-borrow addresses for the live menu algorithm. Canonical receivers never
//! leave their owner while a slot operation calls back into the level.
use super::{Container, ItemStack, MenuError, MenuRef, PlayerContainer, Result};
use crate::{
    block::{
        containers::{access, storage::BaseContainerBlockEntity},
        entity::BlockEntityRef,
    },
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ContainerHandle {
    PlayerInventory(EntityRef),
    BlockEntity(BlockEntityRef),
    MenuLocal {
        player: EntityRef,
        menu: MenuRef,
        container: usize,
    },
    MenuCarried {
        player: EntityRef,
        menu: MenuRef,
    },
    MenuListenerCopies {
        player: EntityRef,
    },
    MenuOperationStacks {
        player: EntityRef,
    },
    Entity(EntityRef),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerSlot {
    pub container: ContainerHandle,
    pub slot: usize,
}

/// Fallible slot access shared by the live and in-memory menu contexts. Every
/// operation completes its native container callbacks before returning.
pub trait MenuContainerAccess {
    fn get_item(&mut self, slot: ContainerSlot) -> Result<ItemStack>;
    fn set_item(&mut self, slot: ContainerSlot, stack: ItemStack) -> Result<()>;
    fn set_item_count(&mut self, slot: ContainerSlot, amount: i32) -> Result<()>;
    fn remove_item(&mut self, slot: ContainerSlot, amount: i32) -> Result<ItemStack>;
    fn remove_item_no_update(&mut self, slot: ContainerSlot) -> Result<ItemStack>;
    fn set_changed(&mut self, container: ContainerHandle) -> Result<()>;
    fn size(&mut self, container: ContainerHandle) -> Result<usize>;
    fn max_stack_size(&mut self, container: ContainerHandle) -> Result<i32>;
    fn can_place_item(&mut self, slot: ContainerSlot, stack: &ItemStack) -> Result<bool>;
    fn still_valid(&mut self, container: ContainerHandle) -> Result<bool>;
}

pub struct LiveMenuContext<'a> {
    pub level: &'a mut Level,
    pub player: EntityRef,
    pub menu: MenuRef,
}

/// Port of the bound menu's stillValid query, including retained-generation
/// failure. Entity-container validity requires the entity owner's real view.
pub fn live_menu_still_valid(
    player: EntityRef,
    menu: super::MenuRef,
    level: &Level,
) -> Result<bool> {
    let menu = level.entity::<ServerPlayer>(player)?.menus.menu(menu)?;
    if !menu.workstation_still_valid(player, level)? {
        return Ok(false);
    }
    if menu.block_entities.is_empty()
        && menu
            .slots
            .iter()
            .any(|slot| matches!(slot.container, super::ContainerId::External(_)))
    {
        return Err(MenuError::Unsupported("entity container validity"));
    }
    super::block_menu_still_valid(player, &menu.block_entities, level)
}
impl LiveMenuContext<'_> {
    pub fn slot_address(&self, index: usize) -> Result<ContainerSlot> {
        let menu = self
            .level
            .entity::<ServerPlayer>(self.player)?
            .menus
            .menu(self.menu)?;
        let slot = menu
            .slots
            .get(index)
            .ok_or(MenuError::InvalidSlot(index as i32))?;
        let container = match slot.container {
            super::ContainerId::Player => ContainerHandle::PlayerInventory(self.player),
            super::ContainerId::Local(container) => ContainerHandle::MenuLocal {
                player: self.player,
                menu: self.menu,
                container,
            },
            super::ContainerId::External(0) => {
                let mut offset = slot.container_slot;
                for &handle in &menu.block_entities {
                    let size = self
                        .level
                        .block_entity::<BaseContainerBlockEntity>(handle)?
                        .size();
                    if offset < size {
                        return Ok(ContainerSlot {
                            container: ContainerHandle::BlockEntity(handle),
                            slot: offset,
                        });
                    }
                    offset -= size;
                }
                return Err(MenuError::Unsupported("unbound external menu container"));
            }
            super::ContainerId::External(_) => {
                return Err(MenuError::Unsupported("entity container resolver"));
            }
        };
        Ok(ContainerSlot {
            container,
            slot: slot.container_slot,
        })
    }
    fn with_player<R>(
        &mut self,
        player: EntityRef,
        operation: impl FnOnce(&mut PlayerContainer<'_>) -> R,
    ) -> Result<R> {
        let player = self.level.entity_mut::<ServerPlayer>(player)?;
        Ok(operation(&mut PlayerContainer {
            inventory: &mut player.inventory,
            equipment: &mut player.parent.equipment,
        }))
    }
    fn with_local<R>(
        &mut self,
        player: EntityRef,
        menu: MenuRef,
        container: usize,
        operation: impl FnOnce(&mut super::MenuContainer) -> R,
    ) -> Result<R> {
        let menu = self
            .level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .menu_mut(menu)?;
        let container = menu
            .local
            .get_mut(container)
            .ok_or(MenuError::Unsupported("missing local menu container"))?;
        Ok(operation(container))
    }
}
impl MenuContainerAccess for LiveMenuContext<'_> {
    fn set_item_count(&mut self, slot: ContainerSlot, amount: i32) -> Result<()> {
        match slot.container {
            ContainerHandle::PlayerInventory(player) => self.with_player(player, |container| {
                container.set_item_count(slot.slot, amount)
            })?,
            ContainerHandle::BlockEntity(handle) => self
                .level
                .block_entity_mut::<BaseContainerBlockEntity>(handle)?
                .set_item_count(slot.slot, amount),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.set_item_count(slot.slot, amount)
            })?,
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn get_item(&mut self, slot: ContainerSlot) -> Result<ItemStack> {
        match slot.container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.get_item(slot.slot))
            }
            ContainerHandle::BlockEntity(handle) => {
                Ok(access::get_item(handle, slot.slot, self.level)?)
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.get_item(slot.slot)
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn set_item(&mut self, slot: ContainerSlot, stack: ItemStack) -> Result<()> {
        match slot.container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.set_item(slot.slot, stack))
            }
            ContainerHandle::BlockEntity(handle) => {
                Ok(access::set_item(handle, slot.slot, stack, self.level)?)
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.set_item(slot.slot, stack)
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn remove_item(&mut self, slot: ContainerSlot, amount: i32) -> Result<ItemStack> {
        match slot.container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.remove_item(slot.slot, amount))
            }
            ContainerHandle::BlockEntity(handle) => {
                Ok(access::remove_item(handle, slot.slot, amount, self.level)?)
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.remove_item(slot.slot, amount)
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn remove_item_no_update(&mut self, slot: ContainerSlot) -> Result<ItemStack> {
        if self.level.stack_is_retained(slot)? {
            return Err(MenuError::Unsupported(
                "retained removal requires an Existing stack value",
            ));
        }
        match slot.container {
            ContainerHandle::PlayerInventory(player) => self.with_player(player, |container| {
                container.remove_item_no_update(slot.slot)
            }),
            ContainerHandle::BlockEntity(handle) => Ok(access::remove_item_no_update(
                handle, slot.slot, self.level,
            )?),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.remove_item_no_update(slot.slot)
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn set_changed(&mut self, container: ContainerHandle) -> Result<()> {
        match container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.set_changed())
            }
            ContainerHandle::BlockEntity(handle) => {
                self.level
                    .block_entity_mut::<BaseContainerBlockEntity>(handle)?
                    .set_changed();
                Ok(handle.set_changed(self.level)?)
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| container.set_changed()),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn size(&mut self, container: ContainerHandle) -> Result<usize> {
        match container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.size())
            }
            ContainerHandle::BlockEntity(handle) => Ok(self
                .level
                .block_entity::<BaseContainerBlockEntity>(handle)?
                .size()),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| container.size()),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn max_stack_size(&mut self, container: ContainerHandle) -> Result<i32> {
        match container {
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.max_stack_size())
            }
            ContainerHandle::BlockEntity(handle) => Ok(self
                .level
                .block_entity::<BaseContainerBlockEntity>(handle)?
                .max_stack_size()),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.max_stack_size()
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn can_place_item(&mut self, slot: ContainerSlot, stack: &ItemStack) -> Result<bool> {
        match slot.container {
            ContainerHandle::PlayerInventory(player) => self.with_player(player, |container| {
                container.can_place_item(slot.slot, stack)
            }),
            ContainerHandle::BlockEntity(handle) => Ok(access::can_place_item(
                self.level.block_entity_state(handle)?,
                slot.slot,
                stack,
            )),
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| {
                container.can_place_item(slot.slot, stack)
            }),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
    fn still_valid(&mut self, container: ContainerHandle) -> Result<bool> {
        match container {
            ContainerHandle::BlockEntity(handle) => Ok(self.level.block_entity_ref(handle.pos)?
                == Some(handle)
                && self
                    .level
                    .entity::<ServerPlayer>(self.player)?
                    .is_within_block_interaction_range(handle.pos, 4.0)),
            ContainerHandle::PlayerInventory(player) => {
                self.with_player(player, |container| container.still_valid())
            }
            ContainerHandle::MenuLocal {
                player,
                menu,
                container,
            } => self.with_local(player, menu, container, |container| container.still_valid()),
            ContainerHandle::MenuCarried { .. } => Err(MenuError::Unsupported(
                "cursor is a stack receiver, not a slot container",
            )),
            ContainerHandle::MenuListenerCopies { .. }
            | ContainerHandle::MenuOperationStacks { .. } => Err(MenuError::Unsupported(
                "operation objects are not inventory containers",
            )),
            ContainerHandle::Entity(_) => Err(MenuError::Unsupported("entity container resolver")),
        }
    }
}

/// Retain both the menu and its canonical block receivers for one dynamic call
/// frame. Closing/replacing either receiver during a callback cannot retarget it.
pub fn with_menu_callback<R>(
    player: EntityRef,
    menu: MenuRef,
    level: &mut Level,
    operation: impl FnOnce(&mut LiveMenuContext<'_>) -> Result<R>,
) -> Result<R> {
    level.retain_entity(player)?;
    let result = with_menu_callback_inner(player, menu, level, operation);
    level.release_entity(player);
    result
}
fn with_menu_callback_inner<R>(
    player: EntityRef,
    menu: MenuRef,
    level: &mut Level,
    operation: impl FnOnce(&mut LiveMenuContext<'_>) -> Result<R>,
) -> Result<R> {
    level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .bind_listener_copies(menu)?;
    let roots = level
        .entity::<ServerPlayer>(player)?
        .menus
        .menu(menu)?
        .block_entities
        .clone();
    for (index, &root) in roots.iter().enumerate() {
        if let Err(error) = level.retain_block_entity(root) {
            for &retained in &roots[..index] {
                level.release_block_entity(retained);
            }
            return Err(error.into());
        }
    }
    let retained = level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .retain_menu(menu);
    if let Err(error) = retained {
        for root in roots {
            level.release_block_entity(root);
        }
        return Err(error);
    }
    let result = operation(&mut LiveMenuContext {
        level,
        player,
        menu,
    });
    let cleanup = level
        .entity_mut::<ServerPlayer>(player)
        .map(|player| player.menus.release_menu(menu));
    for root in roots {
        level.release_block_entity(root);
    }
    match result {
        Err(error) => Err(error),
        Ok(result) => {
            cleanup?;
            Ok(result)
        }
    }
}
