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
        source: Block,
        _orientation: Option<redstone::Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.emit(LevelEffect::LevelEvent {
            pos,
            id: 99,
            data: i32::from(source.protocol_id()),
        });
        if self.nested && pos == BlockPos::new(-1, 0, 0) {
            level.neighbor_changed(BlockPos::new(10, 0, 0), Block::STONE, None)?;
            level.neighbor_changed(BlockPos::new(11, 0, 0), Block::STONE, None)?;
        }
        Ok(())
    }
}
static EMPTY: Behaviors = Behaviors::EMPTY;

/// Explicit missing virtuals for required-query and policy tests. Real family
/// ports must never change which operation this fixture leaves unavailable.
struct PendingBlockQueries;
impl BlockBehavior for PendingBlockQueries {
    fn update_shape(
        &self,
        state: BlockState,
        _: BlockPos,
        _: Direction,
        _: BlockPos,
        _: BlockState,
        _: &mut crate::block::LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::UpdateShape,
        ))
    }
    fn random_tick(&self, state: BlockState, _: BlockPos, _: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::RandomTick,
        ))
    }
    fn can_survive(
        &self,
        state: BlockState,
        _: BlockPos,
        _: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::CanSurvive,
        ))
    }
    fn collision_shape(
        &self,
        state: BlockState,
        _: BlockPos,
        _: &crate::block::BlockCollisionContext<'_>,
        _: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::CollisionShape,
        ))
    }
    fn entity_inside_collision_shape(
        &self,
        state: BlockState,
        _: BlockPos,
        _: crate::entity::EntityRef,
        _: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::EntityInsideCollisionShape,
        ))
    }
    fn can_be_replaced(
        &self,
        state: BlockState,
        _: &crate::block::BlockPlaceContext<'_>,
        _: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::CanBeReplaced,
        ))
    }
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _: BlockPos,
        _: bool,
        _: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_protocol::component::ItemStack, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            crate::block::BlockMethod::CloneItemStack,
        ))
    }
}

#[test]
fn block_drops_requires_the_published_rule_and_observes_each_change() {
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    for policy in [
        crate::block::UnportedBehavior::Strict,
        crate::block::UnportedBehavior::Warn,
    ] {
        level.set_unported_behavior(policy);
        assert_eq!(
            level.block_drops_enabled(),
            Err(LevelError::UnsupportedGameRule("minecraft:block_drops"))
        );
    }
    for enabled in [true, false, true] {
        level.rules_mut().block_drops = Some(enabled);
        assert_eq!(level.block_drops_enabled(), Ok(enabled));
    }
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}
static PLAIN: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &Trace { nested: false });
static NESTED: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &Trace { nested: true });
#[test]
fn inactive_entity_phase_stops_after_300_running_ticks_and_active_tickets_reset_it_during_freeze() {
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    for _ in 1..300 {
        assert!(level.update_empty_time(false, true));
    }
    assert!(!level.update_empty_time(false, true));
    assert!(!level.update_empty_time(false, false));
    assert!(level.update_empty_time(true, false));
    for _ in 0..400 {
        assert!(level.update_empty_time(false, false));
    }
    assert!(level.update_empty_time(true, true));
}
fn empty_chunk(pos: ChunkPos) -> chunk::LevelChunk {
    chunk::LevelChunk::new(
        pos,
        -1..1,
        ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
            )),
            ferraloom_chunk::BiomeId(0),
        ),
    )
}

#[test]
fn shared_loading_status_changes_are_visible_to_an_existing_chunk_reader() {
    use distance_manager::FullChunkStatus;
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    let pos = ChunkPos::ZERO;
    level.add_chunk(empty_chunk(pos)).unwrap();
    let reader = &level;
    let chunk = reader.chunk(pos).unwrap();
    for (requested, ready) in [
        (FullChunkStatus::Full, false),
        (FullChunkStatus::EntityTicking, false),
        (FullChunkStatus::EntityTicking, true),
        (FullChunkStatus::Full, false),
    ] {
        reader
            .set_chunk_loading_status(pos, requested, ready)
            .unwrap();
        assert_eq!(chunk.full_status(), requested);
        assert_eq!(chunk.ticking_ready(), ready);
        assert!(!chunk.is_unsaved());
    }
    let absent = ChunkPos::new(1, 0);
    assert_eq!(
        reader.set_chunk_loading_status(absent, FullChunkStatus::Full, false),
        Err(LevelError::MissingChunk(absent))
    );
}

