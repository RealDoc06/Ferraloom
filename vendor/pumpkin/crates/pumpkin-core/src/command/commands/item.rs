use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::hover::HoverEvent;

use crate::command::argument_builder::{
    ArgumentBuilder, RequiredArgumentBuilder, argument, command, literal,
};
use crate::command::argument_types::coordinates::block_pos::BlockPosArgumentType;
use crate::command::argument_types::core::integer::IntegerArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::argument_types::item::ItemStackArgumentType;
use crate::command::argument_types::slot::SlotArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

const DESCRIPTION: &str = "Modifies items in block or entity inventories.";
const PERMISSION: &str = "minecraft:command.item";

const ERROR_NOT_CONTAINER: CommandErrorType<3> = CommandErrorType::new(
    translation::java::COMMANDS_ITEM_TARGET_NOT_A_CONTAINER,
    translation::java::COMMANDS_ITEM_TARGET_NOT_A_CONTAINER,
);

const ERROR_NO_SUCH_SLOT: CommandErrorType<1> = CommandErrorType::new(
    translation::java::COMMANDS_ITEM_TARGET_NO_SUCH_SLOT,
    translation::java::COMMANDS_ITEM_TARGET_NO_SUCH_SLOT,
);

struct BlockReplaceExecutor {
    has_count: bool,
}

impl CommandExecutor for BlockReplaceExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context.source.world();
        let pos = BlockPosArgumentType::get_loaded_block_pos(context, "pos")?;
        let slot = SlotArgumentType::get(context, "slot")?;
        let parsed_stack = ItemStackArgumentType::get(context, "item")?;
        let item = parsed_stack.item;
        let count = if self.has_count {
            IntegerArgumentType::get(context, "count")?
        } else {
            1
        };

        let block_entity = world.get_block_entity(&pos).ok_or_else(|| {
            ERROR_NOT_CONTAINER.create_without_context(
                TextComponent::text(pos.0.x.to_string()),
                TextComponent::text(pos.0.y.to_string()),
                TextComponent::text(pos.0.z.to_string()),
            )
        })?;

        let inventory = block_entity.get_inventory().ok_or_else(|| {
            ERROR_NOT_CONTAINER.create_without_context(
                TextComponent::text(pos.0.x.to_string()),
                TextComponent::text(pos.0.y.to_string()),
                TextComponent::text(pos.0.z.to_string()),
            )
        })?;

        if slot >= inventory.size() {
            return Err(
                ERROR_NO_SUCH_SLOT.create_without_context(TextComponent::text(slot.to_string()))
            );
        }

        let mut item_stack = parsed_stack.clone();
        item_stack.item_count = count as u8;
        inventory.set_stack(slot, item_stack);

        let item_name = item.registry_key;
        let msg = TextComponent::translate_cross(
            translation::java::COMMANDS_ITEM_BLOCK_SET_SUCCESS,
            translation::java::COMMANDS_ITEM_BLOCK_SET_SUCCESS,
            [
                TextComponent::text(pos.0.x.to_string()),
                TextComponent::text(pos.0.y.to_string()),
                TextComponent::text(pos.0.z.to_string()),
                TextComponent::text("[")
                    .add_child(item.translated_name())
                    .add_child(TextComponent::text("]"))
                    .hover_event(HoverEvent::ShowItem {
                        id: item_name.to_string().into(),
                        count: Some(count),
                    }),
            ],
        );
        context.source.send_feedback(msg, true);

        Ok(1)
    }
}

struct EntityReplaceExecutor {
    has_count: bool,
}

impl CommandExecutor for EntityReplaceExecutor {
    #[expect(clippy::too_many_lines)]
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let mojang_slot = SlotArgumentType::get(context, "slot")?;
        let parsed_stack = ItemStackArgumentType::get(context, "item")?;
        let item = parsed_stack.item;
        let count = if self.has_count {
            IntegerArgumentType::get(context, "count")?
        } else {
            1
        };

