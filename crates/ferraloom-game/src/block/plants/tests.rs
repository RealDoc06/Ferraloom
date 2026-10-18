use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    level::{Level, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::{BlockState, properties as p};
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::sync::Arc;
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    level_with_height(2)
}
pub(super) fn level_with_height(sections: i32) -> Level {
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    level.set_has_skylight(true);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..sections,
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
    super::fixtures::install_loot(&mut level);
    level.rules_mut().block_drops = Some(false);
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

pub(super) fn tags() -> (
    Arc<ferraloom_data::datapack::TagSet<Block>>,
    Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>,
) {
    type Tags = (
        Arc<ferraloom_data::datapack::TagSet<Block>>,
        Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>,
    );
    static TAGS: std::sync::OnceLock<Tags> = std::sync::OnceLock::new();
    TAGS.get_or_init(|| {
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
            Arc::new(
                TagSet::from_tags(fluids, ferraloom_data::registries::fluid::ALL.len()).unwrap(),
            ),
        )
    })
    .clone()
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
    let (blocks, fluids) = tags();
    for light in [0, 11, 12, 15] {
        let mut level = level();
        super::fixtures::install_loot(&mut level);
        level.rules_mut().block_drops = Some(false);
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
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
        assert_eq!(
            reader.is_collision_shape_full_block(state, pos).unwrap(),
            f[7] == "true",
            "full collision {line}"
        );
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
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    for block in [Block::FARMLAND, Block::DIRT_PATH] {
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
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
pub(super) fn crop_random_ticks_match_real_server_outcomes_and_rng() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    let text = super::fixtures::native_text(
        "random-crop",
        include_bytes!("../../../tests/java/blocks/plants/random-ticks.tsv.gz"),
    );
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
fn spreading_snowy_support_matches_native_occlusion_and_full_fluids() {
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
        // Actual decay, neighbor updates, tick queues and RNG are replayed by
        // runtime_tests::native_ground_ticks_match_changes_queues_and_rng.
        count += 1;
    }
    assert_eq!(count, 1612);
}

#[test]
fn nearby_flower_scan_preserves_native_section_and_cell_order() {
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/eyeblossom-scan.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let origin = BlockPos::new(
            fields[2].parse().unwrap(),
            fields[3].parse().unwrap(),
            fields[4].parse().unwrap(),
        );
        let mut level = level();
        for pos in [
            ChunkPos::new(0, 1),
            ChunkPos::new(1, 0),
            ChunkPos::new(1, 1),
        ] {
            level
                .add_chunk(LevelChunk::new(
                    pos,
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
        }
        for dx in [-3, 0, 3] {
            for dy in [-2, 0, 2] {
                for dz in [-3, 0, 3] {
                    level
                        .set_block_state(origin.offset(dx, dy, dz), state)
                        .unwrap();
                }
            }
        }
        let mut actual = Vec::new();
        eyeblossom::for_each_matching(state, origin, &mut level, |_, at| {
            actual.push(format!(
                "{}:{}:{}",
                at.x - origin.x,
                at.y - origin.y,
                at.z - origin.z
            ));
            Ok(())
        })
        .unwrap();
        assert_eq!(actual.join(","), fields[5], "{line}");
        count += 1;
    }
    assert_eq!(count, 6);
}

#[test]
fn farmland_trampling_preserves_draw_order_permission_size_and_actor() {
    use crate::{
        entity::{
            EntityData, EntityRef, EntityState,
            animal::Cow,
            dimensions::EntityDimensions,
            experience_orb::ExperienceOrb,
            player::{GameType, ServerPlayer},
        },
        level::interaction::SpawnProtectionSnapshot,
    };
    use ferraloom_data::{EntityType, registries::game_event::Id as GameEvent};
    use ferraloom_util::{
        Uuid, Vec3,
        random::{LegacyRandomSource, Random, RandomSource},
    };
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let actor = EntityRef(1);
    // kind: 0 nonliving, 1 cow, 2 small cow, 3 player.
    for (kind, mob_griefing, protected, distance, tramples) in [
        (0, true, false, 1.5, false),
        (1, true, false, 1.5, true),
        (1, false, false, 1.5, false),
        (2, true, false, 1.5, false),
        (3, false, true, 1.5, false),
        (3, false, false, 1.5, true),
        (3, true, false, 0.0, false),
    ] {
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.rules_mut().mob_griefing = mob_griefing;
        let state = Block::FARMLAND.default_state();
        level.set_block_state(pos, state).unwrap();
        let location = Vec3::new(8.5, 5.9375, 8.5);
        let entity = match kind {
            0 => {
                let mut orb = ExperienceOrb::new(1, 0);
                orb.parent.set_pos(location);
                EntityState::ExperienceOrb(Box::new(orb))
            }
            1 | 2 => {
                let mut data = EntityData::new(EntityType::COW, 1, 0);
                if kind == 2 {
                    data.dimensions = EntityDimensions::scalable(0.25, 0.25);
                }
                data.set_pos(location);
                EntityState::Cow(Box::new(Cow::with_parent(data)))
            }
            _ => EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "Farmer".into(),
                1,
                location,
                GameType::Creative,
            ))),
        };
        level
            .update_entity_chunk_status(ChunkPos::ZERO, crate::level::entity::Visibility::Ticking)
            .unwrap();
        level.add_entity(actor, entity).unwrap();
        let mut shared = crate::ServerShared::default();
        shared.ops.set_operator(
            &ferraloom_protocol::login::GameProfile {
                id: Uuid::new(3, 4),
                name: "Other".into(),
                properties: vec![],
            },
            Some(crate::commands::permissions::PermissionLevel::Owners),
        );
        level.install_shared(Box::new(shared));
        level.set_spawn_protection(SpawnProtectionSnapshot::new(
            "minecraft:overworld".parse().unwrap(),
            "minecraft:overworld".parse().unwrap(),
            pos,
            if protected { 10 } else { 0 },
        ));
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
        let mut expected_random = LegacyRandomSource::new(42);
        expected_random.next_float();
        ground::GROUND
            .fall_on(state, pos, actor, distance, &mut level)
            .unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap(),
            if tramples {
                Block::DIRT.default_state()
            } else {
                state
            }
        );
        assert_eq!(level.random().next_long(), expected_random.next_long());
        let effects = level.take_effects();
        assert_eq!(
            effects.iter().any(|effect| matches!(effect,
            LevelEffect::GameEvent { event: GameEvent::BLOCK_CHANGE, source: Some(source),
                affected_state: Some(new_state), .. }
                if *source == actor && new_state.block() == Block::DIRT)),
            tramples
        );
        assert_eq!(
            level.entity::<EntityData>(actor).unwrap().position.y,
            if tramples { 6.0 } else { location.y },
            "kind={kind} mob_griefing={mob_griefing} protected={protected} distance={distance}"
        );
    }
}