fn empty_level(behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(
        Region::new(behaviors),
        crate::level::LevelContext::fixture(0, 63),
    );
    // Controlled test dimension: no timelines and native constant attributes.
    let resources = ferraloom_data::datapack::ResourceManager::new(vec![]);
    let environment = Arc::new(environment::EnvironmentDefinitions::load(&resources).unwrap());
    let dimension = environment.dimension(&serde_json::json!({})).unwrap();
    level
        .set_environment_attributes(
            environment,
            &dimension,
            "test".parse().unwrap(),
            &[],
            environment::ClockSnapshot::default(),
        )
        .unwrap();
    for x in -1..=1 {
        for z in -1..=1 {
            let pos = ChunkPos::new(x, z);
            level.add_chunk(empty_chunk(pos)).unwrap();
            level
                .set_chunk_loading_status(
                    pos,
                    distance_manager::FullChunkStatus::EntityTicking,
                    true,
                )
                .unwrap();
            level
                .update_entity_chunk_status(pos, entity::Visibility::Ticking)
                .unwrap();
        }
    }
    for request in level.take_entity_storage_requests() {
        let entity::EntityStorageRequest::Load { id, .. } = request else {
            panic!("new empty fixture only requests entity loads")
        };
        level.complete_entity_chunk_load(id, Ok(None)).unwrap();
    }
    level.tick_entity_management().unwrap();
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
    let values = include_str!("../../tests/java/level/fluid-tags.tsv")
        .lines()
        .map(|line| {
            let (name, fluids) = line.split_once('\t').unwrap();
            (
                name.strip_prefix("fluid-tag:").unwrap().parse().unwrap(),
                fluids
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|name| ferraloom_data::registries::fluid::Id::from_name(name).unwrap())
                    .collect(),
            )
        })
        .collect();
    let tags = ferraloom_data::datapack::TagSet::from_tags(
        ferraloom_data::datapack::Tags {
            values,
            diagnostics: Vec::new(),
        },
        ferraloom_data::registries::fluid::ALL.len(),
    )
    .unwrap();
    level.set_fluid_tags(Arc::new(tags));
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
#[test]
fn client_updates_use_loading_status_and_ticking_future_independently_of_simulation() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    use distance_manager::FullChunkStatus;
    for line in include_str!("../../tests/java/level/client-updates.tsv").lines() {
        let (input, expected) = line.split_once('\t').unwrap();
        let mut parts = input.split(':').skip(1);
        let status = match parts.next().unwrap() {
            "FULL" => FullChunkStatus::Full,
            "BLOCK_TICKING" => FullChunkStatus::BlockTicking,
            _ => panic!("unexpected vanilla full status"),
        };
        let ready = parts.next().unwrap().parse::<bool>().unwrap();
        let mut level = empty_level(&EMPTY);
        install_tags(&mut level);
        let chunk = ChunkPos::new(0, 0);
        let pos = BlockPos::new(8, 0, 8);
        assert!(!level.should_tick_blocks_at(pos));
        level
            .set_chunk_loading_status(chunk, status, ready)
            .unwrap();
        level
            .set_block(
                pos,
                Block::STONE.default_state(),
                UpdateFlags::CLIENTS | UpdateFlags::KNOWN_SHAPE,
                UPDATE_LIMIT,
            )
            .unwrap();
        let navigation = level
            .take_effects()
            .iter()
            .any(|effect| matches!(effect, LevelEffect::BlockUpdated { .. }));
        let broadcast = !level.take_block_updates(chunk).unwrap().is_empty();
        assert_eq!(format!("{navigation}:{broadcast}"), expected, "{input}");
        // A subsequent future completion cannot retroactively queue a change
        // made while the holder lacked a ticking chunk.
        level
            .set_chunk_loading_status(chunk, FullChunkStatus::BlockTicking, true)
            .unwrap();
        assert!(level.take_block_updates(chunk).unwrap().is_empty());
    }
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
fn block_event_audiences_capture_before_later_event_movement() {
    use crate::entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_protocol::play::clientbound::Packet;
    use ferraloom_util::{Uuid, Vec3};
    struct MovingEvent;
    impl BlockBehavior for MovingEvent {
        fn trigger_event(
            &self,
            _: BlockState,
            _: BlockPos,
            a: i32,
            _: i32,
            level: &mut Level,
        ) -> Result<bool, LevelError> {
            if a == 1 {
                level.set_entity_pos(EntityRef(1000), Vec3::new(100.0, 0.0, 4.0))?;
            }
            Ok(true)
        }
    }
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &MovingEvent);
    let mut level = empty_level(&BEHAVIORS);
    let p = BlockPos::new(4, 0, 4);
    level
        .set_block_state(p, Block::STONE.default_state())
        .unwrap();
    level.set_chunk_block_ticking(ChunkPos::ZERO, true).unwrap();
    let player = EntityRef(1000);
    let network_id = level.next_entity_id();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::offline_player("EventAudience"),
                "EventAudience".into(),
                network_id,
                Vec3::new(8.0, 0.0, 4.0),
                GameType::Survival,
            ))),
        )
        .unwrap();
    level.take_effects();
    for a in [0, 1] {
        level.block_event(events::BlockEvent {
            pos: p,
            block: Block::STONE,
            param_a: a,
            param_b: 0,
        });
    }
    level.run_block_events().unwrap();
    let effects = level.take_effects();
    assert!(
        matches!(&effects[0], LevelEffect::PlayerPacket { recipients,packet }
        if recipients == &[player] && matches!(packet.as_ref(), Packet::BlockEvent(event) if event.b0 == 0))
    );
    assert!(
        matches!(&effects[1], LevelEffect::PlayerSectionChange { player:id,.. } if *id == player)
    );
    assert!(
        matches!(&effects[2], LevelEffect::PlayerPacket { recipients,packet }
        if recipients.is_empty() && matches!(packet.as_ref(), Packet::BlockEvent(event) if event.b0 == 1))
    );
    assert_eq!(effects.len(), 3);
}
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
            LevelEffect::PlayerPacket { packet, .. } => match *packet {
                ferraloom_protocol::play::clientbound::Packet::BlockEvent(event) => Some(event.b0),
                _ => None,
            },
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
fn scheduled_phases_wait_for_entity_load_integration_simulation_and_ticking_future() {
    use ferraloom_data::registries::fluid::Id as Fluid;
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let chunk = ChunkPos::ZERO;
    let pos = BlockPos::new(8, 0, 8);
    level.add_chunk(empty_chunk(chunk)).unwrap();
    level
        .set_block_state(pos, Block::STONE.default_state())
        .unwrap();
    level.set_chunk_block_ticking(chunk, true).unwrap();
    level
        .set_chunk_loading_status(chunk, distance_manager::FullChunkStatus::BlockTicking, true)
        .unwrap();
    level
        .schedule_block_tick_after(pos, Block::STONE, 0)
        .unwrap();
    level
        .schedule_fluid_tick_after(pos, Fluid::WATER, 0)
        .unwrap();
    let check = |level: &mut Level, name: &str| {
        level.tick_block_phase(0, 65536).unwrap();
        level.tick_fluid_phase(0, 65536).unwrap();
        let key = format!("scheduled-readiness:{name}");
        let expected = include_str!("../../tests/java/level/scheduled-readiness.tsv")
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once('\t')?;
                (name == key).then_some(value.parse::<bool>().unwrap())
            })
            .unwrap();
        assert_eq!(
            level.has_scheduled_block_tick(pos, Block::STONE),
            expected,
            "{name}"
        );
        assert_eq!(
            level.has_scheduled_fluid_tick(pos, Fluid::WATER),
            expected,
            "{name}"
        );
    };
    check(&mut level, "fresh");
    level
        .update_entity_chunk_status(chunk, entity::Visibility::Tracked)
        .unwrap();
    check(&mut level, "pending");
    let entity::EntityStorageRequest::Load { id, .. } =
        level.take_entity_storage_requests().pop().unwrap()
    else {
        panic!("the fixture requests its entity chunk")
    };
    level.complete_entity_chunk_load(id, Ok(None)).unwrap();
    check(&mut level, "completed-not-integrated");
    level.tick_entity_management().unwrap();
    level.set_chunk_block_ticking(chunk, false).unwrap();
    check(&mut level, "outside-simulation");
    level.set_chunk_block_ticking(chunk, true).unwrap();
    level
        .set_chunk_loading_status(
            chunk,
            distance_manager::FullChunkStatus::BlockTicking,
            false,
        )
        .unwrap();
    check(&mut level, "ticking-not-ready");
    level
        .set_chunk_loading_status(chunk, distance_manager::FullChunkStatus::BlockTicking, true)
        .unwrap();
    check(&mut level, "ready");
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
fn scheduled_falling_sand_creates_the_entity_and_replaces_its_source_synchronously() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    level
        .update_entity_chunk_status(ChunkPos::new(0, 0), entity::Visibility::Ticking)
        .unwrap();
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
    let entities = level.all_entities();
    assert_eq!(entities.len(), 1);
    let falling = level
        .entity::<crate::entity::item::falling_block::FallingBlockEntity>(entities[0])
        .unwrap();
    assert_eq!(falling.block_state, Block::SAND.default_state());
    assert_eq!(falling.start_pos(), pos);
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
    level.set_game_time(200);
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level.save_chunk_ticks(chunk, &mut saved).unwrap();
    assert_eq!(saved.block_ticks.unwrap()[0].delay, Some(7));
    assert_eq!(saved.fluid_ticks.unwrap()[0].delay, Some(3));
    level.start_ticking_chunk(chunk).unwrap();
    assert_eq!(
        level
            .region()
            .block_ticks
            .container(chunk)
            .unwrap()
            .peek()
            .unwrap()
            .trigger_tick,
        207
    );
    assert_eq!(
        level
            .region()
            .fluid_ticks
            .container(chunk)
            .unwrap()
            .peek()
            .unwrap()
            .trigger_tick,
        203
    );
    level.set_game_time(300);
    level.start_ticking_chunk(chunk).unwrap();
    assert_eq!(
        level
            .region()
            .block_ticks
            .container(chunk)
            .unwrap()
            .peek()
            .unwrap()
            .trigger_tick,
        207
    );
}
#[test]
fn shape_updates_skip_wire_and_apply_drop_suppression_at_the_shape_call() {
    use crate::block::{UPDATE_LIMIT, UpdateFlags};
    static PENDING: Behaviors =
        Behaviors::EMPTY.with_block(Block::REDSTONE_WIRE, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
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
        Err(LevelError::UnsupportedBlockMethod(
            Block::REDSTONE_WIRE,
            crate::block::BlockMethod::UpdateShape
        ))
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
    assert!(level.all_entities().is_empty());
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
    struct PendingPolicyFixture;
    impl BlockBehavior for PendingPolicyFixture {
        fn random_tick(
            &self,
            state: BlockState,
            _: BlockPos,
            _: &mut Level,
        ) -> Result<(), LevelError> {
            Err(LevelError::UnsupportedBlock(state.block()))
        }
        fn update_shape(
            &self,
            state: BlockState,
            _: BlockPos,
            _: Direction,
            _: BlockPos,
            _: BlockState,
            _: &mut crate::block::LevelAccessRef<'_, '_>,
        ) -> Result<BlockState, LevelError> {
            Err(LevelError::UnsupportedBlockMethod(
                state.block(),
                BlockMethod::UpdateShape,
            ))
        }
        fn can_survive(
            &self,
            state: BlockState,
            _: BlockPos,
            _: &crate::block::LevelReaderRef<'_, '_>,
        ) -> Result<bool, LevelError> {
            Err(LevelError::UnsupportedBlockMethod(
                state.block(),
                BlockMethod::CanSurvive,
            ))
        }
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingPolicyFixture);
    let mut level = empty_level(&PENDING);
    let pos = BlockPos::new(0, 0, 0);
    let state = Block::STONE.default_state();
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
        level.can_survive(state, pos),
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

#[test]
fn random_phase_uses_registered_callbacks_and_warn_counts_block_and_fluid_skips() {
    use crate::block::{BlockMethod, UnportedBehavior};
    // Leaves supply native per-state random eligibility; their behavior here is
    // deliberately synthetic, so newly ported decay cannot alter this test.
    static PENDING: Behaviors =
        Behaviors::EMPTY.with_block(Block::OAK_LEAVES, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
    let chunk = ChunkPos::new(0, 0);
    let state = Block::OAK_LEAVES.default_state();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                level
                    .set_block_state(BlockPos::new(x, y, z), state)
                    .unwrap();
            }
        }
    }
    install_weather_environment(&mut level, "plains");
    assert_eq!(
        level.tick_random_chunk(chunk),
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::RandomTick
        ))
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    level.tick_random_chunk(chunk).unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 3);
    // Exercise the distinct fluid callback policy without requiring a native
    // fluid method to remain unported.
    for _ in 0..3 {
        level
            .optional_call(state.block(), BlockMethod::FluidRandomTick, |_| {
                Err::<(), _>(LevelError::UnsupportedFluidRandomTick)
            })
            .unwrap();
    }
    assert_eq!(
        level.unported_behavior_stats(),
        crate::block::UnportedBehaviorStats {
            skipped_calls: 6,
            distinct_calls: 2
        }
    );
}