        let mut modified_count = 0;
        let mut item_stack = parsed_stack.clone();
        item_stack.item_count = count as u8;

        for target in &targets {
            if let Some(player) = target.get_player() {
                if let Some(player_arc) = player.world().get_player_by_uuid(player.gameprofile.id) {
                    if (200..=226).contains(&mojang_slot) {
                        let ender_slot = mojang_slot - 200;
                        if ender_slot < player.ender_chest_inventory.size() {
                            player_arc
                                .ender_chest_inventory
                                .set_stack(ender_slot, item_stack.clone());
                            modified_count += 1;
                        }
                    } else {
                        let inventory = player.inventory();
                        let mapped_slot = if mojang_slot == 98 {
                            Some(inventory.get_selected_slot() as usize)
                        } else if mojang_slot == 99 {
                            Some(40)
                        } else if mojang_slot == 100 {
                            Some(36)
                        } else if mojang_slot == 101 {
                            Some(37)
                        } else if mojang_slot == 102 {
                            Some(38)
                        } else if mojang_slot == 103 {
                            Some(39)
                        } else if mojang_slot <= 35 {
                            Some(mojang_slot)
                        } else {
                            None
                        };

                        if let Some(slot) = mapped_slot
                            && slot < inventory.size()
                        {
                            player_arc.inventory().set_stack(slot, item_stack.clone());

                            // Screen handlers own menu slot indices and revision IDs,
                            // including the player slots in an open container.
                            player_arc.sync_inventory_to_client();

                            let eq_slot = if slot == 36 {
                                Some(EquipmentSlot::FEET)
                            } else if slot == 37 {
                                Some(EquipmentSlot::LEGS)
                            } else if slot == 38 {
                                Some(EquipmentSlot::CHEST)
                            } else if slot == 39 {
                                Some(EquipmentSlot::HEAD)
                            } else if slot == 40 {
                                Some(EquipmentSlot::OFF_HAND)
                            } else if slot == inventory.get_selected_slot() as usize {
                                Some(EquipmentSlot::MAIN_HAND)
                            } else {
                                None
                            };

                            if let Some(eq) = eq_slot {
                                player
                                    .living_entity
                                    .send_equipment_changes(&[(eq, item_stack.clone())]);
                            }

                            modified_count += 1;
                        }
                    }
                }
            } else if let Some(living) = target.get_living_entity() {
                let mapped_eq = if mojang_slot == 98 {
                    Some(EquipmentSlot::MAIN_HAND)
                } else if mojang_slot == 99 {
                    Some(EquipmentSlot::OFF_HAND)
                } else if mojang_slot == 100 {
                    Some(EquipmentSlot::FEET)
                } else if mojang_slot == 101 {
                    Some(EquipmentSlot::LEGS)
                } else if mojang_slot == 102 {
                    Some(EquipmentSlot::CHEST)
                } else if mojang_slot == 103 {
                    Some(EquipmentSlot::HEAD)
                } else if mojang_slot == 105 {
                    Some(EquipmentSlot::BODY)
                } else if mojang_slot == 106 {
                    Some(EquipmentSlot::SADDLE)
                } else {
                    None
                };

                if let Some(eq) = mapped_eq {
                    living
                        .entity_equipment
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .put(&eq, item_stack.clone());
                    living.send_equipment_changes(&[(eq, item_stack.clone())]);
                    modified_count += 1;
                }
            }
        }

        if modified_count == 0 {
            return Err(ERROR_NO_SUCH_SLOT
                .create_without_context(TextComponent::text(mojang_slot.to_string())));
        }