#[test]
fn tree_growth_restores_captured_saplings_and_uses_the_live_random_stream() {
    use crate::level::LevelContext;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use ferraloom_worldgen::feature::{Feature, FeatureContext, FeatureRegistry};
    #[derive(Debug)]
    struct RejectedTree;
    impl Feature for RejectedTree {
        fn place(
            &self,
            context: &mut FeatureContext<'_, '_>,
        ) -> Result<bool, ferraloom_worldgen::Error> {
            // A feature can fail after consuming RNG and writing outside the
            // captured saplings. TreeGrower only restores those saplings.
            context.random()?.next_int_bound(17);
            context
                .level
                .set_block([10, 5, 8], Block::STONE.default_state(), 260)?;
            Ok(false)
        }
    }
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    for mega in [false, true] {
        let original = level();
        let mut features = FeatureRegistry::default();
        for id in ["oak", "fancy_oak", "mega_spruce", "mega_pine"] {
            features
                .features
                .insert(id.parse().unwrap(), Arc::new(RejectedTree));
        }
        let services = original.context();
        let context = Arc::new(LevelContext::new(
            0,
            63,
            services.generator.clone(),
            Arc::new(features),
            services.structures.clone(),
            services.damage_types().clone(),
            services.initial_difficulty,
        ));
        let mut level = Level::new(original.into_region(), context);
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        let block = if mega {
            Block::SPRUCE_SAPLING
        } else {
            Block::OAK_SAPLING
        };
        let grower = tree_grower::TreeGrower::for_block(block).unwrap();
        let mut captured = Vec::new();
        for (x, z) in if mega {
            vec![(0, 0), (1, 0), (0, 1), (1, 1)]
        } else {
            vec![(0, 0)]
        } {
            let at = pos.offset(x, 0, z);
            let state = block
                .default_state()
                .with(p::STAGE, ((x + z) % 2) as u8)
                .unwrap();
            level
                .set_block_state(at.below(1), Block::DIRT.default_state())
                .unwrap();
            level.set_block_state(at, state).unwrap();
            captured.push((at, state));
        }
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
        let mut expected = LegacyRandomSource::new(42);
        expected.next_int_bound(if mega { 2 } else { 10 });
        expected.next_int_bound(17);
        assert!(
            !grower
                .grow_tree(block.default_state(), pos, &mut level)
                .unwrap()
        );
        for (at, state) in captured {
            assert_eq!(level.get_block_state(at).unwrap(), state);
        }
        assert_eq!(
            level.get_block_state(pos.east(2)).unwrap(),
            Block::STONE.default_state()
        );
        assert_eq!(level.random().next_long(), expected.next_long());
    }
}

