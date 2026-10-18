use super::*;
use crate::{Behaviors, block::BlockBehavior};
use effects::LevelEffect;
use ferraloom_util::Direction;

struct Trace {
    nested: bool,
}
impl BlockBehavior for Trace {
    fn neighbor_changed(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<redstone::Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.emit(LevelEffect::LevelEvent {
            pos,
            id: 99,
            data: 0,
        });
        if self.nested && pos == BlockPos::new(-1, 0, 0) {
            level.neighbor_changed(BlockPos::new(10, 0, 0), Block::STONE, None)?;
            level.neighbor_changed(BlockPos::new(11, 0, 0), Block::STONE, None)?;
        }
        Ok(())
    }
}
static EMPTY: Behaviors = Behaviors::EMPTY;
static PLAIN: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &Trace { nested: false });
static NESTED: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &Trace { nested: true });
fn empty_level(behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(Region::new(behaviors));
    for x in -1..=1 {
        for z in -1..=1 {
            level
                .add_chunk(chunk::LevelChunk::new(
                    ChunkPos::new(x, z),
                    -1..1,
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
    }
    level
}
fn fixture(behaviors: &'static Behaviors) -> Level {
    let mut level = empty_level(behaviors);
    for direction in Direction::VALUES {
        level
            .set_block_state(
                BlockPos::ZERO.relative(direction, 1),
                Block::STONE.default_state(),
            )
            .unwrap();
    }
    for x in [10, 11] {
        level
            .set_block_state(BlockPos::new(x, 0, 0), Block::STONE.default_state())
            .unwrap();
    }
    level
}
fn trace(level: &mut Level) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|e| match e {
            LevelEffect::LevelEvent { pos, id: 99, .. } => {
                Some(format!("{}, {}, {}", pos.x, pos.y, pos.z))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn vector(name: &str) -> &'static str {
    include_str!("../../tests/java/level/neighbors.tsv")
        .lines()
        .find_map(|line| line.strip_prefix(name).and_then(|s| s.strip_prefix('\t')))
        .unwrap()
}
#[test]
fn vanilla_neighbor_direction_order_skipped_face_and_nested_layers() {
    for (name, nested, skip) in [
        ("direction-order", false, None),
        ("skip-west", false, Some(Direction::West)),
        ("nested-layer", true, None),
    ] {
        let mut level = fixture(if nested { &NESTED } else { &PLAIN });
        level
            .update_neighbors_at_except_from_facing(BlockPos::ZERO, Block::STONE, skip, None)
            .unwrap();
        assert_eq!(trace(&mut level), vector(name), "{name}");
    }
}
#[test]
fn vanilla_neighbor_chain_limit_counts_operations_and_resets() {
    for (name, max) in [("chain-limit", 2), ("zero-limit", 0)] {
        let mut level = fixture(&NESTED);
        level.set_max_chained_neighbor_updates(max);
        level
            .update_neighbors_at(BlockPos::ZERO, Block::STONE)
            .unwrap();
        assert_eq!(trace(&mut level), vector(name), "{name}");
        level.set_max_chained_neighbor_updates(-1);
        level
            .update_neighbors_at(BlockPos::ZERO, Block::STONE)
            .unwrap();
        assert_eq!(trace(&mut level), vector("nested-layer"));
    }
}
#[test]
fn neighbor_error_clears_active_stack_before_the_next_chain() {
    let mut level = fixture(&PLAIN);
    level.remove_chunk(ChunkPos::new(-1, 0));
    assert!(matches!(
        level.update_neighbors_at(BlockPos::ZERO, Block::STONE),
        Err(LevelError::MissingChunk(_))
    ));
    level
        .neighbor_changed(BlockPos::new(10, 0, 0), Block::STONE, None)
        .unwrap();
    assert_eq!(trace(&mut level), "10, 0, 0");
}
#[test]
fn random_position_lcg_matches_signed_java_vector() {
    let mut level = fixture(&PLAIN);
    level.set_random_streams(
        ferraloom_util::random::Random::Xoroshiro(
            ferraloom_util::random::XoroshiroRandomSource::new(0),
        ),
        0,
    );
    let positions: Vec<_> = (0..4)
        .map(|_| level.get_block_random_pos(BlockPos::new(-16, -64, 32), 15))
        .collect();
    // Captured from Level#getBlockRandomPos in the same concrete Java fixture.
    let expected = vector("random-positions");
    let actual = positions
        .into_iter()
        .map(|p| format!("{}, {}, {}", p.x, p.y, p.z))
        .collect::<Vec<_>>()
        .join(";");
    assert_eq!(actual, expected);
}

fn install_tags(level: &mut Level) {
    let values = include_str!("../../tests/java/level/tags.tsv")
        .lines()
        .map(|line| {
            let (name, blocks) = line.split_once('\t').unwrap();
            (
                name.strip_prefix("tag:").unwrap().parse().unwrap(),
                blocks
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|name| Block::from_name(name).unwrap())
                    .collect(),
            )
        })
        .collect();
    let tags = ferraloom_data::datapack::TagSet::from_tags(
        ferraloom_data::datapack::Tags {
            values,
            diagnostics: Vec::new(),
        },
        ferraloom_data::BLOCKS.len(),
    )
    .unwrap();
    level.set_block_tags(Arc::new(tags));
}
#[test]
fn vanilla_set_block_flags_schedule_sand_notify_and_update_all_heightmaps() {
    use crate::block::UpdateFlags;
    let pos = BlockPos::new(8, 0, 8);
    for flags in [0, 1, 2, 3, 4, 8, 16, 32, 64, 128, 256, 512, 816] {
        let mut level = empty_level(&EMPTY);
        install_tags(&mut level);
        level
            .set_chunk_block_ticking(ChunkPos::new(0, 0), true)
            .unwrap();
        assert!(
            level
                .set_block(pos, Block::SAND.default_state(), UpdateFlags(flags), 512)
                .unwrap()
        );
        let expected = vector(&format!("setblock-{flags}"));
        let (expected_trace, expected_heights) = expected
            .strip_prefix("true:")
            .unwrap()
            .rsplit_once(':')
            .unwrap();
        let schedule = level
            .region()
            .block_ticks
            .container(ChunkPos::new(0, 0))
            .unwrap()
            .pack(0);
        assert_eq!(
            schedule.len(),
            usize::from(expected_trace.contains("schedule:2"))
        );
        if let Some(tick) = schedule.first() {
            assert_eq!(tick.delay, 2);
        }
        let trace = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::SectionEmptiness { empty, .. } => Some(format!("section:{empty}")),
                LevelEffect::BlockUpdated { .. } => Some("send".into()),
                LevelEffect::PoiChanged { .. } => Some("poi".into()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(",");
        let expected_trace = expected_trace
            .split(',')
            .filter(|s| !s.starts_with("schedule:") && *s != "neighbors")
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(trace, expected_trace, "flags={flags}");
        let chunk = level.chunk(ChunkPos::new(0, 0)).unwrap();
        let heights = heightmap::Heightmap::ALL
            .into_iter()
            .map(|map| (chunk.height_at(map, 8, 8).unwrap() - 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(heights, expected_heights, "flags={flags}");
        assert!(
            !level
                .set_block(pos, Block::SAND.default_state(), UpdateFlags(flags), 512)
                .unwrap()
        );
    }
}
#[test]
fn block_broadcasts_deduplicate_and_capture_final_states_at_broadcast_time() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    use ferraloom_protocol::play::clientbound::Packet;
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let chunk = ChunkPos::new(0, 0);
    level.set_chunk_block_ticking(chunk, true).unwrap();
    let a = BlockPos::new(2, 0, 2);
    let b = BlockPos::new(3, 0, 2);
    let flags = UpdateFlags::CLIENTS | UpdateFlags::KNOWN_SHAPE;
    level
        .set_block(a, Block::STONE.default_state(), flags, UPDATE_LIMIT)
        .unwrap();
    level
        .set_block(a, Block::DIRT.default_state(), flags, UPDATE_LIMIT)
        .unwrap();
    let packets = level.take_block_updates(chunk).unwrap();
    assert!(
        matches!(packets.as_slice(),[Packet::BlockUpdate(packet)] if packet.block_state.0 == i32::from(Block::DIRT.default_state().id()))
    );
    level
        .set_block(a, Block::STONE.default_state(), flags, UPDATE_LIMIT)
        .unwrap();
    level
        .set_block(b, Block::DIRT.default_state(), flags, UPDATE_LIMIT)
        .unwrap();
    let packets = level.take_block_updates(chunk).unwrap();
    assert!(
        matches!(packets.as_slice(),[Packet::SectionBlocksUpdate(packet)] if packet.changes.len()==2)
    );
    assert!(level.take_block_updates(chunk).unwrap().is_empty());
}
struct EventChain;
impl BlockBehavior for EventChain {
    fn trigger_event(
        &self,
        _state: BlockState,
        pos: BlockPos,
        a: i32,
        _b: i32,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        if a == 0 {
            let event = events::BlockEvent {
                pos,
                block: Block::STONE,
                param_a: 1,
                param_b: 0,
            };
            level.block_event(event);
            level.block_event(event);
        }
        Ok(true)
    }
}
static EVENT_BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &EventChain);
#[test]
fn block_events_drain_reentrant_additions_deduplicate_and_retry_non_ticking_chunks() {
    let mut level = empty_level(&EVENT_BEHAVIORS);
    let pos = BlockPos::new(4, 0, 4);
    level
        .set_block_state(pos, Block::STONE.default_state())
        .unwrap();
    let event = events::BlockEvent {
        pos,
        block: Block::STONE,
        param_a: 0,
        param_b: 0,
    };
    level.block_event(event);
    level.block_event(event);
    level.run_block_events().unwrap();
    assert!(level.take_effects().is_empty());
    level
        .set_chunk_block_ticking(ChunkPos::new(0, 0), true)
        .unwrap();
    level.run_block_events().unwrap();
    let events: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|e| match e {
            LevelEffect::BlockEvent(e) => Some(e.param_a),
            _ => None,
        })
        .collect();
    assert_eq!(events, [0, 1]);
    level.block_event(event);
    level
        .set_block_state(pos, Block::DIRT.default_state())
        .unwrap();
    level.run_block_events().unwrap();
    assert!(level.take_effects().is_empty());
}
#[test]
fn foundation_survival_uses_runtime_tag_membership() {
    use crate::block::LevelReaderRef;
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let pos = BlockPos::new(3, 0, 3);
    let state = Block::DANDELION.default_state();
    level
        .set_block_state(pos.below(1), Block::DIRT.default_state())
        .unwrap();
    assert!(
        level
            .block_behavior(state.block())
            .unwrap()
            .can_survive(state, pos, &LevelReaderRef::Server(&level))
            .unwrap()
    );
    level
        .set_block_state(pos.below(1), Block::STONE.default_state())
        .unwrap();
    assert!(
        !level
            .block_behavior(state.block())
            .unwrap()
            .can_survive(state, pos, &LevelReaderRef::Server(&level))
            .unwrap()
    );
}

fn flat_pool() -> Level {
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    for x in -16..32 {
        for z in -16..32 {
            level
                .set_block_state(BlockPos::new(x, -1, z), Block::STONE.default_state())
                .unwrap();
        }
    }
    for x in -1..=1 {
        for z in -1..=1 {
            level
                .set_chunk_block_ticking(ChunkPos::new(x, z), true)
                .unwrap();
        }
    }
    level.rules_mut().spread_fire = false;
    level
}
#[test]
fn water_spread_uses_scheduled_delay_and_crosses_a_chunk_boundary() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    let mut level = flat_pool();
    let source = BlockPos::new(15, 0, 8);
    level
        .set_block(
            source,
            Block::WATER.default_state(),
            UpdateFlags::ALL,
            UPDATE_LIMIT,
        )
        .unwrap();
    for time in 1..5 {
        level.tick_fluid_phase(time, 65536).unwrap();
    }
    assert!(level.get_block_state(source.east(1)).unwrap().is_air());
    level.tick_fluid_phase(5, 65536).unwrap();
    for direction in ferraloom_util::core::Plane::Horizontal {
        let fluid = level
            .get_block_state(source.relative(direction, 1))
            .unwrap()
            .data()
            .fluid;
        assert_eq!(fluid.amount, 7);
        assert!(!fluid.source);
        assert!(!fluid.falling);
    }
    level.tick_fluid_phase(10, 65536).unwrap();
    assert_eq!(
        level
            .get_block_state(source.east(2))
            .unwrap()
            .data()
            .fluid
            .amount,
        6
    );
}
#[test]
fn water_source_conversion_requires_support_and_its_rule_and_above_makes_falling_fluid() {
    use crate::block::fluid::FlowingFluid;
    let mut level = flat_pool();
    let pos = BlockPos::new(8, 0, 8);
    for direction in [Direction::West, Direction::East] {
        level
            .set_block_state(pos.relative(direction, 1), Block::WATER.default_state())
            .unwrap();
    }
    let state = level.get_block_state(pos).unwrap();
    assert!(
        FlowingFluid::Water
            .get_new_liquid(&level, pos, state)
            .unwrap()
            .source
    );
    level.rules_mut().water_source_conversion = false;
    assert_eq!(
        FlowingFluid::Water
            .get_new_liquid(&level, pos, state)
            .unwrap()
            .amount,
        7
    );
    level
        .set_block_state(pos.above(1), Block::WATER.default_state())
        .unwrap();
    let falling = FlowingFluid::Water
        .get_new_liquid(&level, pos, state)
        .unwrap();
    assert!(falling.falling);
    assert_eq!(falling.amount, 8);
}
#[test]
fn liquid_block_mixes_lava_into_obsidian_cobblestone_and_basalt() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    for (source, expected) in [(true, Block::OBSIDIAN), (false, Block::COBBLESTONE)] {
        let mut level = flat_pool();
        let pos = BlockPos::new(8, 0, 8);
        level
            .set_block_state(pos.east(1), Block::WATER.default_state())
            .unwrap();
        let state = if source {
            Block::LAVA.default_state()
        } else {
            Block::LAVA
                .default_state()
                .with(ferraloom_data::properties::LEVEL, 3)
                .unwrap()
        };
        level
            .set_block(pos, state, UpdateFlags::ALL, UPDATE_LIMIT)
            .unwrap();
        assert_eq!(level.get_block_state(pos).unwrap().block(), expected);
    }
    let mut level = flat_pool();
    let pos = BlockPos::new(8, 0, 8);
    level
        .set_block_state(pos.below(1), Block::SOUL_SOIL.default_state())
        .unwrap();
    level
        .set_block_state(pos.east(1), Block::BLUE_ICE.default_state())
        .unwrap();
    level
        .set_block(
            pos,
            Block::LAVA.default_state(),
            UpdateFlags::ALL,
            UPDATE_LIMIT,
        )
        .unwrap();
    assert_eq!(level.get_block_state(pos).unwrap().block(), Block::BASALT);
}
#[test]
fn falling_sand_removes_its_block_before_the_entity_insertion_hook() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let pos = BlockPos::new(8, 0, 8);
    level
        .set_chunk_block_ticking(ChunkPos::new(0, 0), true)
        .unwrap();
    level
        .set_block(
            pos,
            Block::SAND.default_state(),
            UpdateFlags::ALL,
            UPDATE_LIMIT,
        )
        .unwrap();
    level.take_effects();
    level.tick_block_phase(1, 65536).unwrap();
    assert_eq!(level.get_block_state(pos).unwrap().block(), Block::SAND);
    level.tick_block_phase(2, 65536).unwrap();
    assert!(level.get_block_state(pos).unwrap().is_air());
    let effects = level.take_effects();
    assert!(matches!(
        effects.last(),
        Some(LevelEffect::FallingBlock { .. })
    ));
}
#[test]
fn block_and_fluid_ticks_survive_unload_nbt_and_reload_with_their_remaining_delay() {
    let mut level = empty_level(&EMPTY);
    let chunk = ChunkPos::new(0, 0);
    let pos = BlockPos::new(8, 0, 8);
    level.set_game_time(10);
    level
        .schedule_block_tick_after(pos, Block::SAND, 7)
        .unwrap();
    level
        .schedule_fluid_tick_after(pos, ferraloom_data::registries::fluid::Id::WATER, 3)
        .unwrap();
    let mut data = ferraloom_storage::SerializableChunkData::default();
    level.save_chunk_ticks(chunk, &mut data).unwrap();
    assert_eq!(data.block_ticks.as_ref().unwrap()[0].delay, Some(7));
    let old = level.remove_chunk(chunk).unwrap();
    let mut loaded = chunk::LevelChunk::new(chunk, -1..1, old.sections().next().unwrap().clone());
    loaded.load_saved_ticks(&data).unwrap();
    level.set_game_time(100);
    level.add_chunk(loaded).unwrap();
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level.save_chunk_ticks(chunk, &mut saved).unwrap();
    assert_eq!(saved.block_ticks.unwrap()[0].delay, Some(7));
    assert_eq!(saved.fluid_ticks.unwrap()[0].delay, Some(3));
}
#[test]
fn shape_updates_skip_wire_and_apply_drop_suppression_at_the_shape_call() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let pos = BlockPos::new(8, 0, 8);
    let neighbor = pos.below(1);
    level
        .set_block_state(pos, Block::REDSTONE_WIRE.default_state())
        .unwrap();
    level
        .neighbor_shape_changed(
            Direction::Down,
            pos,
            neighbor,
            Block::STONE.default_state(),
            UpdateFlags::SKIP_SHAPE_UPDATE_ON_WIRE,
            UPDATE_LIMIT,
        )
        .unwrap();
    assert!(matches!(
        level.neighbor_shape_changed(
            Direction::Down,
            pos,
            neighbor,
            Block::STONE.default_state(),
            UpdateFlags::default(),
            UPDATE_LIMIT
        ),
        Err(LevelError::UnsupportedBlock(Block::REDSTONE_WIRE))
    ));
    level
        .set_block_state(pos, Block::DANDELION.default_state())
        .unwrap();
    level
        .neighbor_shape_changed(
            Direction::Down,
            pos,
            neighbor,
            Block::AIR.default_state(),
            UpdateFlags::SUPPRESS_DROPS,
            UPDATE_LIMIT,
        )
        .unwrap();
    assert!(level.get_block_state(pos).unwrap().is_air());
    assert!(
        !level
            .take_effects()
            .iter()
            .any(|e| matches!(e, LevelEffect::DropResources { .. }))
    );
}