        let item_name = item.registry_key;
        let msg = if targets.len() == 1 {
            TextComponent::translate_cross(
                translation::java::COMMANDS_ITEM_ENTITY_SET_SUCCESS_SINGLE,
                translation::java::COMMANDS_ITEM_ENTITY_SET_SUCCESS_SINGLE,
                [
                    targets[0].as_ref().get_display_name(),
                    TextComponent::text("[")
                        .add_child(item.translated_name())
                        .add_child(TextComponent::text("]"))
                        .hover_event(HoverEvent::ShowItem {
                            id: item_name.to_string().into(),
                            count: Some(count),
                        }),
                ],
            )
        } else {
            TextComponent::translate_cross(
                translation::java::COMMANDS_ITEM_ENTITY_SET_SUCCESS_MULTIPLE,
                translation::java::COMMANDS_ITEM_ENTITY_SET_SUCCESS_MULTIPLE,
                [
                    TextComponent::text(modified_count.to_string()),
                    TextComponent::text("[")
                        .add_child(item.translated_name())
                        .add_child(TextComponent::text("]"))
                        .hover_event(HoverEvent::ShowItem {
                            id: item_name.to_string().into(),
                            count: Some(count),
                        }),
                ],
            )
        };
        context.source.send_feedback(msg, true);

        Ok(modified_count)
    }
}

fn block_item_node() -> RequiredArgumentBuilder {
    argument("item", ItemStackArgumentType)
        .executes(BlockReplaceExecutor { has_count: false })
        .then(
            argument("count", IntegerArgumentType::new(1, 99))
                .executes(BlockReplaceExecutor { has_count: true }),
        )
}