#[test]
pub(super) fn native_bonemeal_snapshots_and_rng_match_registered_growth() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::collections::BTreeMap;
    let text = super::fixtures::native_text(
        "bonemeal",
        include_bytes!("../../../tests/java/blocks/plants/bonemeal.tsv.gz"),
    );
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 100, 8);
    let mut matched = 0;
    let mut pending = 0;
    let cells = |text: &str| -> BTreeMap<(i32, i32, i32), BlockState> {
        text.split(',')
            .filter(|cell| !cell.is_empty())
            .map(|cell| {
                let fields: Vec<i32> = cell
                    .split(':')
                    .map(|value| value.parse().unwrap())
                    .collect();
                (
                    (fields[0], fields[1], fields[2]),
                    BlockState::from_id(fields[3] as u16).unwrap(),
                )
            })
            .collect()
    };
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        if matches!(
            state.block().class(),
            BlockClass::StemBlock | BlockClass::PitcherCropBlock
        ) && f.len() < 10
        {
            // Older captures omit required crop/stem light inputs. The fresh
            // ServerLevel oracle supplies them and requires all 5,808 matches.
            pending += 1;
            continue;
        }
        let mut level = level_with_height(8);
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.rules_mut().block_drops = Some(false);
        super::fixtures::install_loot(&mut level);
        if f.len() >= 10 {
            let mut sky = ferraloom_light::DataLayer::new(f[8].parse().unwrap());
            let mut block = ferraloom_light::DataLayer::new(f[9].parse().unwrap());
            if f.len() >= 12 {
                // Upper pitcher halves query the lower cell's real light, which
                // can differ even within this section.
                let below = pos.below(1);
                sky.set(
                    (below.x & 15) as u8,
                    (below.y & 15) as u8,
                    (below.z & 15) as u8,
                    f[10].parse().unwrap(),
                );
                block.set(
                    (below.x & 15) as u8,
                    (below.y & 15) as u8,
                    (below.z & 15) as u8,
                    f[11].parse().unwrap(),
                );
            }
            level.set_light_snapshot(
                ChunkPos::containing(pos),
                crate::level::weather::ChunkLightSnapshot {
                    sky: [(pos.y >> 4, sky)].into(),
                    block: [(pos.y >> 4, block)].into(),
                },
            );
        }
        for ((x, y, z), state) in cells(f[5]) {
            level.set_block_state(pos.offset(x, y, z), state).unwrap();
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
            0,
        );
        let behavior = bonemealable_behavior(state.block()).expect("ported fixture class");
        let mut access = LevelAccessRef::Server(&mut level);
        let valid = behavior
            .is_valid_bonemeal_target(state, pos, &mut access, BonemealSource::Interaction)
            .unwrap();
        assert_eq!(valid, f[3] == "true", "target {line}");
        let success = valid
            && behavior
                .is_bonemeal_success(state, pos, &mut access, BonemealSource::Interaction)
                .unwrap();
        assert_eq!(success, f[4] == "true", "success {line}");
        if success {
            behavior
                .perform_bonemeal(state, pos, &mut access, BonemealSource::Interaction)
                .unwrap();
        }
        let mut actual = BTreeMap::new();
        for z in -3..=3 {
            for y in -20..=20 {
                for x in -3..=3 {
                    let at = pos.offset(x, y, z);
                    let state = level.get_block_state(at).unwrap();
                    if !state.is_air() {
                        actual.insert((x, y, z), state);
                    }
                }
            }
        }
        assert_eq!(
            actual,
            cells(f[6]),
            "snapshot state={state:?} seed={} before={}",
            f[2],
            f[5]
        );
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "random {line}"
        );
        matched += 1;
    }
    assert_eq!(matched + pending, 5808);
    assert!(matched > 0);
    if std::env::var_os("FERRALOOM_PLANTS_ORACLE").is_some() {
        assert_eq!(pending, 0);
    }
    eprintln!(
        "plant bonemeal: {matched} complete matches; {pending} awaiting native light snapshots"
    );
}

