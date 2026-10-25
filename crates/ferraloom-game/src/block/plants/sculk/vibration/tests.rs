use super::*;
use crate::{
    block::entity::{
        create,
        sculk::{SculkCatalystBlockEntity, SculkSensorBlockEntity},
    },
    level::{chunk::LevelChunk, game_event::test_support},
};
use ferraloom_data::{
    BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
use std::{path::Path, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 80, 8);
fn level() -> Level {
    let mut level = crate::block::plants::tests::level_with_height(8);
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    level.set_block_tags(Arc::new(TagSet::load(&resources).unwrap()));
    level.set_item_tags(Arc::new(TagSet::load(&resources).unwrap()));
    let section = ferraloom_chunk::LevelChunkSection::new(
        ferraloom_chunk::Strategy::block_states(),
        ferraloom_chunk::Strategy::biomes(Arc::new(
            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
        )),
        ferraloom_chunk::BiomeId(0),
    );
    for x in -1..=1 {
        for z in -1..=1 {
            let chunk = ChunkPos::new(x, z);
            if x != 0 || z != 0 {
                level
                    .add_chunk(LevelChunk::new(chunk, 0..8, section.clone()))
                    .unwrap();
            }
            level.set_chunk_block_ticking(chunk, false).unwrap();
        }
    }
    level
}
fn install(state: BlockState, level: &mut Level) -> BlockEntityRef {
    level.remove_block_entity(POS).unwrap();
    level.set_block_state(POS, state).unwrap();
    level.set_block_entity(create(POS, state).unwrap()).unwrap();
    test_support::take_vibration_changes(level);
    *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
    level.block_entity_ref(POS).unwrap().unwrap()
}
fn info(event: GameEvent, distance: f32) -> VibrationInfo {
    VibrationInfo {
        game_event: event,
        distance,
        pos: Vec3::at_center_of(POS.relative(Direction::East, 3).into()),
        uuid: None,
        projectile_owner_uuid: None,
        entity: None,
    }
}
fn summary(receiver: BlockEntityRef, level: &mut Level) -> String {
    let state = level.get_block_state(POS).unwrap();
    let frequency = level
        .block_entity::<SculkSensorBlockEntity>(receiver)
        .map_or(0, |sensor| sensor.last_vibration_frequency);
    let data = data(receiver, level).unwrap();
    let fields = format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        state.id(),
        frequency,
        data.current_vibration.is_some(),
        data.travel_time_in_ticks,
        data.selection_strategy.current_vibration_data.is_some(),
        data.reload_vibration_particle
    );
    format!(
        "{fields}\t{}\t{}",
        test_support::take_vibration_changes(level).join(";"),
        level.random().next_long()
    )
}
#[test]
fn native_vibration_selector_travel_receive_and_listener_lifecycle() {
    let path = std::env::var_os("FERRALOOM_VIBRATION_VECTORS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/java/blocks/plants/sculk/vibrations.tsv")
        });
    let vectors = std::fs::read_to_string(path).expect("real ServerLevel vibration vectors");
    let mut level = level();
    let mut counts = [0; 6];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        match f[0] {
            "frequency" => {
                assert_eq!(
                    frequency(GameEvent::from_name(f[1]).unwrap()),
                    f[2].parse::<i32>().unwrap(),
                    "{row}"
                );
                counts[0] += 1;
            }
            "selector" => {
                let mut data = VibrationData::default();
                add_candidate(
                    &mut data,
                    info(GameEvent::from_name(f[1]).unwrap(), 1.0),
                    10,
                );
                add_candidate(
                    &mut data,
                    info(
                        GameEvent::from_name(f[2]).unwrap(),
                        f32::from_bits(f[3].parse::<i32>().unwrap() as u32),
                    ),
                    10 + f[4].parse::<i64>().unwrap(),
                );
                let selected = data
                    .selection_strategy
                    .current_vibration_data
                    .as_ref()
                    .unwrap();
                assert_eq!(selected.event.game_event.name(), f[5], "{row}");
                assert_eq!(
                    selected.event.distance.to_bits(),
                    f[6].parse::<i32>().unwrap() as u32,
                    "{row}"
                );
                assert_eq!((selected.tick < 10).to_string(), f[7], "{row}");
                counts[1] += 1;
            }
            "tick" | "select-tick" | "receive" => {
                let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
                let receiver = install(state, &mut level);
                let distance = f32::from_bits(f[2].parse::<i32>().unwrap() as u32);
                let begin = match f[0] {
                    "tick" => {
                        let data = data_mut(receiver, &mut level).unwrap();
                        data.current_vibration = Some(info(GameEvent::BLOCK_PLACE, distance));
                        data.reload_vibration_particle = f[3].parse().unwrap();
                        data.travel_time_in_ticks = f[4].parse().unwrap();
                        for x in -1..=1 {
                            for z in -1..=1 {
                                level
                                    .set_chunk_block_ticking(
                                        ChunkPos::new(x, z),
                                        f[5].parse().unwrap(),
                                    )
                                    .unwrap();
                            }
                        }
                        tick(receiver, &mut level).unwrap();
                        counts[2] += 1;
                        6
                    }
                    "select-tick" => {
                        let time = level.game_time() + f[3].parse::<i64>().unwrap();
                        add_candidate(
                            data_mut(receiver, &mut level).unwrap(),
                            info(GameEvent::BLOCK_PLACE, distance),
                            time,
                        );
                        for x in -1..=1 {
                            for z in -1..=1 {
                                level
                                    .set_chunk_block_ticking(
                                        ChunkPos::new(x, z),
                                        f[4].parse().unwrap(),
                                    )
                                    .unwrap();
                            }
                        }
                        tick(receiver, &mut level).unwrap();
                        counts[3] += 1;
                        5
                    }
                    "receive" => {
                        crate::block::plants::sculk::sensor::receive(
                            receiver,
                            GameEvent::BLOCK_PLACE,
                            None,
                            distance,
                            &mut level,
                        )
                        .unwrap();
                        counts[4] += 1;
                        3
                    }
                    _ => unreachable!(),
                };
                assert_eq!(
                    summary(receiver, &mut level),
                    f[begin..].join("\t"),
                    "{row}"
                );
            }
            "catalyst" => {
                for x in 4..=12 {
                    for z in 4..=12 {
                        for y in 79..=83 {
                            level
                                .set_block_state(
                                    BlockPos::new(x, y, z),
                                    if y == 79 { Block::STONE } else { Block::AIR }.default_state(),
                                )
                                .unwrap();
                        }
                    }
                }
                for x in -1..=1 {
                    for z in -1..=1 {
                        level
                            .set_chunk_block_ticking(ChunkPos::new(x, z), f[1].parse().unwrap())
                            .unwrap();
                    }
                }
                let receiver = install(Block::SCULK_CATALYST.default_state(), &mut level);
                level
                    .block_entity_mut::<SculkCatalystBlockEntity>(receiver)
                    .unwrap()
                    .sculk_spreader
                    .add_cursors(POS.relative(Direction::East, 2), f[2].parse().unwrap());
                for _ in 0..f[3].parse::<usize>().unwrap() {
                    crate::block::plants::sculk::tick_catalyst(receiver, &mut level).unwrap();
                }
                assert_eq!(
                    level
                        .block_entity_state(receiver)
                        .unwrap()
                        .save_custom_only()
                        .unwrap(),
                    ferraloom_nbt::parse_compound(f[4]).unwrap(),
                    "{row}"
                );
                let mut blocks = Vec::new();
                for x in 4..=12 {
                    for y in 79..=83 {
                        for z in 4..=12 {
                            let pos = BlockPos::new(x, y, z);
                            let state = level.get_block_state(pos).unwrap();
                            let initial = if y == 79 {
                                Block::STONE
                            } else if pos == POS {
                                Block::SCULK_CATALYST
                            } else {
                                Block::AIR
                            }
                            .default_state();
                            if state != initial {
                                blocks.push(format!("{x},{y},{z},{}", state.id()));
                            }
                        }
                    }
                }
                assert_eq!(blocks.join(";"), f[5], "{row}");
                assert_eq!(
                    level.random().next_long(),
                    f[6].parse::<i64>().unwrap(),
                    "{row}"
                );
            }
            "lifecycle" => {
                // Both native rows are replayed together below on the same receiver.
                counts[5] += 1;
            }
            _ => panic!("unclassified native vector {row}"),
        }
    }
    assert_eq!(&counts[1..], &[108, 180, 54, 60, 2]);
    let receiver = install(Block::SCULK_SENSOR.default_state(), &mut level);
    let context = GameEventContext {
        source_entity: None,
        affected_state: Some(Block::STONE.default_state()),
    };
    level
        .game_event(
            GameEvent::BLOCK_PLACE,
            Vec3::at_center_of(POS.relative(Direction::East, 2).into()),
            None,
            context.affected_state,
        )
        .unwrap();
    assert!(
        data(receiver, &level)
            .unwrap()
            .selection_strategy
            .current_vibration_data
            .is_some()
    );
    data_mut(receiver, &mut level)
        .unwrap()
        .selection_strategy
        .current_vibration_data = None;
    level.retain_block_entity(receiver).unwrap();
    level.remove_block_entity(POS).unwrap();
    level
        .game_event(
            GameEvent::BLOCK_PLACE,
            Vec3::at_center_of(POS.relative(Direction::East, 2).into()),
            None,
            context.affected_state,
        )
        .unwrap();
    assert!(
        data(receiver, &level)
            .unwrap()
            .selection_strategy
            .current_vibration_data
            .is_none()
    );
    level.release_block_entity(receiver);
    eprintln!("native vibration case counts: {counts:?}");
}

#[test]
fn live_catalyst_keeps_canonical_cursors_and_required_reentrancy() {
    let mut level = level();
    let state = Block::SCULK_CATALYST.default_state();
    let receiver = install(state, &mut level);
    let before = level.random().clone().next_long();
    crate::block::plants::sculk::tick_catalyst(receiver, &mut level).unwrap();
    assert_eq!(
        level.random().next_long(),
        before,
        "empty native spreader consumes no RNG"
    );
    level
        .block_entity_mut::<SculkCatalystBlockEntity>(receiver)
        .unwrap()
        .sculk_spreader
        .add_cursors(POS.above(1), 50);
    let generation = level
        .block_entity::<SculkCatalystBlockEntity>(receiver)
        .unwrap()
        .sculk_spreader
        .cursor_generation;
    level
        .block_entity_mut::<SculkCatalystBlockEntity>(receiver)
        .unwrap()
        .sculk_spreader
        .replace_cursors(Vec::new());
    let access = crate::block::plants::sculk::SpreaderAccess::Live {
        receiver,
        generation,
    };
    assert!(
        matches!(access.read(&crate::block::LevelAccessRef::Server(&mut level)),Err(LevelError::UnsupportedSculkCursorReentrancy(r)) if r==receiver)
    );
}
