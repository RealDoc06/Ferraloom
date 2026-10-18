use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    level::{Level, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::properties as p;
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::sync::Arc;
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY));
    level.set_has_skylight(true);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..2,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    level
}
#[test]
fn upper_double_plant_requires_its_own_lower_half() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let upper = Block::TALL_GRASS
        .default_state()
        .with(p::HALF, p::Half::Upper)
        .unwrap();
    for (below, expected) in [
        (Block::TALL_GRASS.default_state(), true),
        (upper, false),
        (Block::LARGE_FERN.default_state(), false),
        (Block::STONE.default_state(), false),
    ] {
        level.set_block_state(pos.below(1), below).unwrap();
        assert_eq!(
            survival::PLANT
                .can_survive(upper, pos, &LevelReaderRef::Server(&level))
                .unwrap(),
            expected,
            "{below:?}"
        );
    }
}
#[test]
fn multiface_neighbor_loss_removes_only_the_unsupported_face() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let north =
        multiface::with_face(Block::GLOW_LICHEN.default_state(), Direction::North, true).unwrap();
    let both = multiface::with_face(north, Direction::South, true).unwrap();
    for (input, expected) in [
        (
            both,
            multiface::with_face(both, Direction::North, false).unwrap(),
        ),
        (north, Block::AIR.default_state()),
    ] {
        let output = multiface::MULTIFACE
            .update_shape(
                input,
                pos,
                Direction::North,
                pos.north(1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(output, expected);
    }
}
#[test]
fn leaf_decay_gates_preserve_persistent_and_connected_leaves() {
    let mut level = level();
    let (blocks, fluids) = tags();
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    let pos = BlockPos::new(8, 5, 8);
    let decaying = Block::OAK_LEAVES
        .default_state()
        .with(p::WATERLOGGED, true)
        .unwrap();
    // Isolate this callback: vanilla removeBlock delegates fluid replacement to setBlock.
    for state in [
        decaying.with(p::PERSISTENT, true).unwrap(),
        decaying.with(p::DISTANCE, 6).unwrap(),
    ] {
        level.set_block_state(pos, state).unwrap();
        leaves::LEAVES.random_tick(state, pos, &mut level).unwrap();
        assert_eq!(level.get_block_state(pos).unwrap(), state);
    }
    // Dry leaves exercise the same no-particle removal without starting fluid flow.
    let dry = decaying.with(p::WATERLOGGED, false).unwrap();
    level.set_block_state(pos, dry).unwrap();
    leaves::LEAVES.random_tick(dry, pos, &mut level).unwrap();
    assert_eq!(
        level.get_block_state(pos).unwrap(),
        Block::AIR.default_state()
    );
    let effects = level.take_effects();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e,LevelEffect::DropResources{state,..} if *state==dry))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, LevelEffect::DestroyParticles { .. }))
    );
}
#[test]
fn shared_raw_brightness_requires_light_context() {
    let level = level();
    assert!(
        LevelReaderRef::Server(&level)
            .get_raw_brightness(BlockPos::new(8, 5, 8), 0)
            .is_err()
    );
}