#[test]
fn chorus_survival_and_six_neighbor_updates_match_native_topologies() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/chorus-support.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let below = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let mode: i32 = f[4].parse().unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(pos.below(1), below).unwrap();
        if f[3] == "true" {
            level
                .set_block_state(pos.above(1), Block::STONE.default_state())
                .unwrap();
        }
        if (1..=3).contains(&mode) {
            level
                .set_block_state(pos.north(1), Block::CHORUS_PLANT.default_state())
                .unwrap();
            if mode == 1 {
                level
                    .set_block_state(pos.north(1).below(1), Block::END_STONE.default_state())
                    .unwrap();
            }
            if mode == 3 {
                level
                    .set_block_state(pos.south(1), Block::CHORUS_PLANT.default_state())
                    .unwrap();
            }
        }
        if mode == 4 {
            level
                .set_block_state(pos.east(1), Block::STONE.default_state())
                .unwrap();
        }
        if mode == 5 {
            level
                .set_block_state(pos.west(1), Block::CHORUS_FLOWER.default_state())
                .unwrap();
        }
        assert_eq!(
            level.can_survive(state, pos).unwrap(),
            f[5] == "true",
            "survival {line}"
        );
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(1234)), 0);
        let direction = Direction::VALUES[f[6].parse::<usize>().unwrap()];
        let at = pos.relative(direction, 1);
        let neighbor = level.get_block_state(at).unwrap();
        let behavior = level
            .required_block_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap();
        let updated = behavior
            .update_shape(
                state,
                pos,
                direction,
                at,
                neighbor,
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(updated.id(), f[7].parse::<u16>().unwrap(), "update {line}");
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let ticks: Vec<_> = saved
            .block_ticks
            .unwrap()
            .into_iter()
            .map(|tick| format!("schedule:{}", tick.delay.unwrap()))
            .collect();
        let expected: Vec<_> = f[9]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(ticks, expected, "ticks {line}");
        count += 1;
    }
    assert_eq!(count, 20160);
}