#[test]
fn water_slope_search_matches_real_vanilla_across_chunk_borders_and_at_its_limit() {
    use crate::block::fluid::FlowingFluid;
    for line in include_str!("../../tests/java/level/slopes.tsv").lines() {
        let (hole, expected) = line.split_once('\t').unwrap();
        let distance = hole
            .strip_prefix("water-slope:")
            .unwrap()
            .parse::<i32>()
            .unwrap();
        let mut level = flat_pool();
        let origin = BlockPos::new(15, 0, 8);
        level
            .set_block_state(origin.east(distance).below(1), Block::AIR.default_state())
            .unwrap();
        assert_eq!(
            FlowingFluid::Water
                .slope_distance(
                    &level,
                    origin.east(1),
                    1,
                    Direction::West,
                    Block::AIR.default_state()
                )
                .unwrap(),
            expected.parse::<i32>().unwrap(),
            "{hole}"
        );
    }
}

#[test]
fn fluid_wall_occlusion_matches_vanilla_native_partial_faces_in_all_directions() {
    let level = empty_level(&EMPTY);
    for line in include_str!("../../tests/java/level/walls.tsv")
        .lines()
        .filter(|line| line.starts_with("fluid-wall:"))
    {
        let (input, expected) = line.split_once('\t').unwrap();
        let mut ids = input
            .strip_prefix("fluid-wall:")
            .unwrap()
            .split(':')
            .map(|id| id.parse::<u16>().unwrap());
        let source = BlockState::from_id(ids.next().unwrap()).unwrap();
        let target = BlockState::from_id(ids.next().unwrap()).unwrap();
        let direction = Direction::VALUES[ids.next().unwrap() as usize];
        assert_eq!(
            crate::block::fluid::can_pass_through_wall(
                direction,
                &level,
                BlockPos::ZERO,
                source,
                BlockPos::ZERO.relative(direction, 1),
                target
            )
            .unwrap(),
            expected.parse::<bool>().unwrap(),
            "{input}"
        );
    }
}

struct DynamicFluidWall;
impl BlockBehavior for DynamicFluidWall {
    fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _context: &crate::block::BlockCollisionContext<'_>,
        _level: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        match pos.x {
            0 => Ok(ferraloom_util::shapes::block()),
            1 => Ok(ferraloom_util::shapes::empty()),
            _ => Err(LevelError::UnsupportedShape(state)),
        }
    }
}
static DYNAMIC_FLUID_WALL: Behaviors = Behaviors::EMPTY
    .with_block(Block::SCAFFOLDING, &DynamicFluidWall)
    .with_block(Block::MOVING_PISTON, &PendingBlockQueries);

#[test]
fn fluid_wall_queries_share_dynamic_dispatch_and_match_vanilla_target_first_shortcuts() {
    let level = empty_level(&DYNAMIC_FLUID_WALL);
    let states = [Block::STONE, Block::WATER, Block::MOVING_PISTON].map(Block::default_state);
    for line in include_str!("../../tests/java/level/walls.tsv")
        .lines()
        .filter(|line| line.starts_with("dynamic-fluid-wall:"))
    {
        let (input, expected) = line.split_once('\t').unwrap();
        let mut positions = input
            .strip_prefix("dynamic-fluid-wall:")
            .unwrap()
            .split(':')
            .map(|x| x.parse::<usize>().unwrap());
        let source = positions.next().unwrap();
        let target = positions.next().unwrap();
        let result = crate::block::fluid::can_pass_through_wall(
            Direction::East,
            &level,
            BlockPos::new(source as i32, 0, 0),
            states[source],
            BlockPos::new(target as i32, 0, 0),
            states[target],
        );
        if expected == "unsupported" {
            assert_eq!(
                result,
                Err(LevelError::UnsupportedBlockMethod(
                    Block::MOVING_PISTON,
                    crate::block::BlockMethod::CollisionShape
                )),
                "{input}"
            );
        } else {
            assert_eq!(result.unwrap().to_string(), expected, "{input}");
        }
    }
    // A state without a native collision cache reaches the registered dynamic
    // method, including successful contextual results.
    let dynamic = Block::SCAFFOLDING.default_state();
    assert!(
        !crate::block::fluid::can_pass_through_wall(
            Direction::East,
            &level,
            BlockPos::ZERO,
            dynamic,
            BlockPos::new(1, 0, 0),
            dynamic,
        )
        .unwrap()
    );
}