fn tags() -> (
    Arc<ferraloom_data::datapack::TagSet<Block>>,
    Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>,
) {
    use ferraloom_data::datapack::{Pack, ResourceManager, TagSet, load_tags};
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let blocks = load_tags(&resources, "block", |id, _| {
        Block::from_name(&id.to_string())
    });
    let fluids = load_tags(&resources, "fluid", |id, _| {
        ferraloom_data::registries::fluid::Id::from_name(&id.to_string())
    });
    assert!(blocks.diagnostics.is_empty());
    assert!(fluids.diagnostics.is_empty());
    (
        Arc::new(TagSet::from_tags(blocks, ferraloom_data::BLOCKS.len()).unwrap()),
        Arc::new(TagSet::from_tags(fluids, ferraloom_data::registries::fluid::ALL.len()).unwrap()),
    )
}
#[test]
fn survival_and_six_direction_shape_updates_match_vanilla_levels() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/survival.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let chunk = ChunkPos::new(0, 0);
    let mut count = 0;
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let support = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        let light = fields[3].parse().unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_light_snapshot(
            chunk,
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(light))].into(),
                block: Default::default(),
            },
        );
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(1234)), 0);
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(pos.below(1), support).unwrap();
        if fields[0] == "survive" {
            assert_eq!(
                level.can_survive(state, pos).unwrap(),
                fields[4] == "true",
                "{line}"
            );
        } else {
            let direction = Direction::VALUES[fields[4].parse::<usize>().unwrap()];
            let neighbor_pos = pos.relative(direction, 1);
            let neighbor = level.get_block_state(neighbor_pos).unwrap();
            let behavior = level
                .required_block_behavior(state.block(), BlockMethod::UpdateShape)
                .unwrap();
            let next = behavior
                .update_shape(
                    state,
                    pos,
                    direction,
                    neighbor_pos,
                    neighbor,
                    &mut LevelAccessRef::Server(&mut level),
                )
                .unwrap();
            assert_eq!(next.id(), fields[5].parse::<u16>().unwrap(), "{line}");
            assert_eq!(
                level.random().next_long(),
                fields[6].parse::<i64>().unwrap(),
                "RNG {line}"
            );
            let mut saved = ferraloom_storage::SerializableChunkData::default();
            level.save_chunk_ticks(chunk, &mut saved).unwrap();
            let mut ticks = Vec::new();
            for tick in saved.block_ticks.unwrap() {
                ticks.push(format!("schedule:{}", tick.delay.unwrap()));
            }
            for tick in saved.fluid_ticks.unwrap() {
                ticks.push(format!("fluid:{}", tick.delay.unwrap()));
            }
            let mut expected = fields[7]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            ticks.sort();
            expected.sort();
            assert_eq!(ticks, expected, "ticks {line}");
        }
        count += 1;
    }
    assert_eq!(count, 79520);
}

#[test]
fn crop_bonemeal_and_growth_speed_match_real_vanilla_methods() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/growth.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let chunk = ChunkPos::new(0, 0);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_light_snapshot(
            chunk,
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(15))].into(),
                block: Default::default(),
            },
        );
        if f[0] == "grow" {
            level.set_random_streams(
                Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
                0,
            );
            level.set_block_state(pos, state).unwrap();
            level
                .set_block_state(pos.below(1), Block::FARMLAND.default_state())
                .unwrap();
            crops::grow_crops(state, pos, &mut LevelAccessRef::Server(&mut level)).unwrap();
            assert_eq!(
                level.get_block_state(pos).unwrap().id(),
                f[3].parse::<u16>().unwrap(),
                "{line}"
            );
            assert_eq!(
                level.random().next_long(),
                f[4].parse::<i64>().unwrap(),
                "RNG {line}"
            );
        } else {
            let moisture = f[2].parse().unwrap();
            let pattern = f[3].parse::<u8>().unwrap();
            let soil = Block::FARMLAND
                .default_state()
                .with(p::MOISTURE, moisture)
                .unwrap();
            for x in -1..=1 {
                for z in -1..=1 {
                    level
                        .set_block_state(pos.below(1).offset(x, 0, z), soil)
                        .unwrap();
                }
            }
            for (bit, at) in [
                (1, pos.west(1)),
                (2, pos.north(1)),
                (4, pos.west(1).north(1)),
                (8, pos.east(1).south(1)),
            ] {
                if pattern & bit != 0 {
                    level.set_block_state(at, state).unwrap();
                }
            }
            assert_eq!(
                crops::growth_speed(state.block(), pos, &LevelReaderRef::Server(&level))
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 592);
}

#[test]
fn snow_melts_only_above_eleven_block_light_and_emits_destroy_context() {
    for light in [0, 11, 12, 15] {
        let mut level = level();
        let pos = BlockPos::new(8, 5, 8);
        let state = Block::SNOW.default_state();
        level
            .set_block_state(pos.below(1), Block::STONE.default_state())
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_light_snapshot(
            ChunkPos::new(0, 0),
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(15))].into(),
                block: [(0, ferraloom_light::DataLayer::new(light))].into(),
            },
        );
        snow::SNOW.random_tick(state, pos, &mut level).unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap(),
            if light > 11 {
                Block::AIR.default_state()
            } else {
                state
            }
        );
        let effects = level.take_effects();
        assert_eq!(effects.iter().filter(|e|matches!(e,LevelEffect::GameEvent{event:ferraloom_data::registries::game_event::Id::BLOCK_DESTROY,affected_state:Some(affected),..} if *affected==state)).count(),usize::from(light>11));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, LevelEffect::DestroyParticles { .. }))
        );
    }
}

