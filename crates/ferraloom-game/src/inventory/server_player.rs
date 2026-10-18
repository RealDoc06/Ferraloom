//! Container lifecycle calls on the canonical player in a concrete Level.
use super::*;
use crate::{
    entity::{
        EntityRef,
        player::{GameType, ServerPlayer, drop_item, inventory::InventoryEffect},
    },
    level::Level,
};
use ferraloom_protocol::{codec::VarInt, play::clientbound as cb};

/// Port of ServerPlayer#openMenu. The block behavior owns the call site; socket
/// delivery stays in the server adapter, in the returned packet order.
pub fn open_menu(
    player: EntityRef,
    provider: &dyn MenuProvider,
    level: &mut Level,
) -> Result<Vec<cb::Packet>> {
    let roots = provider.block_entity_roots();
    for (index, &handle) in roots.iter().enumerate() {
        if let Err(error) = level.retain_block_entity(handle) {
            for &retained in &roots[..index] {
                level.release_block_entity(retained);
            }
            return Err(error.into());
        }
    }
    let result = open_menu_inner(player, provider, level);
    for &handle in roots {
        level.release_block_entity(handle);
    }
    result
}
fn open_menu_inner(
    player: EntityRef,
    provider: &dyn MenuProvider,
    level: &mut Level,
) -> Result<Vec<cb::Packet>> {
    let mut packets = Vec::new();
    if level
        .entity::<ServerPlayer>(player)?
        .menus
        .container_menu
        .is_some()
    {
        packets.extend(close_container(player, level, true, false)?);
    }
    let id = level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .next_container_counter();
    let Some(menu) = provider.create_menu(id, player, level)? else {
        return Ok(packets);
    };
    let menu_type = menu.kind.registry_id().ok_or(MenuError::Unsupported(
        "mount screens require MountScreenOpen",
    ))?;
    let refs = menu.block_entities.clone();
    if refs.is_empty()
        && menu
            .slots
            .iter()
            .any(|slot| matches!(slot.container, ContainerId::External(_)))
    {
        return Err(MenuError::Unsupported("mount container resolver"));
    }
    for &handle in &refs {
        level.retain_block_entity(handle)?;
    }
    packets.push(cb::Packet::OpenScreen(Box::new(cb::OpenScreen {
        container_id: VarInt(menu.container_id),
        menu_type,
        title: provider.display_name(),
    })));
    let reference = level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .install_container_menu(menu);
    let initial = with_block_menu_context(player, level, |menus, _book, context| {
        let menu = menus.current_mut();
        menu.broadcast_changes(context)?;
        menu.set_synchronizer(context);
        menu.changes.clear();
        Ok(menu.drain_packets())
    });
    match initial {
        Ok(initial) => packets.extend(initial),
        Err(error) => {
            level
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .detach_menu(reference);
            for handle in refs {
                level.release_block_entity(handle);
            }
            return Err(error);
        }
    }
    Ok(packets)
}

/// Port of ServerPlayer#closeContainer/doCloseContainer and menu removed hooks.
pub fn close_container(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
) -> Result<Vec<cb::Packet>> {
    let bound = level
        .entity::<ServerPlayer>(player)?
        .menus
        .current()
        .block_entities
        .clone();
    let result = close_container_inner(player, level, send_close, disconnected, &bound);
    for handle in bound {
        level.release_block_entity(handle);
    }
    result
}
fn close_container_inner(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
    bound: &[crate::block::entity::BlockEntityRef],
) -> Result<Vec<cb::Packet>> {
    let mut packets = Vec::new();
    let effects = {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        if send_close {
            packets.push(cb::Packet::ContainerClose(cb::ContainerClose {
                container_id: VarInt(player.menus.current().container_id),
            }));
        }
        let mut context = MenuContext::player(
            &mut player.inventory,
            &mut player.parent.equipment,
            player.game_mode == GameType::Creative,
        );
        player.menus.do_close_container(&mut context, disconnected)
    };
    for effect in effects {
        match effect {
            InventoryEffect::SetSlot { slot, stack } => {
                packets.push(cb::Packet::SetPlayerInventory(cb::SetPlayerInventory {
                    slot: VarInt(slot as i32),
                    contents: stack,
                }))
            }
            InventoryEffect::Drop(stack) => {
                drop_item(player, stack, false, level)?;
            }
        }
    }
    for &handle in bound {
        crate::block::containers::menu::stop_open(handle, player, level)?;
    }
    Ok(packets)
}

/// Port of ServerPlayer#tick's opening container broadcast and validity check.
/// Canonical entity ticking calls this; the server drains pending_packets in
/// the same entity phase, preserving each player's native output order.
pub fn tick_container_menu(
    player: EntityRef,
    level: &mut Level,
) -> std::result::Result<(), crate::level::LevelError> {
    tick_container_menu_inner(player, level).map_err(|error| match error {
        MenuError::Level(error) => error,
        MenuError::Unsupported(operation) => crate::level::LevelError::Inventory(operation),
        _ => crate::level::LevelError::Inventory("container synchronization codec"),
    })
}
fn tick_container_menu_inner(player: EntityRef, level: &mut Level) -> Result<()> {
    let (valid, packets) = with_block_menu_context(player, level, |menus, _book, ctx| {
        let menu = menus.current_mut();
        menu.broadcast_changes(ctx)?;
        let valid = menu.still_valid(ctx);
        menu.changes.clear();
        Ok((valid, menu.drain_packets()))
    })?;
    level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .pending_packets
        .extend(packets);
    if !valid {
        let packets = close_container(player, level, true, false)?;
        level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .pending_packets
            .extend(packets);
    }
    Ok(())
}