#[test]
fn multiface_and_sculk_vein_configs_match_native_replacement_and_sources() {
    use super::multiface_spreader::{SpreadPos, SpreaderConfig};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/spreader-config.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let config = match f[1] {
            "0" => SpreaderConfig::Default(Block::GLOW_LICHEN),
            "1" => SpreaderConfig::SculkVein,
            "2" => SpreaderConfig::SculkVeinSameSpace,
            _ => unreachable!(),
        };
        if f[0] == "spreader-source" {
            let state = BlockState::from_id(f[2].parse().unwrap()).unwrap();
            let face = Direction::VALUES[f[3].parse::<usize>().unwrap()];
            assert_eq!(
                config.is_other_block_valid_as_source(state),
                f[4] == "true",
                "source {line}"
            );
            assert_eq!(
                config.can_spread_from(state, face),
                f[5] == "true",
                "face {line}"
            );
        } else {
            let mut level = level();
            level.set_block_tags(blocks.clone());
            level.set_fluid_tags(fluids.clone());
            for cell in f[8].split(',') {
                let c: Vec<i32> = cell.split(':').map(|s| s.parse().unwrap()).collect();
                level
                    .set_block_state(
                        pos.offset(c[0], c[1], c[2]),
                        BlockState::from_id(c[3] as u16).unwrap(),
                    )
                    .unwrap();
            }
            let spread = SpreadPos {
                pos: pos.offset(f[2].parse().unwrap(), f[3].parse().unwrap(), 0),
                face: Direction::VALUES[f[4].parse::<usize>().unwrap()],
            };
            let old = BlockState::from_id(f[5].parse().unwrap()).unwrap();
            let reader = LevelReaderRef::Server(&level);
            assert_eq!(
                config
                    .state_can_be_replaced(&reader, pos, spread, old)
                    .unwrap(),
                f[6] == "true",
                "replacement {line}"
            );
            assert_eq!(
                config.can_spread_into(&reader, pos, spread).unwrap(),
                f[7] == "true",
                "placement {line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 8730);
}

#[test]
fn multiface_spread_candidate_order_matches_vanilla() {
    use super::multiface_spreader::{SpreadPos, SpreaderConfig};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/spreader-candidates.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let config = match f[1] {
            "0" => SpreaderConfig::Default(Block::GLOW_LICHEN),
            "1" => SpreaderConfig::SculkVein,
            "2" => SpreaderConfig::SculkVeinSameSpace,
            _ => unreachable!(),
        };
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        for cell in f[7].split(',') {
            let c: Vec<i32> = cell.split(':').map(|s| s.parse().unwrap()).collect();
            level
                .set_block_state(
                    pos.offset(c[0], c[1], c[2]),
                    BlockState::from_id(c[3] as u16).unwrap(),
                )
                .unwrap();
        }
        let expected = if f[6] == "-" {
            None
        } else {
            let c: Vec<i32> = f[6].split(':').map(|s| s.parse().unwrap()).collect();
            Some(SpreadPos {
                pos: pos.offset(c[0], c[1], c[2]),
                face: Direction::VALUES[c[3] as usize],
            })
        };
        assert_eq!(
            config
                .get_spread(
                    &LevelReaderRef::Server(&level),
                    BlockState::from_id(f[2].parse().unwrap()).unwrap(),
                    pos,
                    Direction::VALUES[f[4].parse::<usize>().unwrap()],
                    Direction::VALUES[f[5].parse::<usize>().unwrap()],
                )
                .unwrap(),
            expected,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 4320);
}

#[test]
fn coral_and_hanging_roots_placement_match_native_states_schedules_and_rng() {
    use crate::block::DirectionalPlaceContext;
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/special-placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let stack = ItemStack::default();
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let fluid = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_block_state(pos, fluid).unwrap();
        let neighbor: usize = f[3].parse().unwrap();
        if neighbor < 6 {
            level
                .set_block_state(pos.relative(Direction::VALUES[neighbor], 1), fluid)
                .unwrap();
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
            0,
        );
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            Direction::North,
            &stack,
            Direction::Up,
        )
        .unwrap();
        let actual = LevelAccessRef::Server(&mut level)
            .get_state_for_placement(block, &context)
            .unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[5].parse::<i32>().unwrap(),
            "state {line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[6].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let delays = saved
            .block_ticks
            .unwrap_or_default()
            .into_iter()
            .map(|tick| tick.delay.unwrap())
            .collect::<Vec<_>>();
        let expected = f[7]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(delays, expected, "scheduled death {line}");
        count += 1;
    }
    assert_eq!(count, 1680);
}