#[test]
fn plant_outline_and_empty_context_collision_shapes_match_vanilla() {
    use crate::block::BlockCollisionContext;
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/shapes.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let level = level();
    let reader = LevelReaderRef::Server(&level);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let pos = BlockPos::new(
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
            f[4].parse().unwrap(),
        );
        let shape = if f[5] == "true" {
            reader
                .collision_shape(state, pos, &BlockCollisionContext::empty())
                .unwrap()
        } else {
            reader.shape(state, pos).unwrap()
        };
        let actual = shape
            .to_aabbs()
            .iter()
            .map(|bb| {
                [bb.min_x, bb.min_y, bb.min_z, bb.max_x, bb.max_y, bb.max_z]
                    .iter()
                    .map(|d| format!("{:x}", d.to_bits()))
                    .collect::<Vec<_>>()
                    .join(":")
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, f[6], "{line}");
        count += 1;
    }
    assert_eq!(count, 3810);
}

#[test]
fn bamboo_crown_rewrites_and_stage_draws_match_vanilla() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/bamboo.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let below = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let two = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(pos.below(1), below).unwrap();
        level.set_block_state(pos.below(2), two).unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[5].parse().unwrap())),
            0,
        );
        bamboo::grow(
            state,
            pos,
            f[4].parse().unwrap(),
            &mut LevelAccessRef::Server(&mut level),
        )
        .unwrap();
        let output = (-2..=1)
            .map(|dy| {
                level
                    .get_block_state(pos.above(dy))
                    .unwrap()
                    .id()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(output, f[6], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 384);
}

#[test]
fn farmland_water_boundary_and_crop_protection_preserve_random_stream() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    for (water_offset, crop, moisture, expected) in [
        (Some((4, 1, 4)), false, 0, Some(7)),
        (Some((5, 1, 4)), false, 1, Some(0)),
        (Some((4, 2, 4)), false, 1, Some(0)),
        (None, true, 0, Some(0)),
        (None, false, 0, None),
    ] {
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        let state = Block::FARMLAND
            .default_state()
            .with(p::MOISTURE, moisture)
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        if let Some((x, y, z)) = water_offset {
            level
                .set_block_state(pos.offset(x, y, z), Block::WATER.default_state())
                .unwrap();
        }
        if crop {
            level
                .set_block_state(pos.above(1), Block::WHEAT.default_state())
                .unwrap();
        }
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(7331)), 0);
        ground::GROUND.random_tick(state, pos, &mut level).unwrap();
        let actual = level.get_block_state(pos).unwrap();
        assert_eq!(
            actual,
            expected.map_or(Block::DIRT.default_state(), |m| state
                .with(p::MOISTURE, m)
                .unwrap())
        );
        assert_eq!(
            level.random().next_long(),
            LegacyRandomSource::new(7331).next_long()
        );
        let effects = level.take_effects();
        assert_eq!(effects.iter().filter(|effect| matches!(effect, LevelEffect::GameEvent { event: ferraloom_data::registries::game_event::Id::BLOCK_CHANGE, affected_state: Some(state), .. } if *state == Block::DIRT.default_state())).count(), usize::from(expected.is_none()));
    }
}

#[test]
fn ground_scheduled_tick_retains_farmland_but_always_converts_path() {
    let pos = BlockPos::new(8, 5, 8);
    for block in [Block::FARMLAND, Block::DIRT_PATH] {
        let mut level = level();
        let state = block.default_state();
        level.set_block_state(pos, state).unwrap();
        ground::GROUND.tick(state, pos, &mut level).unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap(),
            if block == Block::FARMLAND {
                state
            } else {
                Block::DIRT.default_state()
            }
        );
    }
}

