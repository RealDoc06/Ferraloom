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

fn live_level() -> crate::level::Level {
    use crate::level::{Level, LevelContext, Region, chunk::LevelChunk};
    use std::sync::Arc;
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ferraloom_util::ChunkPos::new(0, 0),
            0..6,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level
}
fn removal_trace(level: &mut crate::level::Level) -> String {
    use crate::level::{
        effects::LevelEffect,
        game_event::test_support::{self as events, Observation},
    };
    use ferraloom_protocol::{
        codec::Holder,
        play::{SoundSource, clientbound::Packet},
    };
    events::take(level)
        .into_iter()
        .filter_map(|entry| match entry {
            Observation::GameEvent {
                event,
                position,
                source,
                affected_state,
            } => {
                assert!(source.is_none());
                Some(format!(
                    "g:{}:{:.1},{:.1},{:.1}:{}",
                    event.protocol_id(),
                    position.x,
                    position.y,
                    position.z,
                    affected_state.unwrap().id()
                ))
            }
            Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => match *packet {
                Packet::Sound(sound) => {
                    let Holder::Reference(key) = sound.sound.0 else {
                        panic!("registered sound")
                    };
                    assert_eq!(sound.source, SoundSource::Blocks);
                    Some(format!(
                        "s:{key}:{:.1},{:.1},{:.1}:blocks:{:.1}:{:.1}",
                        f64::from(sound.x) / 8.0,
                        f64::from(sound.y) / 8.0,
                        f64::from(sound.z) / 8.0,
                        sound.volume,
                        sound.pitch
                    ))
                }
                Packet::LevelEvent(event) => Some(format!(
                    "l:{}:{},{},{}:{}",
                    event.event_type, event.pos.x, event.pos.y, event.pos.z, event.data
                )),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn native_live_class_removal_replacement_and_clear_notifications_match() {
    use crate::level::game_event::test_support as events;
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/block-entity-removal.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = 0;
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let block = Block::from_name(f[1]).unwrap();
        let mut level = live_level();
        level.set_block_state(POS, block.default_state()).unwrap();
        let mut state = create(POS, block.default_state()).unwrap();
        let context = level.context().clone();
        state.load_with_components(
            &parse_compound(if block == Block::JUKEBOX {
                "{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:7L}"
            } else {
                "{CustomName:'old'}"
            })
            .unwrap(),
            context.registry_context().unwrap(),
        );
        level.set_block_entity(state).unwrap();
        let old = level.block_entity_ref(POS).unwrap().unwrap();
        level.retain_block_entity(old).unwrap();
        events::observe(&mut level);
        events::take(&mut level);
        match f[2] {
            "break" => level.remove_block_entity(POS).unwrap(),
            "replace" => level
                .set_block_entity(create(POS, block.default_state()).unwrap())
                .unwrap(),
            "clear" => level
                .detach_block_entities_for_unload(ferraloom_util::ChunkPos::containing(POS))
                .unwrap(),
            _ => panic!("native removal mode"),
        }
        assert_eq!(removal_trace(&mut level), f[3], "{row}");
        assert_eq!(
            level.block_entity::<BlockEntityData>(old).unwrap().removed,
            f[4] == "true",
            "{row}"
        );
        let current = events::staged_block_entity_reference(&level, POS);
        assert_eq!(
            current
                .map(|handle| level
                    .block_entity_state(handle)
                    .unwrap()
                    .data()
                    .removed
                    .to_string())
                .unwrap_or_else(|| "none".into()),
            f[5],
            "{row}"
        );
        assert_eq!(
            level
                .block_entity_state(old)
                .unwrap()
                .save_custom_only()
                .unwrap(),
            parse_compound(f[6]).unwrap(),
            "{row}"
        );
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(old).is_ok());
        if let Some(current) = current {
            assert_ne!(old, current);
            assert!(!level.block_entity_state(current).unwrap().data().removed);
        }
        level.release_block_entity(old);
        level.collect_detached_block_entities();
        assert!(matches!(
            level.block_entity_state(old),
            Err(crate::level::LevelError::MissingBlockEntity(_))
        ));
        checked += 1;
    }
    assert_eq!(checked, 6);
}
#[test]
fn unported_live_services_remain_required_and_empty_jukebox_tick_is_native() {
    for block in [Block::BEACON, Block::CONDUIT, Block::JUKEBOX] {
        let mut level = live_level();
        let state = if block == Block::JUKEBOX {
            block
                .default_state()
                .with(ferraloom_data::properties::HAS_RECORD, true)
                .unwrap()
        } else {
            block.default_state()
        };
        level.set_block_state(POS, state).unwrap();
        level.set_block_entity(create(POS, state).unwrap()).unwrap();
        let handle = level.block_entity_ref(POS).unwrap().unwrap();
        let behavior = level.block_entity_state(handle).unwrap().behavior();
        if block == Block::JUKEBOX {
            behavior.tick(handle, &mut level).unwrap();
            handle
                .load_custom_only(
                    &parse_compound(
                        "{RecordItem:{id:'music_disc_13',count:1},ticks_since_song_started:7L}",
                    )
                    .unwrap(),
                    &mut level,
                )
                .unwrap();
        }
        assert!(
            matches!(behavior.tick(handle,&mut level),Err(crate::level::LevelError::UnsupportedBlockEntity(found)) if found==block)
        );
    }
    for block in [Block::JUKEBOX, Block::OAK_SHELF] {
        let mut level = live_level();
        level.set_block_state(POS, block.default_state()).unwrap();
        level
            .set_block_entity(create(POS, block.default_state()).unwrap())
            .unwrap();
        assert!(
            matches!(crate::block::containers::access::block_container_at(POS,&level),Err(crate::level::LevelError::UnsupportedBlockEntity(found)) if found==block)
        );
    }
}