static ANALOG: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &Trace { nested: false });
#[test]
fn comparator_notifications_match_vanilla_direct_and_through_conductors() {
    let mut level = empty_level(&ANALOG);
    install_tags(&mut level);
    let center = BlockPos::new(8, 0, 8);
    for (pos, block) in [
        (center.north(1), Block::COMPARATOR),
        (center.east(1), Block::STONE),
        (center.east(2), Block::COMPARATOR),
        (center.south(1), Block::GLASS),
        (center.south(2), Block::COMPARATOR),
        (center.west(1), Block::STONE_SLAB),
        (center.west(2), Block::COMPARATOR),
    ] {
        level.set_block_state(pos, block.default_state()).unwrap();
    }
    level
        .update_neighbour_for_output_signal(center, Block::CAKE)
        .unwrap();
    let expected = vector("analog-neighbors").replace("analog:", "");
    assert_eq!(trace(&mut level), expected);
    level
        .set_block(
            center,
            Block::CAKE.default_state(),
            crate::block::UpdateFlags::NEIGHBORS | crate::block::UpdateFlags::KNOWN_SHAPE,
            512,
        )
        .unwrap();
    let source_ids = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::LevelEvent { pos, id: 99, data } if pos == center.north(1) => Some(data),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Ordinary neighborChanged receives the removed block; the following
    // comparator notification receives the newly placed analog-output block.
    assert_eq!(
        source_ids,
        [
            i32::from(Block::AIR.protocol_id()),
            i32::from(Block::CAKE.protocol_id())
        ]
    );
}