fn entity_item_node() -> RequiredArgumentBuilder {
    argument("item", ItemStackArgumentType)
        .executes(EntityReplaceExecutor { has_count: false })
        .then(
            argument("count", IntegerArgumentType::new(1, 99))
                .executes(EntityReplaceExecutor { has_count: true }),
        )
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let block_node = literal("block")
        .then(argument("pos", BlockPosArgumentType).then(
            argument("slot", SlotArgumentType).then(literal("with").then(block_item_node())),
        ));

    let entity_node =
        literal("entity").then(argument("targets", EntityArgumentType::Entities).then(
            argument("slot", SlotArgumentType).then(literal("with").then(entity_item_node())),
        ));

    dispatcher.register(
        command("item", DESCRIPTION)
            .requires(PERMISSION)
            .then(literal("replace").then(block_node).then(entity_node)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, atomic::Ordering};
    use std::time::Duration;

    use arc_swap::ArcSwap;
    use pumpkin_data::{
        item::Item,
        item_stack::ItemStack,
        packet::{CURRENT_MC_VERSION, clientbound::play::CONTAINER_SET_CONTENT},
    };
    use pumpkin_inventory::{
        SimpleInventory,
        generic_container_screen_handler::create_generic_9x3,
        screen_handler::{ScreenHandler, SharedScreenHandler},
        sync_handler::SyncHandler,
    };
    use pumpkin_protocol::{
        RawPacket, ServerPacket,
        java::{
            client::play::{CSetContainerContent, CSetContainerSlot, CSetSelectedSlot},
            packet_decoder::TCPNetworkDecoder,
        },
        packet::MultiVersionJavaPacket,
    };
    use pumpkin_util::GameMode;
    use tokio::{io::BufReader, net::TcpStream};

    use crate::{
        command::CommandSource,
        entity::player::Player,
        net::{
            ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
            java::{JavaClient, pending::PendingConnection},
        },
    };

    struct Fixture {
        world: crate::command::commands::data::tests::Fixture,
        player: Arc<Player>,
        sync: Arc<SyncHandler>,
        reader: TCPNetworkDecoder<BufReader<TcpStream>>,
    }

    impl Fixture {
        async fn new() -> Self {
            let mut world = crate::command::commands::data::tests::Fixture::new().await;
            register(&mut world.dispatcher, &PermissionRegistry::default());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (peer, accepted) = tokio::join!(
                TcpStream::connect(listener.local_addr().unwrap()),
                listener.accept(),
            );
            let (socket, address) = accepted.unwrap();
            let profile = GameProfile {
                id: uuid::Uuid::new_v4(),
                name: "ItemTarget".into(),
                properties: ArcSwap::from_pointee(Vec::new()),
                profile_actions: None,
            };
            let pending = PendingConnection::new(
                socket,
                address,
                0,
                PacketRateLimiter::new(false, 0.0, 0.0),
                world.source.world().server.clone(),
            );
            let mut client =
                JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
            client.start_outgoing_packet_task();
            let player = Arc::new(Player::new(
                Arc::new(ClientPlatform::Java(client)),
                profile,
                PlayerConfig::default(),
                world.source.world(),
                GameMode::Survival,
            ));
            world
                .source
                .world()
                .players
                .store(Arc::new(vec![player.clone()]));

            // Use the production handler and network sink, with an independently
            // owned sync handler so teardown can break the player reference cycle.
            let sync = Arc::new(SyncHandler::new());
            sync.store_player(player.clone());
            {
                let mut handler = player.player_screen_handler.lock().unwrap();
                handler.add_listener(player.screen_handler_listener.clone());
                handler.update_sync_handler(sync.clone());
            }
            Self {
                world,
                player,
                sync,
                reader: TCPNetworkDecoder::new(BufReader::new(peer.unwrap())),
            }
        }

        fn execute(&self, command: &str) -> CommandExecutorResult {
            let source = CommandSource {
                server: self.world.source.server.clone(),
                ..CommandSource::dummy()
                    .with_world(self.world.source.world().clone())
                    .with_entity(self.player.clone())
                    .with_silent()
            };
            self.world.dispatcher.execute_input(command, &source)
        }

        /// Read the real serialized FIFO through a marker; no sleeps or packet-count guesses.
        async fn packets(&mut self) -> Vec<RawPacket> {
            let client = self.player.client.java().unwrap();
            client.try_send_packet(&CSetSelectedSlot::new(0));
            client.flush_channel();
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut packets = Vec::new();
                loop {
                    let packet = self.reader.get_raw_packet().await.unwrap();
                    if packet.id == CSetSelectedSlot::to_id(CURRENT_MC_VERSION) {
                        return packets;
                    }
                    packets.push(packet);
                }
            })
            .await
            .expect("item command packets did not reach the FIFO marker")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.player.client.java().unwrap().close();
            self.world
                .source
                .world()
                .players
                .store(Arc::new(Vec::new()));
            self.player
                .player_screen_handler
                .lock()
                .unwrap()
                .get_behaviour_mut()
                .sync_handler = None;
            self.player
                .current_screen_handler
                .lock()
                .unwrap()
                .lock()
                .unwrap()
                .get_behaviour_mut()
                .sync_handler = None;
        }
    }

    fn initial_state(packets: &[RawPacket], menu: i32) -> i32 {
        let states: Vec<_> = packets
            .iter()
            .filter(|packet| packet.id == CONTAINER_SET_CONTENT.to_id(CURRENT_MC_VERSION))
            .filter_map(|packet| {
                let content =
                    CSetContainerContent::read(&mut packet.payload.as_ref(), &CURRENT_MC_VERSION)
                        .unwrap();
                (content.window_id.0 == menu).then_some(content.state_id.0)
            })
            .collect();
        assert_eq!(states.len(), 1);
        assert!(states[0] > 0);
        states[0]
    }

    fn assert_slot_updates(packets: &[RawPacket], expected: &[(i8, i32, i16)], stack: &ItemStack) {
        let mut updates = Vec::new();
        for packet in packets {
            assert_ne!(packet.id, CSetContainerContent::to_id(CURRENT_MC_VERSION));
            if packet.id == CSetContainerSlot::to_id(CURRENT_MC_VERSION) {
                let mut payload = packet.payload.as_ref();
                let slot = CSetContainerSlot::read(&mut payload, &CURRENT_MC_VERSION).unwrap();
                assert!(payload.is_empty());
                assert!(slot.slot_data.0.are_equal(stack));
                updates.push((slot.window_id, slot.state_id.0, slot.slot));
            }
        }
        // Exact packet equality also rejects an extra update to crafting slot 0.
        assert_eq!(updates, expected);
    }

    #[tokio::test]
    async fn replace_player_items_sends_menu_slots_and_live_revisions() {
        let mut fixture = Fixture::new().await;
        let mut state = initial_state(&fixture.packets().await, 0);
        for (slot_name, inventory_slot, menu_slot, item) in [
            ("hotbar.0", 0, 36, &Item::STICK),
            ("inventory.0", 9, 9, &Item::GOLD_INGOT),
            ("armor.feet", 36, 8, &Item::IRON_BOOTS),
            ("armor.legs", 37, 7, &Item::IRON_LEGGINGS),
            ("armor.chest", 38, 6, &Item::IRON_CHESTPLATE),
            ("armor.head", 39, 5, &Item::IRON_HELMET),
            ("weapon.offhand", 40, 45, &Item::SHIELD),
        ] {
            let command = format!(
                "item replace entity @s {slot_name} with minecraft:{}",
                item.registry_key,
            );
            assert_eq!(fixture.execute(&command).unwrap(), 1);
            let stack = ItemStack::new(1, item);
            assert!(
                fixture
                    .player
                    .inventory()
                    .get_stack(inventory_slot)
                    .are_equal(&stack)
            );
            state = (state + 1) & 32767;
            assert_slot_updates(&fixture.packets().await, &[(0, state, menu_slot)], &stack);

            // Replacing with the same item still succeeds without resending it.
            assert_eq!(fixture.execute(&command).unwrap(), 1);
            assert_slot_updates(&fixture.packets().await, &[], &stack);
        }
        assert!(
            fixture
                .execute("item replace entity @s container.50 with stone")
                .is_err()
        );
        assert_slot_updates(&fixture.packets().await, &[], ItemStack::EMPTY);
    }

    #[tokio::test]
    async fn replace_player_items_preserves_open_menu_and_its_own_slot_state() {
        let mut fixture = Fixture::new().await;
        let mut player_state = initial_state(&fixture.packets().await, 0);
        let chest = Arc::new(SimpleInventory::new(27));
        let chest_stack = ItemStack::new(5, &Item::DIAMOND);
        chest.set_stack(0, chest_stack.clone());
        let menu: SharedScreenHandler = Arc::new(Mutex::new(create_generic_9x3(
            7,
            fixture.player.inventory(),
            chest.clone(),
            fixture.player.as_ref(),
        )));
        fixture
            .player
            .open_handled_screen_direct(menu.clone(), &TextComponent::text("Inventory witness"));
        menu.lock()
            .unwrap()
            .update_sync_handler(fixture.sync.clone());
        let mut menu_state = initial_state(&fixture.packets().await, 7);
        assert_ne!(player_state, menu_state);

        for (slot_name, inventory_slot, player_slot, container_slot) in [
            ("hotbar.0", 0, 36, Some(54)),
            ("inventory.0", 9, 9, Some(27)),
            ("armor.head", 39, 5, None),
            ("weapon.offhand", 40, 45, None),
        ] {
            assert_eq!(
                fixture
                    .execute(&format!(
                        "item replace entity @s {slot_name} with gold_ingot 37"
                    ))
                    .unwrap(),
                1,
            );
            let stack = ItemStack::new(37, &Item::GOLD_INGOT);
            assert!(
                fixture
                    .player
                    .inventory()
                    .get_stack(inventory_slot)
                    .are_equal(&stack)
            );
            player_state = (player_state + 1) & 32767;
            let mut expected = vec![(0, player_state, player_slot)];
            if let Some(slot) = container_slot {
                menu_state = (menu_state + 1) & 32767;
                expected.push((7, menu_state, slot));
            }
            assert_slot_updates(&fixture.packets().await, &expected, &stack);
            assert!(Arc::ptr_eq(
                &menu,
                &fixture.player.current_screen_handler.lock().unwrap(),
            ));
            assert!(chest.get_stack(0).are_equal(&chest_stack));
            assert_eq!(
                menu.lock()
                    .unwrap()
                    .get_behaviour()
                    .revision
                    .load(Ordering::Relaxed),
                menu_state as u32,
            );
        }
    }
}