#[test]
fn leaf_distance_and_water_ticks_match_native_updates_in_every_direction() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/leaf-updates.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let neighbor = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[3].parse::<usize>().unwrap()];
        let at = pos.relative(direction, 1);
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(at, neighbor).unwrap();
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(1234)), 0);
        let actual = level
            .required_block_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                state,
                pos,
                direction,
                at,
                neighbor,
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[4].parse::<u16>().unwrap(), "state {line}");
        assert_eq!(
            level.random().next_long(),
            f[5].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let mut actual = saved
            .block_ticks
            .unwrap_or_default()
            .iter()
            .map(|tick| format!("schedule:{}", tick.delay.unwrap()))
            .collect::<Vec<_>>();
        actual.extend(
            saved
                .fluid_ticks
                .unwrap_or_default()
                .iter()
                .map(|tick| format!("fluid:{}", tick.delay.unwrap())),
        );
        let mut expected = f[6]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "ticks {line}");
        count += 1;
    }
    assert_eq!(count, 5376);
}

#[test]
fn double_plant_and_moss_post_placement_match_native_updates_and_rng() {
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{
        core::Plane,
        random::{LegacyRandomSource, Random, RandomSource},
    };
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/post-placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level
            .set_block_state(
                pos.below(1),
                BlockState::from_id(f[2].parse().unwrap()).unwrap(),
            )
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        level
            .set_block_state(
                pos.above(1),
                BlockState::from_id(f[3].parse().unwrap()).unwrap(),
            )
            .unwrap();
        let mode: u8 = f[4].parse().unwrap();
        if mode != 0 {
            for &direction in Plane::Horizontal.faces() {
                if mode == 1 && direction != Direction::North {
                    continue;
                }
                for y in 0..=1 {
                    level
                        .set_block_state(
                            pos.above(y).relative(direction, 1),
                            Block::STONE.default_state(),
                        )
                        .unwrap();
                }
            }
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[5].parse().unwrap())),
            0,
        );
        level
            .set_placed_by(state, pos, None, &ItemStack::default())
            .unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap().id(),
            f[6].parse::<u16>().unwrap(),
            "base {line}"
        );
        assert_eq!(
            level.get_block_state(pos.above(1)).unwrap().id(),
            f[7].parse::<u16>().unwrap(),
            "upper {line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 4704);
}

#[test]
fn creative_double_plant_destruction_matches_native_states_and_rng() {
    use crate::entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_util::{
        Uuid, Vec3,
        random::{LegacyRandomSource, Random, RandomSource},
    };
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/creative-destruction.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let actor = EntityRef(1);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = level();
        super::fixtures::install_loot(&mut level);
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level
            .add_entity(
                actor,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(0, 1),
                    "plants".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Creative,
                ))),
            )
            .unwrap();
        level
            .set_block_state(pos.below(2), Block::CLAY.default_state())
            .unwrap();
        level
            .set_block_state(
                pos.below(1),
                BlockState::from_id(f[2].parse().unwrap()).unwrap(),
            )
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        let result = level.player_will_destroy(state, pos, actor).unwrap();
        assert_eq!(result.id(), f[4].parse::<u16>().unwrap(), "return {line}");
        assert_eq!(
            level.get_block_state(pos.below(1)).unwrap().id(),
            f[5].parse::<u16>().unwrap(),
            "lower {line}"
        );
        assert_eq!(
            level.get_block_state(pos).unwrap().id(),
            f[6].parse::<u16>().unwrap(),
            "current {line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        assert!(level.take_effects().iter().any(|effect|matches!(effect,LevelEffect::GameEvent{source:Some(source),affected_state:Some(affected),..} if *source==actor && *affected==state)),"actor {line}");
        count += 1;
    }
    assert_eq!(count, 512);
}
