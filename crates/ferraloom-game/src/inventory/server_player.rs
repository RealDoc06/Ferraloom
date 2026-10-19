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

/// Port of ServerPlayer#openMenu. Sends OpenScreen before initMenu and publishes
/// the new receiver only after initialization and its callbacks return.
pub fn open_menu_live(
    player: EntityRef,
    provider: &dyn MenuProvider,
    level: &mut Level,
) -> Result<Option<MenuRef>> {
    flush_container_packets(player, level)?;
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
        close_container_live(player, level, true, false)?;
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
            emit_menu_packet(
                player,
                cb::Packet::SystemChat(Box::new(cb::SystemChat {
                    content,
                    overlay: true,
                })),
                level,
            );
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
    let crafter = menu.kind == MenuKind::Crafter;
    let bound_crafter = crafter && !refs.is_empty();
    let reference = {
        let menus = &mut level.entity_mut::<ServerPlayer>(player)?.menus;
        menus.register_container_menu(menu)
    };
    let initial = (|| {
        // Crafter's constructor refresh/addSlotListener precede OpenScreen and
        // the later ServerPlayer listener. No synchronizer is installed yet.
        if crafter {
            with_menu_callback(player, reference, level, |context| {
                super::operation::refresh_crafter_result(player, reference, context.level)?;
                if bound_crafter {
                    context.menu_mut()?.crafter_listener = true;
                    broadcast_menu_changes(context)?;
                }
                Ok(())
            })?;
        }
        emit_menu_packet(player, packet, level);
        synchronize_menu(player, reference, MenuSynchronization::Initialize, level)
    })();
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
    close_container_with_delivery(player, level, send_close, disconnected, false)
}

/// Port of the live closeContainer send point, before removed and item drops.
pub fn close_container_live(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
) -> Result<()> {
    flush_container_packets(player, level)?;
    close_container_with_delivery(player, level, send_close, disconnected, true)?;
    Ok(())
}

