use super::*;
use ferraloom_data::Item;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::BlockPos;
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn prime(entity: &mut BlockEntityState) {
    match entity {
        BlockEntityState::Beacon(entity) => {
            entity.levels = 7;
            entity.last_check_y = 91;
        }
        BlockEntityState::Conduit(entity) => {
            entity.tick_count = 91;
            entity.active_rotation = 1.25;
            entity.is_active = true;
            entity.is_hunting = true;
            entity.next_ambient_sound_activation = 123;
            entity.effect_blocks.push(POS.above(1));
        }
        BlockEntityState::Shelf(entity) => {
            entity.items[0] = ItemStack::new(Item::DIAMOND, 3, Default::default());
            entity.align_items_to_bottom = true;
        }
        BlockEntityState::Jukebox(entity) => {
            entity.item = ItemStack::new(Item::DIAMOND, 3, Default::default());
            entity.song_player.ticks_since_song_started = 91;
        }
        BlockEntityState::CopperGolemStatue(_) => {}
        _ => unreachable!(),
    }
}
fn scalar(
    entity: &BlockEntityState,
    registries: &dyn ferraloom_protocol::codec::RegistryContext,
) -> String {
    match entity {
        BlockEntityState::Beacon(entity) => format!("{},{}", entity.levels, entity.last_check_y),
        BlockEntityState::Conduit(entity) => format!(
            "{},{},{},{},{},{}",
            entity.tick_count,
            entity.active_rotation.to_bits(),
            entity.is_active,
            entity.is_hunting,
            entity.effect_blocks.len(),
            entity.next_ambient_sound_activation
        ),
        BlockEntityState::Shelf(entity) => entity.align_items_to_bottom.to_string(),
        BlockEntityState::Jukebox(entity) => format!(
            "{},{},{}",
            entity
                .song_player
                .song
                .as_ref()
                .map_or("null", |song| song.key.as_str()),
            entity.song_player.ticks_since_song_started,
            entity.comparator_output(registries)
        ),
        BlockEntityState::CopperGolemStatue(_) => "base".into(),
        _ => unreachable!(),
    }
}
#[test]
fn native_jukebox_beacon_conduit_shelf_statue_factories_and_components_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/factory-batch-three.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let context = crate::level::LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut checked = [0; 3];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] == "beacon-attach" {
            let mut entity = create(POS, Block::BEACON.default_state()).unwrap();
            entity.set_level(9, f[1].parse().unwrap());
            let BlockEntityState::Beacon(entity) = entity else {
                unreachable!()
            };
            assert_eq!(entity.last_check_y, f[2].parse::<i32>().unwrap(), "{row}");
            assert!(entity.parent.has_level);
            checked[2] += 1;
            continue;
        }
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        assert!(supports(state.block()), "{row}");
        assert!(supports_creation(state.block()), "{row}");
        let mut entity = new_block_entity(POS, state).unwrap().unwrap();
        match f[0] {
            "factory3-data" => {
                if f[2] != "@new" {
                    prime(&mut entity);
                    entity.load_with_components(&parse_compound(f[2]).unwrap(), registries);
                }
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(f[4]).unwrap(),
                    "update {row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    f[5] == "true",
                    "packet {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    components::load(Some(&parse_compound(f[6]).unwrap()), registries),
                    "components {row}"
                );
                entity.set_level(0, -64);
                assert_eq!(scalar(&entity, registries), f[7], "scalar {row}");
                assert_eq!(
                    entity.has_server_ticker(Some(true)).unwrap(),
                    f[8] == "true",
                    "ticker {row}"
                );
                let saved = entity.save_with_full_metadata().unwrap();
                let round = load_static(POS, state, &saved, registries)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    round.save_with_full_metadata().unwrap(),
                    parse_compound(f[9]).unwrap(),
                    "round trip {row}"
                );
                checked[0] += 1;
            }
            "factory3-components" => {
                prime(&mut entity);
                let item =
                    ItemStack::from_nbt(&Tag::Compound(parse_compound(f[2]).unwrap())).unwrap();
                entity.apply_components_from_item_stack(&item).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    components::load(Some(&parse_compound(f[4]).unwrap()), registries),
                    "components {row}"
                );
                let mut stripped = entity.save_custom_only().unwrap();
                entity.remove_components_from_tag(&mut stripped);
                assert_eq!(stripped, parse_compound(f[5]).unwrap(), "stripped {row}");
                checked[1] += 1;
            }
            _ => panic!("native vector kind"),
        }
    }
    assert_eq!(checked, [147, 50, 3]);
}