#[test]
fn vanilla_simple_waterlogged_chain_places_source_ticks_and_picks_up_water() {
    use crate::block::{LevelAccessRef, UPDATE_LIMIT, UpdateFlags, fluid::FlowingFluid};
    use ferraloom_data::{Item, properties, registries::fluid::Id};
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let pos = BlockPos::new(8, 0, 8);
    level
        .set_chunk_block_ticking(ChunkPos::new(0, 0), true)
        .unwrap();
    let dry = Block::IRON_CHAIN.default_state();
    level
        .set_block(
            pos,
            dry,
            UpdateFlags::KNOWN_SHAPE | UpdateFlags::SKIP_ON_PLACE,
            UPDATE_LIMIT,
        )
        .unwrap();
    let behavior = level.block_behavior(dry.block()).unwrap();
    assert!(
        !behavior
            .place_liquid(
                dry,
                pos,
                FlowingFluid::Water.flowing(7, false),
                &mut LevelAccessRef::Server(&mut level)
            )
            .unwrap()
    );
    level.take_effects();
    let placed = behavior
        .place_liquid(
            dry,
            pos,
            FlowingFluid::Water.source(),
            &mut LevelAccessRef::Server(&mut level),
        )
        .unwrap();
    let wet = level.get_block_state(pos).unwrap();
    assert_eq!(wet.get(properties::WATERLOGGED), Some(true));
    assert!(level.has_scheduled_fluid_tick(pos, Id::WATER));
    let bucket = behavior
        .pickup_block(wet, pos, &mut LevelAccessRef::Server(&mut level))
        .unwrap();
    assert_eq!(bucket, Some(Item::WATER_BUCKET));
    assert_eq!(
        level
            .get_block_state(pos)
            .unwrap()
            .get(properties::WATERLOGGED),
        Some(false)
    );
    let expected = vector("simple-waterlogged");
    assert!(expected.starts_with(&format!("{placed}:true:minecraft:water_bucket:")));
    let effects: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|e| match e {
            LevelEffect::BlockUpdated { .. } => Some("send"),
            LevelEffect::PoiChanged { .. } => Some("poi"),
            _ => None,
        })
        .collect();
    assert_eq!(effects, ["send", "poi", "send", "poi"]);
    level.tick_fluid_phase(5, 65536).unwrap();
    assert!(!level.has_scheduled_fluid_tick(pos, Id::WATER));
}
#[test]
fn vanilla_liquid_legacy_state_canonicalizes_falling_levels() {
    for line in include_str!("../../tests/java/level/fluids.tsv").lines() {
        let (input, expected) = line.split_once('\t').unwrap();
        let (block, level) = input
            .strip_prefix("fluid-legacy:")
            .unwrap()
            .rsplit_once(':')
            .unwrap();
        let state = Block::from_name(block)
            .unwrap()
            .default_state()
            .with(ferraloom_data::properties::LEVEL, level.parse().unwrap())
            .unwrap();
        assert_eq!(
            crate::block::fluid::legacy_block(state.data().fluid).id(),
            expected.parse::<u16>().unwrap(),
            "{input}"
        );
    }
}