/// Move already generated menu packets into the ordered Level packet stream.
pub fn flush_container_packets(
    player: EntityRef,
    level: &mut Level,
) -> std::result::Result<(), crate::level::LevelError> {
    let packets = std::mem::take(
        &mut level
            .entity_mut::<ServerPlayer>(player)?
            .menus
            .pending_packets,
    );
    for packet in packets {
        emit_menu_packet(player, packet, level);
    }
    Ok(())
}
fn emit_menu_packet(player: EntityRef, packet: cb::Packet, level: &mut Level) {
    level.emit(crate::level::effects::LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(packet),
    });
}
/// Deliver already captured protocol updates before entering another native
/// callback. This bridges legacy pure menu operations during their migration.
pub fn send_container_packets(
    player: EntityRef,
    packets: impl IntoIterator<Item = cb::Packet>,
    level: &mut Level,
) -> Result<()> {
    flush_container_packets(player, level)?;
    for packet in packets {
        emit_menu_packet(player, packet, level);
    }
    Ok(())
}
fn deliver_close_packet(
    player: EntityRef,
    packet: cb::Packet,
    live: bool,
    level: &mut Level,
    packets: &mut Vec<cb::Packet>,
) {
    if live {
        emit_menu_packet(player, packet, level);
    } else {
        packets.push(packet);
    }
}
fn close_container_with_delivery(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
    live: bool,
) -> Result<Vec<cb::Packet>> {
    super::block_containers::ensure_unrooted_menu_values(player, level)?;
    let bound = level
        .entity::<ServerPlayer>(player)?
        .menus
        .current()
        .block_entities
        .clone();
    let result = close_container_inner(player, level, send_close, disconnected, &bound, live);
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
    live: bool,
) -> Result<Vec<cb::Packet>> {
    let mut packets = Vec::new();
    if send_close {
        let container_id = level
            .entity::<ServerPlayer>(player)?
            .menus
            .current()
            .container_id;
        deliver_close_packet(
            player,
            cb::Packet::ContainerClose(cb::ContainerClose {
                container_id: VarInt(container_id),
            }),
            live,
            level,
            &mut packets,
        );
    }
    let effects = {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        let infinite_materials = player.instabuild();
        let mut context = MenuContext::player_with_abilities(
            &mut player.inventory,
            &mut player.parent.equipment,
            player.game_mode == GameType::Creative,
            infinite_materials,
        );
        player.menus.do_close_container(&mut context, disconnected)
    };
    for effect in effects {
        match effect {
            InventoryEffect::SetSlot { slot, stack } => {
                deliver_close_packet(
                    player,
                    cb::Packet::SetPlayerInventory(cb::SetPlayerInventory {
                        slot: VarInt(slot as i32),
                        contents: stack,
                    }),
                    live,
                    level,
                    &mut packets,
                );
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
/// Canonical entity ticking calls this; sends enter Level's ordered packet
/// stream at their synchronizer callbacks.
pub fn tick_container_menu(
    player: EntityRef,
    level: &mut Level,
) -> std::result::Result<(), crate::level::LevelError> {
    broadcast_container_menu(player, level)?;
    check_container_menu_still_valid(player, level)
}
fn menu_level_error(error: MenuError) -> crate::level::LevelError {
    match error {
        MenuError::Level(error) => error,
        MenuError::Unsupported(operation) => crate::level::LevelError::Inventory(operation),
        _ => crate::level::LevelError::Inventory("container synchronization codec"),
    }
}
/// Port of the current containerMenu.broadcastChanges call used by rewards.
/// The retained receiver finishes broadcasting even if a callback replaces it.
pub fn broadcast_container_menu(
    player: EntityRef,
    level: &mut Level,
) -> std::result::Result<(), crate::level::LevelError> {
    let reference = level.entity::<ServerPlayer>(player)?.menus.current_ref();
    synchronize_menu(player, reference, MenuSynchronization::Changes, level)
        .map_err(menu_level_error)
}

#[derive(Clone, Copy, Debug)]
pub enum MenuSynchronization {
    Changes,
    FullState,
    Contents,
    Initialize,
}
/// Run the receiver captured at one native menu call. Reentrant replacements
/// affect subsequent player.containerMenu reads, never this receiver.
pub fn synchronize_menu(
    player: EntityRef,
    menu: MenuRef,
    synchronization: MenuSynchronization,
    level: &mut Level,
) -> Result<()> {
    flush_container_packets(player, level)?;
    with_menu_callback(player, menu, level, |context| {
        match synchronization {
            MenuSynchronization::Changes => broadcast_menu_changes(context)?,
            MenuSynchronization::FullState => broadcast_menu_full_state(context)?,
            MenuSynchronization::Contents => send_all_menu_data(context)?,
            MenuSynchronization::Initialize => {
                if !context.menu()?.player_listener {
                    context.menu_mut()?.player_listener = true;
                    broadcast_menu_changes(context)?;
                }
                set_menu_synchronizer(context)?;
            }
        }
        context.menu_mut()?.changes.clear();
        Ok(())
    })
}

/// Port of ServerPlayer#initInventoryMenu, after addNewPlayer on initial join.
pub fn initialize_inventory_menu(player: EntityRef, level: &mut Level) -> Result<()> {
    let menu = level
        .entity::<ServerPlayer>(player)?
        .menus
        .inventory_menu
        .reference();
    synchronize_menu(player, menu, MenuSynchronization::Initialize, level)
}

/// Port of ServerGamePacketListenerImpl#tryPickItem. The selected-slot packet
/// precedes inventoryMenu's synchronous listener and synchronizer callbacks.
pub fn try_pick_item_live(player: EntityRef, stack: ItemStack, level: &mut Level) -> Result<()> {
    if !stack.item().is_enabled_in_vanilla() {
        return Ok(());
    }
    flush_container_packets(player, level)?;
    let (selected, menu) = {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        let infinite_materials = player.instabuild();
        if let Some(slot) = player.inventory.find_slot_matching_item(&stack) {
            if slot < 9 {
                player.inventory.set_selected_slot(slot);
            } else {
                player.inventory.pick_slot(slot);
            }
        } else if infinite_materials {
            player.inventory.add_and_pick_item(stack);
        }
        (
            player.inventory.selected_slot(),
            player.menus.inventory_menu.reference(),
        )
    };
    emit_menu_packet(
        player,
        cb::Packet::SetHeldSlot(cb::SetHeldSlot {
            slot: VarInt(selected as i32),
        }),
        level,
    );
    synchronize_menu(player, menu, MenuSynchronization::Changes, level)
}

/// Port of ServerPlayer#doTick's current menu validity check, with no broadcast.
pub fn check_container_menu_still_valid(
    player: EntityRef,
    level: &mut Level,
) -> std::result::Result<(), crate::level::LevelError> {
    check_container_menu_still_valid_inner(player, level).map_err(menu_level_error)
}
fn check_container_menu_still_valid_inner(player: EntityRef, level: &mut Level) -> Result<()> {
    // Java reads containerMenu again after broadcasting: a synchronous slot
    // callback may have closed or replaced the menu just broadcast.
    let menu = level.entity::<ServerPlayer>(player)?.menus.current_ref();
    let valid = live_menu_still_valid(player, menu, level)?;
    if !valid {
        close_container_live(player, level, true, false)?;
    }
    Ok(())
}

/// Port of Player#addItem. The supplied receiver is caller-owned generated loot;
/// native Inventory#add mutates its remainder before any subsequent world call.
pub fn add_item(
    player: EntityRef,
    stack: &mut ItemStack,
    level: &mut Level,
) -> std::result::Result<bool, crate::level::LevelError> {
    let player = level.entity_mut::<ServerPlayer>(player)?;
    let infinite_materials = player.instabuild();
    Ok(player
        .inventory
        .add(stack, &mut player.parent.equipment, infinite_materials))
}
