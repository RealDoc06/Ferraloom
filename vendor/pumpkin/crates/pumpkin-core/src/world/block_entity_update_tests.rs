//! Added UNRUN under the source-only R18 follow-up. These observe encoded packet
//! production after real publication/edits, without claiming socket delivery.

use super::*;
use crate::command::commands::data::tests::Fixture;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::{packet::MultiVersionJavaPacket, ser::NetworkReadExt};

const POS: BlockPos = BlockPos::new(0, 64, 0);
const OTHER: BlockPos = BlockPos::new(1, 64, 0);
const CONTAINERS: [(&str, &Block); 3] = [
    ("minecraft:chest", &Block::CHEST),
    ("minecraft:trapped_chest", &Block::TRAPPED_CHEST),
    ("minecraft:barrel", &Block::BARREL),
];

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(nbt) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected compound fixture");
    };
    nbt
}

fn container(id: &str) -> Arc<dyn BlockEntity> {
    let mut nbt = compound(
        "{CustomName:'Vault',lock:{components:{'minecraft:custom_name':'Secret key'}},components:{'minecraft:custom_data':{secret:'Private map data'}},Items:[{Slot:0b,id:'minecraft:diamond',count:3,components:{'minecraft:custom_data':{secret:'Private item data'}}}]}",
    );
    nbt.put_string("id", id.to_owned());
    nbt.put_int("x", POS.0.x);
    nbt.put_int("y", POS.0.y);
    nbt.put_int("z", POS.0.z);
    block_entity_from_nbt(&nbt).unwrap()
}

fn take_packets(world: &World) -> Vec<bytes::Bytes> {
    std::mem::take(&mut *world.block_entity_update_packets.lock().unwrap())
}

fn assert_sign_packet(world: &World, live: &dyn BlockEntity, text: &str) {
    let packets = take_packets(world);
    assert_eq!(packets.len(), 1);
    let mut reader = packets[0].as_ref();
    assert_eq!(
        reader.get_var_int().unwrap().0,
        CBlockEntityData::to_id(CURRENT_MC_VERSION)
    );
    assert_eq!(reader.get_block_pos(&CURRENT_MC_VERSION).unwrap(), POS);
    assert_eq!(reader.get_var_int().unwrap().0, live.get_id() as i32);
    let tag = reader
        .get_nbt_with_version(&CURRENT_MC_VERSION)
        .unwrap()
        .unwrap();
    assert!(reader.is_empty());
    let nbt = tag.extract_compound().unwrap();
    let messages = nbt
        .get_compound("front_text")
        .unwrap()
        .get_list("messages")
        .unwrap();
    assert_eq!(messages[0].extract_string(), Some(text));
    assert_eq!(nbt.get("Items"), None);
}

#[tokio::test]
async fn container_publication_and_notification_send_no_java_block_entity_packet() {
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
    assert!(take_packets(world).is_empty());
    for (id, block) in CONTAINERS {
        let live = container(id);
        fixture.install(block, live.clone());
        let persisted = fixture.pending_data();
        assert_eq!(persisted.get_list("Items").unwrap().len(), 1);
        for field in ["CustomName", "lock", "components"] {
            assert!(persisted.get(field).is_some());
        }
        assert!(take_packets(world).is_empty(), "{id}: no packet on add");
        world.update_block_entity(&live);
        assert!(take_packets(world).is_empty(), "{id}: no packet on update");
        assert_eq!(fixture.pending_data(), persisted);
        assert!(Arc::ptr_eq(&world.get_block_entity(&POS).unwrap(), &live));
    }
}

#[tokio::test]
async fn container_data_edits_keep_single_and_multi_block_updates_without_nbt_packets() {
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
    for (id, block) in CONTAINERS {
        fixture.install(block, container(id));
        fixture
            .execute("data modify block 0 64 0 Items[0].count set value 7")
            .unwrap();
        let persisted = fixture.pending_data();
        assert_eq!(
            persisted.get_list("Items").unwrap()[0]
                .extract_compound()
                .unwrap()
                .get_int("count"),
            Some(7)
        );
        world.flush_block_updates();
        assert_eq!(
            world.take_flushed_block_changes(),
            vec![(POS, block.default_state.id)]
        );
        assert!(take_packets(world).is_empty(), "{id}: single-block flush");
        assert_eq!(fixture.pending_data(), persisted);

        fixture
            .execute("data modify block 0 64 0 Items[0].count set value 9")
            .unwrap();
        fixture.set_state(OTHER, Block::STONE.default_state.id);
        world.register_block_change(OTHER, Block::STONE.default_state.id);
        world.flush_block_updates();
        let changes = world.take_flushed_block_changes();
        assert_eq!(changes.len(), 2);
        assert!(changes.contains(&(POS, block.default_state.id)));
        assert!(changes.contains(&(OTHER, Block::STONE.default_state.id)));
        assert!(take_packets(world).is_empty(), "{id}: multi-block flush");
        assert_eq!(
            fixture.pending_data().get_list("Items").unwrap()[0]
                .extract_compound()
                .unwrap()
                .get_int("count"),
            Some(9)
        );
    }
}

#[tokio::test]
async fn sign_updates_keep_encoded_nbt_on_publication_notification_and_both_flush_paths() {
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
    for (id, block) in [
        ("minecraft:sign", &Block::OAK_SIGN),
        ("minecraft:hanging_sign", &Block::OAK_HANGING_SIGN),
    ] {
        let mut nbt = compound("{front_text:{messages:['Visible','','','']}}");
        nbt.put_string("id", id.to_owned());
        nbt.put_int("x", POS.0.x);
        nbt.put_int("y", POS.0.y);
        nbt.put_int("z", POS.0.z);
        let live = block_entity_from_nbt(&nbt).unwrap();
        fixture.install(block, live.clone());
        assert_sign_packet(world, live.as_ref(), "Visible");
        world.update_block_entity(&live);
        assert_sign_packet(world, live.as_ref(), "Visible");

        fixture
            .execute("data modify block 0 64 0 front_text.messages[0] set value Edited")
            .unwrap();
        world.flush_block_updates();
        assert_sign_packet(world, live.as_ref(), "Edited");
        assert_eq!(
            world.take_flushed_block_changes(),
            vec![(POS, block.default_state.id)]
        );

        fixture.set_state(OTHER, Block::STONE.default_state.id);
        world.queue_block_updates(&[
            (POS, block.default_state.id),
            (OTHER, Block::STONE.default_state.id),
        ]);
        world.flush_block_updates();
        assert_sign_packet(world, live.as_ref(), "Edited");
        assert_eq!(world.take_flushed_block_changes().len(), 2);
    }
}