#[test]
fn shared_collision_queries_use_native_faces_and_liquid_context_without_warn_fallbacks() {
    use crate::block::{BlockCollisionContext, LevelReaderRef, UnportedBehavior};
    let level = flat_pool();
    let pos = BlockPos::new(8, 0, 8);
    let reader = LevelReaderRef::Server(&level);
    let slab = Block::STONE_SLAB.default_state();
    assert!(!reader.is_face_sturdy(slab, pos, Direction::Up).unwrap());
    assert!(reader.is_face_sturdy(slab, pos, Direction::Down).unwrap());
    assert_eq!(
        reader
            .collision_shape(slab, pos, &BlockCollisionContext::empty())
            .unwrap()
            .to_aabbs(),
        slab.collision_shape().unwrap().to_aabbs()
    );
    let water = Block::WATER.default_state();
    assert!(
        reader
            .collision_shape(water, pos, &BlockCollisionContext::empty())
            .unwrap()
            .is_empty()
    );
    assert!(
        reader
            .collision_shape(
                water,
                pos,
                &BlockCollisionContext::empty_with_fluid_collisions(true)
            )
            .unwrap()
            .ptr_eq(&ferraloom_util::shapes::block())
    );
    static PENDING: Behaviors =
        Behaviors::EMPTY.with_block(Block::SCAFFOLDING, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
    level.set_unported_behavior(UnportedBehavior::Warn);
    let dynamic = Block::SCAFFOLDING.default_state();
    assert!(matches!(
        LevelReaderRef::Server(&level).collision_shape(
            dynamic,
            pos,
            &BlockCollisionContext::empty()
        ),
        Err(LevelError::UnsupportedBlockMethod(
            _,
            crate::block::BlockMethod::CollisionShape
        ))
    ));
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}

#[test]
fn reflected_method_ownership_ports_inherited_defaults_without_masking_overrides() {
    use crate::block::{
        BlockBehavior, BlockCollisionContext, BlockMethod, LevelReaderRef, UnportedBehavior,
    };
    let level = flat_pool();
    let pos = BlockPos::new(8, 0, 8);
    let reader = LevelReaderRef::Server(&level);
    assert!(
        crate::block::BASE_BLOCK
            .shape(Block::AIR.default_state(), pos, &reader)
            .unwrap()
            .ptr_eq(&ferraloom_util::shapes::block())
    );
    assert!(
        crate::block::BASE_BLOCK
            .collision_shape(
                Block::AIR.default_state(),
                pos,
                &BlockCollisionContext::empty(),
                &reader
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        level
            .required_block_behavior(Block::AIR, BlockMethod::OutlineShape)
            .unwrap()
            .shape(Block::AIR.default_state(), pos, &reader)
            .unwrap()
            .is_empty()
    );
    // Leaves override random/updateShape, but inherit the full-cube inside
    // query and base survival. These are individually proven vanilla methods.
    let leaves = Block::OAK_LEAVES.default_state();
    let behavior = level
        .required_block_behavior(leaves.block(), BlockMethod::CanSurvive)
        .unwrap();
    assert!(
        behavior
            .can_survive(leaves, pos, &LevelReaderRef::Server(&level))
            .unwrap()
    );
    assert!(
        LevelReaderRef::Server(&level)
            .entity_inside_collision_shape(leaves, pos, crate::entity::EntityRef(0))
            .unwrap()
            .ptr_eq(&ferraloom_util::shapes::block())
    );
    for method in [BlockMethod::RandomTick, BlockMethod::UpdateShape] {
        assert!(level.behavior_for_method(leaves.block(), method).is_ok());
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
    level.set_unported_behavior(UnportedBehavior::Warn);
    assert!(matches!(
        LevelReaderRef::Server(&level).entity_inside_collision_shape(
            Block::STONE.default_state(),
            pos,
            crate::entity::EntityRef(0)
        ),
        Err(LevelError::UnsupportedBlockMethod(
            _,
            BlockMethod::EntityInsideCollisionShape
        ))
    ));
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}

fn weather_test_temperatures() -> Arc<ferraloom_worldgen::biome_manager::BiomeTemperatures> {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use ferraloom_worldgen::biome_manager::BiomeTemperatures;
    static TEMPERATURES: std::sync::OnceLock<Arc<BiomeTemperatures>> = std::sync::OnceLock::new();
    TEMPERATURES
        .get_or_init(|| {
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
                .expect("builder supplies the vanilla datapack");
            let resources =
                ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
            Arc::new(BiomeTemperatures::load(&resources).unwrap())
        })
        .clone()
}
fn install_weather_environment(level: &mut Level, biome: &str) {
    use weather::{ChunkLightSnapshot, WeatherEnvironment};
    let temperatures = weather_test_temperatures();
    level.set_has_skylight(true);
    level.set_weather_environment(WeatherEnvironment {
        temperatures: temperatures.clone(),
        biome_names: Arc::new(vec![biome.parse().unwrap()]),
        biome_zoom_seed: 0,
        sea_level: 63,
    });
    install_tags(level);
    for x in -1..=1 {
        for z in -1..=1 {
            let p = ChunkPos::new(x, z);
            level.prime_chunk_heightmaps(p).unwrap();
            level.set_light_snapshot(
                p,
                ChunkLightSnapshot {
                    sky: (-1..=0)
                        .map(|y| (y, ferraloom_light::DataLayer::new(15)))
                        .collect(),
                    block: std::collections::BTreeMap::new(),
                },
            );
        }
    }
}

#[test]
fn precipitation_selection_slots_match_vanilla_random_and_position_streams_when_dry() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    for line in include_str!("../../tests/java/level/precipitation.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let (seed, speed) = case
            .strip_prefix("precipitation-slots:")
            .unwrap()
            .split_once(':')
            .unwrap();
        let mut level = empty_level(&EMPTY);
        install_weather_environment(&mut level, "plains");
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(seed.parse().unwrap())),
            0,
        );
        level.rules_mut().random_tick_speed = speed.parse().unwrap();
        for _ in 0..30 {
            level.tick_random_chunk(ChunkPos::new(0, 0)).unwrap();
        }
        let precipitation = level
            .take_effects()
            .into_iter()
            .map(|effect| match effect {
                LevelEffect::PendingPrecipitation(pos) => {
                    format!("{}, {}, {}", pos.x, pos.y, pos.z)
                }
                other => panic!("unexpected effect {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            level.pending_precipitation_calls(),
            precipitation.len() as u64
        );
        let next_random = level.random().next_int();
        let next_pos = level.get_block_random_pos(BlockPos::ZERO, 15);
        assert_eq!(
            format!(
                "{}|{next_random}|{}, {}, {}",
                precipitation.join(";"),
                next_pos.x,
                next_pos.y,
                next_pos.z
            ),
            expected,
            "{case}"
        );
    }
}

#[test]
fn thunder_gate_and_position_draw_match_vanilla_before_the_pending_required_query() {
    use crate::block::UnportedBehavior;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    for line in include_str!("../../tests/java/level/thunder.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let (seed, weather) = case
            .strip_prefix("thunder-slots:")
            .unwrap()
            .split_once(':')
            .unwrap();
        for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
            let mut level = empty_level(&EMPTY);
            level.set_random_streams(
                Random::Legacy(LegacyRandomSource::new(seed.parse().unwrap())),
                0,
            );
            level.set_unported_behavior(policy);
            level.load_weather(weather::WeatherData {
                raining: weather != "0",
                thundering: weather == "2",
                ..weather::WeatherData::default()
            });
            let strike = expected.starts_with("true|");
            assert_eq!(
                level.tick_thunder(ChunkPos::new(0, 0)),
                if strike && policy == UnportedBehavior::Strict {
                    Err(LevelError::UnsupportedWeatherQuery)
                } else {
                    Ok(())
                }
            );
            let next_random = level.random().next_int();
            let next_pos = level.get_block_random_pos(BlockPos::ZERO, 15);
            assert_eq!(
                format!(
                    "{strike}|{next_random}|{}, {}, {}",
                    next_pos.x, next_pos.y, next_pos.z
                ),
                expected,
                "{case}"
            );
            assert_eq!(
                level.unported_behavior_stats().skipped_calls,
                u64::from(strike && policy == UnportedBehavior::Warn)
            );
        }
    }
}

#[test]
fn whole_effect_skips_share_the_block_policy_and_counters() {
    use crate::block::{BlockMethod, UnportedBehavior, UnportedBehaviorStats, UnportedOperation};
    let mut level = empty_level(&EMPTY);
    assert_eq!(
        level.unported_operation(UnportedOperation::PackReload),
        Err(LevelError::UnsupportedPackReload)
    );
    assert_eq!(
        level.unported_behavior_stats(),
        UnportedBehaviorStats::default()
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    level
        .unported_operation(UnportedOperation::PackReload)
        .unwrap();
    level
        .unported_operation(UnportedOperation::PackReload)
        .unwrap();
    level
        .unported_operation(UnportedOperation::WeatherStrike)
        .unwrap();
    level
        .optional_call(Block::STONE, BlockMethod::RandomTick, |_| {
            Err::<(), _>(LevelError::UnsupportedBlock(Block::STONE))
        })
        .unwrap();
    assert_eq!(
        level.unported_behavior_stats(),
        UnportedBehaviorStats {
            skipped_calls: 4,
            distinct_calls: 3
        }
    );
}

#[test]
fn slab_waterlogging_matches_vanilla_interface_overrides_and_source_identity() {
    use crate::block::{BlockMethod, LevelAccessRef, fluid::FlowingFluid};
    use ferraloom_data::{
        Item,
        properties::{self, Type},
        registries::fluid::Id as Fluid,
    };
    for line in include_str!("../../tests/java/level/slab-waterlogged.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let (slab_type, fluid) = case
            .strip_prefix("slab-waterlogged:")
            .unwrap()
            .split_once(':')
            .unwrap();
        let slab_type = match slab_type {
            "bottom" => Type::Bottom,
            "top" => Type::Top,
            "double" => Type::Double,
            _ => unreachable!(),
        };
        let fluid = Fluid::from_name(fluid).unwrap();
        let fluid_state = if fluid == Fluid::FLOWING_WATER {
            FlowingFluid::Water.flowing(7, false)
        } else {
            FlowingFluid::from_id(fluid).unwrap().source()
        };
        let mut level = flat_pool();
        let pos = BlockPos::new(8, 0, 8);
        let slab = Block::STONE_SLAB
            .default_state()
            .with(properties::TYPE_, slab_type)
            .unwrap();
        level.set_block_state(pos, slab).unwrap();
        let can = level
            .required_block_behavior(slab.block(), BlockMethod::CanPlaceLiquid)
            .unwrap()
            .can_place_liquid(slab, fluid);
        let placed = level
            .required_block_behavior(slab.block(), BlockMethod::PlaceLiquid)
            .unwrap()
            .place_liquid(
                slab,
                pos,
                fluid_state,
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        let wet = level.get_block_state(pos).unwrap();
        assert_eq!(
            level.has_scheduled_fluid_tick(pos, Fluid::WATER),
            placed,
            "{case}"
        );
        let bucket = level
            .required_block_behavior(slab.block(), BlockMethod::PickupBlock)
            .unwrap()
            .pickup_block(wet, pos, &mut LevelAccessRef::Server(&mut level))
            .unwrap()
            .unwrap_or(Item::AIR);
        assert_eq!(
            format!(
                "{can}:{placed}:{}:{}",
                wet.get(properties::WATERLOGGED).unwrap(),
                bucket.name()
            ),
            expected,
            "{case}"
        );
        assert_eq!(
            level
                .get_block_state(pos)
                .unwrap()
                .get(properties::WATERLOGGED),
            Some(false)
        );
    }
}

#[test]
fn clone_item_query_matches_vanilla_base_and_rejects_unported_overrides() {
    use crate::block::{BlockMethod, UnportedBehavior};
    let level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    for line in include_str!("../../tests/java/level/clone-items.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let (block, include_data) = case
            .strip_prefix("clone-item:")
            .unwrap()
            .rsplit_once(':')
            .unwrap();
        let state = Block::from_name(block).unwrap().default_state();
        let stack = level
            .get_clone_item_stack(state, BlockPos::ZERO, include_data.parse().unwrap())
            .unwrap();
        let (item, count) = stack
            .0
            .as_ref()
            .map_or((ferraloom_data::Item::AIR, 0), |stack| {
                (stack.item, stack.count)
            });
        assert_eq!(format!("{}:{count}", item.name()), expected, "{case}");
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
    level.set_unported_behavior(UnportedBehavior::Warn);
    assert_eq!(
        level.get_clone_item_stack(Block::STONE.default_state(), BlockPos::ZERO, true),
        Err(LevelError::UnsupportedBlockMethod(
            Block::STONE,
            BlockMethod::CloneItemStack
        ))
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}

#[test]
fn directional_replacement_matches_vanilla_and_keeps_required_override_errors() {
    use crate::block::{BlockMethod, DirectionalPlaceContext, LevelReaderRef, UnportedBehavior};
    use ferraloom_protocol::component::{DataComponentPatch, ItemStack};
    for line in include_str!("../../tests/java/level/placement.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let (block, held) = case
            .strip_prefix("directional-replacement:")
            .unwrap()
            .rsplit_once(':')
            .unwrap();
        let block = Block::from_name(block).unwrap();
        let item = match held {
            "0" => ferraloom_data::Item::AIR,
            "1" => block.as_item(),
            "2" => ferraloom_data::Item::DIRT,
            _ => unreachable!(),
        };
        let stack = ItemStack::new(item, 1, DataComponentPatch::default());
        let mut level = empty_level(&EMPTY);
        let pos = BlockPos::new(8, 0, 8);
        let state = block.default_state();
        level.set_block_state(pos, state).unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            Direction::Down,
            &stack,
            Direction::Up,
        )
        .unwrap();
        assert_eq!(
            level.can_be_replaced(state, &context).unwrap().to_string(),
            expected,
            "{case}"
        );
        assert!(level.can_survive(Block::SAND.default_state(), pos).unwrap());
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingBlockQueries);
    let mut level = empty_level(&PENDING);
    level.set_unported_behavior(UnportedBehavior::Warn);
    let pos = BlockPos::new(8, 0, 8);
    level
        .set_block_state(pos, Block::STONE.default_state())
        .unwrap();
    let stack = ItemStack::default();
    assert!(matches!(
        DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            Direction::Down,
            &stack,
            Direction::Up
        ),
        Err(LevelError::UnsupportedBlockMethod(
            Block::STONE,
            BlockMethod::CanBeReplaced
        ))
    ));
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}

#[test]
fn broadcast_membership_drains_separately_from_retained_section_changes() {
    use crate::block::UpdateFlags;
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let chunk = ChunkPos::new(0, 0);
    let p = BlockPos::new(8, 0, 8);
    level
        .set_block(
            p,
            Block::STONE.default_state(),
            UpdateFlags::CLIENTS | UpdateFlags::KNOWN_SHAPE,
            512,
        )
        .unwrap();
    assert_eq!(level.take_block_update_chunks(), vec![chunk]);
    level
        .set_chunk_loading_status(chunk, distance_manager::FullChunkStatus::Full, false)
        .unwrap();
    // The server skips this holder after its ticking future is cancelled.
    assert!(level.take_block_update_chunks().is_empty());
    level
        .set_chunk_loading_status(chunk, distance_manager::FullChunkStatus::BlockTicking, true)
        .unwrap();
    assert!(level.take_block_update_chunks().is_empty());
    level
        .set_block(
            p.above(1),
            Block::STONE.default_state(),
            UpdateFlags::CLIENTS | UpdateFlags::KNOWN_SHAPE,
            512,
        )
        .unwrap();
    assert_eq!(level.take_block_update_chunks(), vec![chunk]);
    let packets = level.take_block_updates(chunk).unwrap();
    assert!(
        matches!(&packets[..],[ferraloom_protocol::play::clientbound::Packet::SectionBlocksUpdate(packet)] if packet.changes.len()==2)
    );
}

struct SnowSurvivalFixture;
impl BlockBehavior for SnowSurvivalFixture {
    fn can_survive(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &crate::block::LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        // Every native predicate vector supplies a full stone support block.
        Ok(true)
    }
}
static SNOW_SURVIVAL: Behaviors = Behaviors::EMPTY.with_block(Block::SNOW, &SnowSurvivalFixture);
#[test]
fn live_freeze_and_snow_predicates_match_real_biome_calls() {
    use ferraloom_light::{DataLayer, LightLayer};
    use ferraloom_worldgen::biome_manager::Precipitation;
    for row in include_str!("../../tests/java/level/weather.tsv").lines() {
        let (input, expected) = row.split_once('\t').unwrap();
        let args: Vec<_> = input.split(':').collect();
        let biome = args[0].parse().unwrap();
        let light = args[1].parse().unwrap();
        let y: i32 = args[3].parse().unwrap();
        let surrounded = args[4] == "true";
        let pos = BlockPos::new(8, y, 8);
        let mut level = empty_level(&SNOW_SURVIVAL);
        install_weather_environment(&mut level, args[0]);
        let state = match args[2] {
            "water" => Block::WATER.default_state(),
            "flowing" => Block::WATER
                .default_state()
                .with(ferraloom_data::properties::LEVEL, 3)
                .unwrap(),
            "snow" => Block::SNOW.default_state(),
            "stone" => Block::STONE.default_state(),
            _ => Block::AIR.default_state(),
        };
        if (-16..16).contains(&y) {
            level.set_block_state(pos, state).unwrap();
            for direction in [
                Direction::West,
                Direction::East,
                Direction::North,
                Direction::South,
            ] {
                if surrounded {
                    level
                        .set_block_state(pos.relative(direction, 1), Block::WATER.default_state())
                        .unwrap();
                }
            }
        }
        if (-16..16).contains(&(y - 1)) {
            level
                .set_block_state(pos.below(1), Block::STONE.default_state())
                .unwrap();
        }
        level.set_light_snapshot(
            ChunkPos::new(0, 0),
            weather::ChunkLightSnapshot {
                sky: std::collections::BTreeMap::new(),
                block: (-1..=0).map(|y| (y, DataLayer::new(light))).collect(),
            },
        );
        let resources = weather_test_temperatures();
        let climate = resources.precipitation_at(&biome, [pos.x, pos.y, pos.z], 63);
        let climate = match climate {
            Precipitation::None => "none",
            Precipitation::Rain => "rain",
            Precipitation::Snow => "snow",
        };
        assert_eq!(
            format!(
                "{},{},{},{}",
                level.should_freeze(&biome, pos, false).unwrap(),
                level.should_freeze(&biome, pos, true).unwrap(),
                level.should_snow(&biome, pos).unwrap(),
                climate
            ),
            expected,
            "{input}"
        );
        if (-16..16).contains(&y) {
            assert_eq!(level.brightness(LightLayer::Block, pos).unwrap(), light);
        }
    }
}
#[test]
fn snow_accumulation_wraps_unsupported_survival_without_inventing_a_query_result() {
    use crate::block::{BlockMethod, UnportedBehavior};
    static PENDING_SNOW: Behaviors = Behaviors::EMPTY.with_block(Block::SNOW, &PendingBlockQueries);
    let mut level = empty_level(&PENDING_SNOW);
    let p = BlockPos::new(8, 0, 8);
    level
        .set_block_state(p.below(1), Block::STONE.default_state())
        .unwrap();
    install_weather_environment(&mut level, "snowy_plains");
    level.load_weather(weather::WeatherData {
        raining: true,
        ..Default::default()
    });
    let biome = "snowy_plains".parse().unwrap();
    let unsupported = Err(LevelError::UnsupportedBlockMethod(
        Block::SNOW,
        BlockMethod::CanSurvive,
    ));
    assert_eq!(level.should_snow(&biome, p), unsupported);
    assert_eq!(level.tick_precipitation(p), unsupported.map(|_| ()));
    level.set_unported_behavior(UnportedBehavior::Warn);
    assert_eq!(level.should_snow(&biome, p), unsupported);
    level.tick_precipitation(p).unwrap();
    level.tick_precipitation(p).unwrap();
    assert!(level.get_block_state(p).unwrap().is_air());
    assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
    assert_eq!(level.unported_behavior_stats().distinct_calls, 1);
}
#[test]
fn rain_queries_use_visible_sky_heightmap_and_zoomed_biome() {
    use ferraloom_light::DataLayer;
    let mut level = empty_level(&EMPTY);
    let p = BlockPos::new(8, 0, 8);
    assert!(!level.is_raining_at(p).unwrap());
    level.load_weather(weather::WeatherData {
        raining: true,
        ..Default::default()
    });
    assert_eq!(
        level.is_raining_at(p),
        Err(LevelError::UnsupportedLightQuery)
    );
    level.set_has_skylight(true);
    assert_eq!(
        level.is_raining_at(p),
        Err(LevelError::UnsupportedWeatherQuery)
    );
    level
        .set_block_state(p.below(1), Block::STONE.default_state())
        .unwrap();
    install_weather_environment(&mut level, "plains");
    assert!(level.is_raining_at(p).unwrap());
    assert!(!level.is_raining_at(p.below(1)).unwrap());
    level.set_light_snapshot(
        ChunkPos::new(0, 0),
        weather::ChunkLightSnapshot {
            sky: [(1, DataLayer::new(14))].into_iter().collect(),
            block: Default::default(),
        },
    );
    assert!(!level.is_raining_at(p).unwrap());
    install_weather_environment(&mut level, "snowy_plains");
    assert!(!level.is_raining_at(p).unwrap());
}

struct PostProcessTrace;
impl BlockBehavior for PostProcessTrace {
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        access: &mut crate::block::LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let crate::block::LevelAccessRef::Server(level) = access else {
            return Err(LevelError::UnsupportedWorldgenEnvironment);
        };
        level.emit(LevelEffect::LevelEvent {
            pos,
            id: 321,
            data: direction as i32,
        });
        let added = BlockPos::new(9, 0, 8);
        if pos == BlockPos::new(8, 0, 8)
            && direction == Direction::Down
            && level.get_block_state(added)?.is_air()
        {
            level.set_block_state(added, Block::DIRT.default_state())?;
            level.mark_pos_for_post_processing(added)?;
        }
        Ok(state)
    }
}
static POST_PROCESS_TRACE: Behaviors = Behaviors::EMPTY.with_block(Block::DIRT, &PostProcessTrace);
#[test]
fn post_processing_drains_sections_in_order_including_reentrant_appends() {
    let mut level = empty_level(&POST_PROCESS_TRACE);
    let upper = BlockPos::new(8, 0, 8);
    let lower = upper.below(1);
    for pos in [upper, lower] {
        level
            .set_block_state(pos, Block::DIRT.default_state())
            .unwrap();
        level.mark_pos_for_post_processing(pos).unwrap();
    }
    let chunk = ChunkPos::new(0, 0);
    assert!(!level.chunk(chunk).unwrap().post_processing_complete());
    level.post_process_generation(chunk).unwrap();
    assert!(level.chunk(chunk).unwrap().post_processing_complete());
    let trace: Vec<_> = level
        .take_effects()
        .into_iter()
        .map(|effect| match effect {
            LevelEffect::LevelEvent { pos, id: 321, data } => (pos, data),
            other => panic!("unexpected post-process effect {other:?}"),
        })
        .collect();
    let expected: Vec<_> = [lower, upper, BlockPos::new(9, 0, 8)]
        .into_iter()
        .flat_map(|pos| redstone::UPDATE_SHAPE_ORDER.map(|direction| (pos, direction as i32)))
        .collect();
    assert_eq!(trace, expected);
}
#[test]
fn pending_block_entity_nbt_survives_full_without_claiming_live_factory_readiness() {
    use crate::block::UnportedBehavior;
    use ferraloom_nbt::{CompoundTag, Compression, NbtAccounter, NbtIo, NbtString, Tag};
    fn encode(data: &ferraloom_storage::SerializableChunkData) -> Vec<u8> {
        let mut bytes = Vec::new();
        NbtIo::write_compound(&mut bytes, Compression::None, &data.write().unwrap()).unwrap();
        bytes
    }
    let chunk = ChunkPos::new(0, 0);
    let pos = BlockPos::new(8, 0, 8);
    let mut tag = CompoundTag::new();
    tag.insert(
        "id",
        Tag::String(Block::SPAWNER.block_entity_type().unwrap().name().into()),
    );
    for (key, value) in [("x", pos.x), ("y", pos.y), ("z", pos.z), ("future", 42)] {
        tag.insert(key, Tag::Int(value));
    }
    tag.insert("future_bytes", Tag::ByteArray(vec![-128, 0, 127]));
    // DoubleTag.valueOf and its native binary loader canonicalize signed zero.
    // Use the canonical constructor before checking untouched pending NBT bytes.
    tag.insert("future_double", Tag::from(-0.0_f64));
    tag.insert(
        "future_utf16",
        Tag::String(NbtString::from_utf16(vec![0xd800, 0, 0xdc00])),
    );
    let data = ferraloom_storage::SerializableChunkData {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        status: Some("minecraft:full".into()),
        x_pos: Some(chunk.x),
        y_pos: Some(-1),
        z_pos: Some(chunk.z),
        block_entities: Some(vec![tag.clone()]),
        post_processing: Some(vec![Vec::new(), Vec::new()]),
        ..Default::default()
    };
    let mut block_chunk = empty_chunk(chunk);
    block_chunk
        .set_block_state(pos, Block::SPAWNER.default_state())
        .unwrap();
    block_chunk.load_pending_generation_data(&data).unwrap();
    assert_eq!(
        block_chunk.pending_block_entity_nbt(),
        std::slice::from_ref(&tag)
    );
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    level.add_chunk(block_chunk).unwrap();
    // Pending-state access is a required query even if its real factory has
    // since been ported. Do not activate the receiver in this storage test.
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert_eq!(
            level.block_entity_ref(pos),
            Err(LevelError::UnsupportedBlockEntity(Block::SPAWNER))
        );
    }
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    level.set_unported_behavior(UnportedBehavior::Strict);
    assert_eq!(
        level.skip_block_entity_activation(Block::STONE, "test:pending_factory".into()),
        Err(LevelError::UnsupportedBlockEntity(Block::STONE))
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    for _ in 0..2 {
        level
            .skip_block_entity_activation(Block::STONE, "test:pending_factory".into())
            .unwrap();
    }
    assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
    assert_eq!(level.unported_behavior_stats().distinct_calls, 1);
    let block_chunk = level.chunk(chunk).unwrap();
    assert!(!block_chunk.post_processing_complete());
    assert!(block_chunk.generation_callbacks_drained());
    let mut saved = data.clone();
    block_chunk.save_pending_generation_data(&mut saved);
    assert_eq!(saved.block_entities, Some(vec![tag]));
    assert_eq!(saved.post_processing, data.post_processing);
    let bytes = encode(&saved);
    assert_eq!(bytes, encode(&data));
    let loaded = ferraloom_storage::SerializableChunkData::parse(
        NbtIo::read_compound(
            bytes.as_slice(),
            Compression::None,
            &mut NbtAccounter::uncompressed(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut reloaded_chunk = empty_chunk(chunk);
    reloaded_chunk
        .load_pending_generation_data(&loaded)
        .unwrap();
    let mut resaved = loaded.clone();
    reloaded_chunk.save_pending_generation_data(&mut resaved);
    assert_eq!(encode(&resaved), bytes);
}

#[test]
fn worldgen_skips_share_policy_counters_and_deduplicate_owned_unit_behavior_pairs() {
    use crate::block::{UnportedBehavior, UnportedOperation};
    let mut level = Level::new(
        Region::new(&EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let pair =
        crate::block::WorldgenSkip::from(("minecraft:oak".to_owned(), "canSurvive".to_owned()));
    level.record_worldgen_skips(Vec::new()).unwrap();
    assert_eq!(
        level.record_worldgen_skips(vec![pair.clone()]),
        Err(LevelError::UnsupportedWorldgenBehavior)
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    level.set_unported_behavior(UnportedBehavior::Warn);
    level
        .unported_operation(UnportedOperation::PackReload)
        .unwrap();
    let shared = &level;
    shared
        .record_worldgen_skips(vec![pair.clone(), pair.clone()])
        .unwrap();
    shared
        .record_worldgen_skips(vec![
            pair,
            crate::block::WorldgenSkip::from((
                "minecraft:oak".to_owned(),
                "updateShape".to_owned(),
            )),
            crate::block::WorldgenSkip::from((
                "minecraft:birch".to_owned(),
                "canSurvive".to_owned(),
            )),
        ])
        .unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 6);
    assert_eq!(level.unported_behavior_stats().distinct_calls, 4);
    level.set_unported_behavior(UnportedBehavior::Strict);
    assert_eq!(
        level.record_worldgen_skips(vec![crate::block::WorldgenSkip::from((
            "minecraft:oak".into(),
            "canSurvive".into()
        ))]),
        Err(LevelError::UnsupportedWorldgenBehavior)
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 6);
}

#[test]
fn save_snapshot_marker_keeps_subsequent_live_mutations_dirty() {
    use crate::block::UpdateFlags;
    let mut level = empty_level(&EMPTY);
    install_tags(&mut level);
    let pos = BlockPos::new(8, 0, 8);
    let chunk = ChunkPos::containing(pos);
    assert!(!level.try_mark_chunk_saved(chunk).unwrap());
    level
        .set_block(
            pos,
            Block::STONE.default_state(),
            UpdateFlags::KNOWN_SHAPE,
            512,
        )
        .unwrap();
    assert!(level.chunk(chunk).unwrap().is_unsaved());
    assert!(level.try_mark_chunk_saved(chunk).unwrap());
    let saved_state = level.get_block_state(pos).unwrap();
    assert!(!level.chunk(chunk).unwrap().is_unsaved());
    level
        .set_block(
            pos,
            Block::DIRT.default_state(),
            UpdateFlags::KNOWN_SHAPE,
            512,
        )
        .unwrap();
    // Completing the old snapshot cannot clear this later write.
    assert_eq!(saved_state, Block::STONE.default_state());
    assert!(level.chunk(chunk).unwrap().is_unsaved());
    assert!(level.try_mark_chunk_saved(chunk).unwrap());
    assert!(!level.try_mark_chunk_saved(chunk).unwrap());
    level.mark_chunk_unsaved(chunk).unwrap();
    assert!(level.chunk(chunk).unwrap().is_unsaved());
    let missing = ChunkPos::new(2, 0);
    assert_eq!(
        level.try_mark_chunk_saved(missing),
        Err(LevelError::MissingChunk(missing))
    );
}

struct PostProcessUnsupported;
impl BlockBehavior for PostProcessUnsupported {
    fn update_shape(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        _level: &mut crate::block::LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if pos.x == 8 {
            Err(LevelError::UnsupportedBlockMethod(
                Block::DIRT,
                crate::block::BlockMethod::UpdateShape,
            ))
        } else {
            Ok(Block::STONE.default_state())
        }
    }
}
static POST_PROCESS_UNSUPPORTED: Behaviors =
    Behaviors::EMPTY.with_block(Block::DIRT, &PostProcessUnsupported);
#[test]
fn warn_post_processing_retains_skipped_units_and_continues_in_position_order() {
    use crate::block::UnportedBehavior;
    let mut level = empty_level(&POST_PROCESS_UNSUPPORTED);
    install_tags(&mut level);
    let skipped = BlockPos::new(8, 0, 8);
    let next = BlockPos::new(9, 0, 8);
    for pos in [skipped, next] {
        level
            .set_block_state(pos, Block::DIRT.default_state())
            .unwrap();
        level.mark_pos_for_post_processing(pos).unwrap();
    }
    let chunk = ChunkPos::containing(skipped);
    assert_eq!(
        level.post_process_generation(chunk),
        Err(LevelError::UnsupportedBlockMethod(
            Block::DIRT,
            crate::block::BlockMethod::UpdateShape
        ))
    );
    assert_eq!(
        level.get_block_state(next).unwrap(),
        Block::DIRT.default_state()
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    level.post_process_generation(chunk).unwrap();
    assert_eq!(
        level.get_block_state(skipped).unwrap(),
        Block::DIRT.default_state()
    );
    assert_eq!(
        level.get_block_state(next).unwrap(),
        Block::STONE.default_state()
    );
    assert!(!level.chunk(chunk).unwrap().post_processing_complete());
    assert!(!level.chunk(chunk).unwrap().generation_callbacks_drained());
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level
        .chunk(chunk)
        .unwrap()
        .save_pending_generation_data(&mut saved);
    assert_eq!(
        saved.post_processing,
        Some(vec![Vec::new(), vec![8 | (8 << 8)]])
    );
    level.post_process_generation(chunk).unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
    assert_eq!(level.unported_behavior_stats().distinct_calls, 1);
}