#[test]
fn strict_is_default_and_warn_records_each_skipped_call_without_query_fallbacks() {
    use crate::block::{BlockMethod, UnportedBehavior};
    let mut level = empty_level(&EMPTY);
    let pos = BlockPos::new(0, 0, 0);
    let state = Block::OAK_LEAVES.default_state();
    assert_eq!(level.unported_behavior(), UnportedBehavior::Strict);
    assert_eq!(
        level.optional_block_call(state.block(), BlockMethod::RandomTick, |behavior, level| {
            behavior.random_tick(state, pos, level)
        }),
        Err(LevelError::UnsupportedBlock(state.block()))
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    for _ in 0..2 {
        assert_eq!(
            level.optional_block_call(state.block(), BlockMethod::RandomTick, |behavior, level| {
                behavior.random_tick(state, pos, level)
            }),
            Ok(None)
        );
    }
    // No fabricated state is returned for an unported shape update.
    assert_eq!(
        level.optional_block_call(
            state.block(),
            BlockMethod::UpdateShape,
            |behavior, level| behavior.update_shape(
                state,
                pos,
                Direction::Down,
                pos.below(1),
                Block::STONE.default_state(),
                &mut crate::block::LevelAccessRef::Server(level)
            )
        ),
        Ok(None)
    );
    assert_eq!(
        level.unported_behavior_stats(),
        crate::block::UnportedBehaviorStats {
            skipped_calls: 3,
            distinct_calls: 2
        }
    );
    assert!(matches!(
        level.required_block_behavior(state.block(), BlockMethod::CanSurvive),
        Err(LevelError::UnsupportedBlockMethod(
            _,
            BlockMethod::CanSurvive
        ))
    ));
    let missing = LevelError::MissingChunk(ChunkPos::new(20, 20));
    assert_eq!(
        level.optional_call(state.block(), BlockMethod::Tick, |_| Err::<(), _>(missing)),
        Err(missing)
    );
    let required = LevelError::UnsupportedBlockMethod(state.block(), BlockMethod::CanSurvive);
    assert_eq!(
        level.optional_call(state.block(), BlockMethod::Tick, |_| Err::<(), _>(required)),
        Err(required)
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 3);
}

#[test]
fn falling_sand_stays_on_a_waterlogged_solid_block() {
    let mut level = flat_pool();
    let pos = BlockPos::new(8, 0, 8);
    let wet_slab = Block::OAK_SLAB
        .default_state()
        .with(ferraloom_data::properties::WATERLOGGED, true)
        .unwrap();
    level.set_block_state(pos.below(1), wet_slab).unwrap();
    level
        .set_block_state(pos, Block::SAND.default_state())
        .unwrap();
    level
        .block_behavior(Block::SAND)
        .unwrap()
        .tick(Block::SAND.default_state(), pos, &mut level)
        .unwrap();
    assert_eq!(
        level.get_block_state(pos).unwrap(),
        Block::SAND.default_state()
    );
    assert!(level.take_effects().is_empty());
}
