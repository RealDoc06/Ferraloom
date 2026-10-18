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

/// Packet-capture adapter over the live open path, for callers that deliver a
/// returned batch. Gameplay calls open_menu_live so nested sends remain ordered.
pub fn open_menu(
    player: EntityRef,
    provider: &dyn MenuProvider,
    level: &mut Level,
) -> Result<Vec<cb::Packet>> {
    let start = level
        .entity::<ServerPlayer>(player)?
        .menus
        .pending_packets
        .len();
    open_menu_live(player, provider, level)?;
    Ok(level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .pending_packets
        .drain(start..)
        .collect())
}

/// Port of ServerPlayer#openMenu. Sends OpenScreen before initMenu and publishes
/// the new receiver only after initialization and its callbacks return.
pub fn open_menu_live(
    player: EntityRef,
    provider: &dyn MenuProvider,
    level: &mut Level,
) -> Result<Option<MenuRef>> {
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
) -> Result<Option<MenuRef>> {
    if level
        .entity::<ServerPlayer>(player)?
        .menus
        .container_menu
        .is_some()
    {
        let packets = close_container(player, level, true, false)?;
        level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .pending_packets
            .extend(packets);
    }
    let id = level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .next_container_counter();
    let Some(menu) = provider.create_menu(id, player, level)? else {
        if level.entity::<ServerPlayer>(player)?.game_mode == GameType::Spectator {
            let mut content = ferraloom_protocol::chat::Component::translatable(
                "container.spectatorCantOpen",
                Vec::new(),
            );
            content.style.color =
                Some(ferraloom_protocol::chat::TextColor::parse("red").expect("native color"));
            level
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .pending_packets
                .push(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                    content,
                    overlay: true,
                })));
        }
        return Ok(None);
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
    for (index, &handle) in refs.iter().enumerate() {
        if let Err(error) = level.retain_block_entity(handle) {
            for &retained in &refs[..index] {
                level.release_block_entity(retained);
            }
            return Err(error.into());
        }
    }
    let packet = cb::Packet::OpenScreen(Box::new(cb::OpenScreen {
        container_id: VarInt(menu.container_id),
        menu_type,
        title: provider.display_name(),
    }));
    let reference = {
        let menus = &mut level.entity_mut::<ServerPlayer>(player)?.menus;
        menus.pending_packets.push(packet);
        menus.register_container_menu(menu)
    };
    let initial = with_menu_callback(player, reference, level, |context| {
        broadcast_menu_changes(context)?;
        set_menu_synchronizer(context)?;
        context.menu_mut()?.changes.clear();
        Ok(())
    });
    if let Err(error) = initial {
        level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .release_menu(reference);
        for handle in refs {
            level.release_block_entity(handle);
        }
        return Err(error);
    }
    let displaced = {
        let menus = &mut level.entity_mut::<ServerPlayer>(player)?.menus;
        let displaced = menus.publish_container_menu(reference)?;
        menus.release_menu(reference);
        displaced
    };
    for handle in displaced {
        level.release_block_entity(handle);
    }
    Ok(Some(reference))
}

/// Port of ServerPlayer#closeContainer/doCloseContainer and menu removed hooks.
pub fn close_container(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
) -> Result<Vec<cb::Packet>> {
    super::block_containers::ensure_unrooted_menu_values(player, level)?;
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
    let reference = level.entity::<ServerPlayer>(player)?.menus.current_ref();
    with_menu_callback(player, reference, level, |context| {
        broadcast_menu_changes(context)?;
        context.menu_mut()?.changes.clear();
        Ok(())
    })?;
    // Java reads containerMenu again after broadcasting: a synchronous slot
    // callback may have closed or replaced the menu just broadcast.
    let bound = &level
        .entity::<ServerPlayer>(player)?
        .menus
        .current()
        .block_entities;
    let valid = block_menu_still_valid(player, bound, level)?;
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
