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
    let Some(mut menu) = provider.create_menu(id, player, level)? else {
        return Ok(packets);
    };
    let menu_type = menu.kind.registry_id().ok_or(MenuError::Unsupported(
        "mount screens require MountScreenOpen",
    ))?;
    if menu
        .slots
        .iter()
        .any(|slot| matches!(slot.container, ContainerId::External(_)))
    {
        return Err(MenuError::Unsupported(
            "canonical block/mount container resolver",
        ));
    }
    packets.push(cb::Packet::OpenScreen(Box::new(cb::OpenScreen {
        container_id: VarInt(menu.container_id),
        menu_type,
        title: provider.display_name(),
    })));
    let player = level.entity_mut::<ServerPlayer>(player)?;
    let context = MenuContext::player(
        &mut player.inventory,
        &mut player.parent.equipment,
        player.game_mode == GameType::Creative,
    );
    menu.broadcast_changes(&context)?;
    menu.set_synchronizer(&context);
    menu.changes.clear();
    packets.extend(menu.drain_packets());
    player.menus.container_menu = Some(menu);
    Ok(packets)
}

/// Port of ServerPlayer#closeContainer/doCloseContainer and menu removed hooks.
pub fn close_container(
    player: EntityRef,
    level: &mut Level,
    send_close: bool,
    disconnected: bool,
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
    Ok(packets)
}
