use super::*;
use crate::data::player_server::test::{close_fixture, player_fixture, stop_chunk_workers};
use pumpkin_data::{
    data_component_impl::{DataComponentImpl, TooltipDisplayImpl},
    packet::CURRENT_MC_VERSION,
};
use pumpkin_inventory::Inventory;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::{
    ClientPacket, RawPacket, ServerPacket,
    java::{client::play::CSetContainerContent, packet_decoder::TCPNetworkDecoder},
    packet::MultiVersionJavaPacket,
    ser::NetworkWriteExt,
};
use std::time::Duration;
use tokio::{io::BufReader, net::TcpStream};

async fn packets(
    java: &JavaClient,
    reader: &mut TCPNetworkDecoder<BufReader<&mut TcpStream>>,
) -> Vec<RawPacket> {
    java.try_send_packet(&CSetSelectedSlot::new(0));
    java.flush_channel();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut packets = Vec::new();
        loop {
            let packet = reader.get_raw_packet().await.unwrap();
            if packet.id == CSetSelectedSlot::to_id(CURRENT_MC_VERSION) {
                return packets;
            }
            packets.push(packet);
        }
    })
    .await
    .expect("creative response did not reach the FIFO marker")
}

async fn dispatch(java: &JavaClient, player: &Arc<Player>, server: &Arc<Server>, body: &[u8]) {
    let mut packet = Vec::new();
    packet
        .put_var_int(&VarInt(SSetCreativeSlot::to_id(CURRENT_MC_VERSION)))
        .unwrap();
    packet.extend_from_slice(body);
    let mut frame = Vec::new();
    frame.put_var_int(&VarInt(packet.len() as i32)).unwrap();
    frame.extend_from_slice(&packet);
    let mut decoder = TCPNetworkDecoder::new(BufReader::new(frame.as_slice()));
    let packet = decoder.get_raw_packet().await.unwrap();
    assert!(
        java.handle_play_packet(player, server, &packet)
            .unwrap()
            .is_none()
    );
}

fn saved_slot(nbt: &NbtCompound, slot: i8) -> ItemStack {
    let item = nbt
        .get_list("Inventory")
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|item| item.get_byte("Slot") == Some(slot))
        .unwrap();
    ItemStack::read_item_stack(item).unwrap()
}

#[tokio::test]
async fn creative_wire_placeholder_refuses_before_publication_resyncs_and_player_save_succeeds() {
    let mut fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let player = fixture.player.clone();
    let java = fixture.client.java().unwrap();
    player.gamemode.store(GameMode::Creative);
    let original = ItemStack::new(3, &Item::DIAMOND);
    player.inventory().set_stack(9, original.clone());
    player.on_screen_handler_opened(&player.player_screen_handler);
    let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
    packets(java, &mut reader).await;
    let entity_count = player.world().entities.load().len();
    let storage = &fixture.server.player_data_storage;

    // TooltipDisplay still discards its wire fields. Keep this independent of
    // the separately implemented Instrument codec: acceptance is not support.
    for slot in [9i16, 5, 45, -1] {
        let mut body = Vec::new();
        body.write_i16_be(slot).unwrap();
        for value in [
            1,
            i32::from(Item::STICK.id),
            1,
            0,
            i32::from(DataComponent::TooltipDisplay.to_id()),
            2,
        ] {
            body.put_var_int(&VarInt(value)).unwrap();
        }
        body.extend_from_slice(&[1, 0]); // hide_tooltip=true; zero hidden components
        let mut input = body.as_slice();
        let decoded = SSetCreativeSlot::read(&mut input, &CURRENT_MC_VERSION).unwrap();
        assert!(input.is_empty());
        assert!(
            decoded
                .clicked_item
                .0
                .get_data_component::<TooltipDisplayImpl>()
                .is_some()
        );
        assert!(validate_runtime_item_stack(decoded.clicked_item.0.as_ref()).is_err());

        dispatch(java, &player, &fixture.server, &body).await;
        assert!(player.inventory().get_stack(9).are_equal(&original));
        assert!(player.inventory().get_stack(39).is_empty());
        assert!(player.inventory().get_stack(40).is_empty());
        assert_eq!(player.world().entities.load().len(), entity_count);
        let response = packets(java, &mut reader).await;
        let updates: Vec<_> = response
            .iter()
            .filter(|packet| packet.id == CSetContainerContent::to_id(CURRENT_MC_VERSION))
            .collect();
        assert_eq!(updates.len(), 1);
        let content =
            CSetContainerContent::read(&mut updates[0].payload.as_ref(), &CURRENT_MC_VERSION)
                .unwrap();
        assert_eq!(content.window_id.0, 0);
        assert!(content.slot_data[9].0.are_equal(&original));
        assert!(content.slot_data[5].0.is_empty());
        assert!(content.slot_data[45].0.is_empty());
        assert!(content.carried_item.0.is_empty());

        storage.extract_data_and_save_player(&player).unwrap();
        let saved = storage.load_data(&player.gameprofile.id).unwrap().unwrap();
        assert!(saved_slot(&saved, 9).are_equal(&original));
    }

    let healthy = ItemStack::new_with_component(
        37,
        &Item::GOLD_INGOT,
        vec![(DataComponent::TooltipDisplay, None)],
    );
    let mut body = Vec::new();
    SSetCreativeSlot::new(9, healthy.clone().into())
        .write_packet_data(&mut body, &CURRENT_MC_VERSION)
        .unwrap();
    dispatch(java, &player, &fixture.server, &body).await;
    assert!(player.inventory().get_stack(9).are_equal(&healthy));
    storage.extract_data_and_save_player(&player).unwrap();
    let saved = storage.load_data(&player.gameprofile.id).unwrap().unwrap();
    assert!(saved_slot(&saved, 9).are_equal(&healthy));

    // Retain the central whole-file refusal as the last line of defense for
    // plugin/server-created stacks that bypass this client ingress boundary.
    let mut poisoned = NbtCompound::new();
    ItemStack::new_with_component(
        1,
        &Item::STICK,
        vec![(
            DataComponent::TooltipDisplay,
            Some(TooltipDisplayImpl.to_dyn()),
        )],
    )
    .write_item_stack(&mut poisoned);
    assert!(pumpkin_world::nbt_io::encode_compound(poisoned).is_err());
    drop(reader);
    close_fixture(&fixture).await;
}