#[test]
fn plant_clone_items_match_vanilla_for_all_registered_states() {
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/clone-items.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let level = level();
    let reader = LevelReaderRef::Server(&level);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let stack = reader
            .get_clone_item_stack(state, BlockPos::ZERO, f[4] == "true")
            .unwrap();
        let (item, amount) = stack.0.map_or((ferraloom_data::Item::AIR, 0), |stack| {
            (stack.item, stack.count)
        });
        assert_eq!(item.protocol_id(), f[2].parse::<u16>().unwrap(), "{line}");
        assert_eq!(amount, f[3].parse::<i32>().unwrap(), "{line}");
        count += 1;
    }
    assert_eq!(count, 268);
}

#[test]
fn nylium_occlusion_and_bonemeal_metadata_match_vanilla() {
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/ground.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let pos = BlockPos::new(8, 5, 8);
    let mut level = level();
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        if f[0] == "nylium-alive" {
            let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
            let above = BlockState::from_id(f[2].parse().unwrap()).unwrap();
            level.set_block_state(pos.above(1), above).unwrap();
            assert_eq!(
                nylium::can_be_nylium(state, pos, &LevelReaderRef::Server(&level)).unwrap(),
                f[3] == "true",
                "{line}"
            );
        } else {
            let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
            let behavior = bonemealable_behavior(block).unwrap();
            assert_eq!(
                behavior.bonemeal_type(),
                if f[2] == "NEIGHBOR_SPREADER" {
                    BonemealType::NeighborSpreader
                } else {
                    BonemealType::Grower
                },
                "{line}"
            );
            assert_eq!(
                behavior.particle_pos(pos),
                pos.above(f[3].parse().unwrap()),
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 817);
}

#[test]
fn vine_placement_adds_the_first_supported_unoccupied_looking_face() {
    use crate::block::DirectionalPlaceContext;
    use ferraloom_protocol::component::ItemStack;
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let mut level = level();
        let clicked = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(pos, clicked).unwrap();
        match f[3] {
            "1" => {
                level
                    .set_block_state(pos.north(1), Block::STONE.default_state())
                    .unwrap();
            }
            "2" => {
                level
                    .set_block_state(pos.above(1), Block::STONE.default_state())
                    .unwrap();
            }
            "3" => {
                level
                    .set_block_state(
                        pos.above(1),
                        multiface::with_face(Block::VINE.default_state(), Direction::North, true)
                            .unwrap(),
                    )
                    .unwrap();
            }
            _ => {}
        }
        let reader = LevelReaderRef::Server(&level);
        let stack = ItemStack::default();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let context =
            DirectionalPlaceContext::new(&reader, pos, direction, &stack, Direction::Up).unwrap();
        let actual = LevelAccessRef::Server(&mut level)
            .get_state_for_placement(Block::VINE, &context)
            .unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[4].parse::<i32>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 792);
}

#[test]
fn disabled_vine_spreading_does_not_consume_random() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    let mut level = level();
    level.rules_mut().spread_vines = false;
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(1234)), 0);
    vines::VINES
        .random_tick(
            Block::VINE.default_state(),
            BlockPos::new(8, 5, 8),
            &mut level,
        )
        .unwrap();
    assert_eq!(
        level.random().next_long(),
        LegacyRandomSource::new(1234).next_long()
    );
}

#[test]
fn crop_random_ticks_match_real_server_outcomes_and_rng() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/random-ticks.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_light_snapshot(
            ChunkPos::new(0, 0),
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(f[5].parse().unwrap()))].into(),
                block: Default::default(),
            },
        );
        let soil = if state.block() == Block::NETHER_WART {
            Block::SOUL_SAND.default_state()
        } else {
            Block::FARMLAND
                .default_state()
                .with(p::MOISTURE, f[2].parse().unwrap())
                .unwrap()
        };
        for x in -1..=1 {
            for z in -1..=1 {
                level
                    .set_block_state(pos.below(1).offset(x, 0, z), soil)
                    .unwrap();
            }
        }
        level.set_block_state(pos, state).unwrap();
        let pattern: u8 = f[3].parse().unwrap();
        if pattern & 1 != 0 {
            level
                .set_block_state(pos.west(1), state.block().default_state())
                .unwrap();
        }
        if pattern & 2 != 0 {
            level
                .set_block_state(pos.north(1), state.block().default_state())
                .unwrap();
        }
        if pattern == 4 {
            level
                .set_block_state(pos.west(1).north(1), state.block().default_state())
                .unwrap();
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[4].parse().unwrap())),
            0,
        );
        level
            .required_block_behavior(state.block(), crate::block::BlockMethod::RandomTick)
            .unwrap()
            .random_tick(state, pos, &mut level)
            .unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap().id(),
            f[6].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 5760);
}

#[test]
fn plant_placement_states_and_rng_match_real_vanilla_contexts() {
    use crate::block::DirectionalPlaceContext;
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/placement-states.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let mut level = level();
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    let pos = BlockPos::new(8, 5, 8);
    for face in [Direction::North, Direction::South, Direction::West] {
        level
            .set_block_state(pos.relative(face, 1), Block::STONE.default_state())
            .unwrap();
    }
    let stack = ItemStack::default();
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let below = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        let clicked = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        let above = BlockState::from_id(f[5].parse().unwrap()).unwrap();
        level.set_block_state(pos, clicked).unwrap();
        level.set_block_state(pos.below(1), below).unwrap();
        level.set_block_state(pos.above(1), above).unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
            0,
        );
        let direction = Direction::VALUES[f[6].parse::<usize>().unwrap()];
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            direction,
            &stack,
            Direction::Up,
        )
        .unwrap();
        let result = LevelAccessRef::Server(&mut level)
            .get_state_for_placement(block, &context)
            .unwrap();
        assert_eq!(
            result.map_or(-1, |state| i32::from(state.id())),
            f[7].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 69600);
}

#[test]
fn weighted_tree_selection_preserves_native_order_and_single_entry_draws() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/tree-selection.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let grower = tree_grower::TreeGrower::for_block(
            Block::from_protocol_id(f[1].parse().unwrap()).unwrap(),
        )
        .unwrap();
        let mut random = Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap()));
        let flowers = f[3] == "true";
        let (tree, mega) = if f[4] == "true" {
            let mega = grower.configured_mega_feature(&mut random);
            (grower.configured_feature(&mut random, flowers), mega)
        } else {
            let tree = grower.configured_feature(&mut random, flowers);
            (tree, grower.configured_mega_feature(&mut random))
        };
        assert_eq!(
            tree.map_or_else(|| "-".to_owned(), |id| format!("minecraft:{id}")),
            f[5],
            "tree {line}"
        );
        assert_eq!(
            mega.map_or_else(|| "-".to_owned(), |id| format!("minecraft:{id}")),
            f[6],
            "mega {line}"
        );
        assert_eq!(
            random.next_long(),
            f[7].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 2816);
}

#[test]
fn frosted_ice_on_place_schedules_native_delay_and_keeps_rng() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/frosted-place.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let mut level = level();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
            0,
        );
        level
            .required_block_behavior(state.block(), crate::block::BlockMethod::OnPlace)
            .unwrap()
            .on_place(state, pos, Block::AIR.default_state(), false, &mut level)
            .unwrap();
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let ticks = saved.block_ticks.unwrap();
        assert_eq!(ticks.len(), 1);
        assert_eq!(
            ticks[0].delay.unwrap(),
            f[3].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[4].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 256);
}

#[test]
fn spreading_snowy_support_and_decay_match_native_occlusion_and_full_fluids() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/grass-support.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let mut level = level();
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let above = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(pos.above(1), above).unwrap();
        let alive = fields[3] == "true";
        assert_eq!(
            spreading::can_stay_alive(state, pos, &LevelReaderRef::Server(&level)).unwrap(),
            alive,
            "{line}"
        );
        assert_eq!(
            spreading::can_propagate(state, pos, &LevelReaderRef::Server(&level)).unwrap(),
            fields[4] == "true",
            "{line}"
        );
        if !alive {
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(7331)), 0);
            spreading::SPREADING_SNOWY
                .random_tick(state, pos, &mut level)
                .unwrap();
            assert_eq!(
                level.get_block_state(pos).unwrap(),
                Block::DIRT.default_state(),
                "{line}"
            );
            assert_eq!(
                level.random().next_long(),
                LegacyRandomSource::new(7331).next_long(),
                "decay RNG {line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 1612);
}
