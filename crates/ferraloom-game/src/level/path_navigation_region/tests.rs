use super::*;
use crate::{
    Behaviors,
    block::{
        UnportedBehavior, entity::BlockEntityDataView,
        redstone::moving_piston::PistonMovingBlockEntity,
    },
    entity::item::ItemEntity,
    level::{
        LevelContext, Region,
        acquisition::{
            AcquireDriver, AcquireFailure, AcquireHandle, AcquireProgress, AcquireResult,
        },
        chunk_holder::ChunkStatus,
    },
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::datapack::{Pack, ResourceManager};
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    path::Path,
    sync::{Arc, OnceLock},
};

static EMPTY: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 64, 8);
const VECTORS: &str = include_str!("../../../tests/java/level/path-navigation-region.tsv");
fn ints(value: &str) -> Vec<i32> {
    value.split(',').map(|v| v.parse().unwrap()).collect()
}
fn pos(value: &str) -> BlockPos {
    let v = ints(value);
    BlockPos::new(v[0], v[1], v[2])
}
fn empty_chunk(pos: ChunkPos) -> LevelChunk {
    LevelChunk::new(
        pos,
        -4..20,
        LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
            BiomeId(0),
        ),
    )
}
fn level(chunks: &str, behaviors: &'static Behaviors) -> Level {
    struct Tags {
        blocks: Arc<TagSet<Block>>,
        fluids: Arc<TagSet<fluid::Id>>,
        entities: Arc<TagSet<EntityType>>,
    }
    static TAGS: OnceLock<Tags> = OnceLock::new();
    let tags = TAGS.get_or_init(|| {
        let jar =
            std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native datapack");
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
        Tags {
            blocks: Arc::new(TagSet::load(&resources).unwrap()),
            fluids: Arc::new(TagSet::load(&resources).unwrap()),
            entities: Arc::new(TagSet::load(&resources).unwrap()),
        }
    });
    let mut level = {
        let mut fixture_level = Level::new(Region::new(behaviors), LevelContext::fixture(0, 63));
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    level.set_block_tags(tags.blocks.clone());
    level.set_fluid_tags(tags.fluids.clone());
    level.set_entity_type_tags(tags.entities.clone());
    for chunk in chunks.split(';').filter(|s| !s.is_empty()) {
        let v = ints(chunk);
        level
            .add_chunk(empty_chunk(ChunkPos::new(v[0], v[1])))
            .unwrap();
    }
    level
}

/// Only the getChunkNow holder lookup is available. Every loading/progress
/// operation fails immediately, so a shape cannot silently escape the region.
struct NoLoadDriver {
    chunks: HashMap<ChunkPos, ChunkReference>,
    reads: RefCell<Vec<ChunkPos>>,
}
impl AcquireDriver for NoLoadDriver {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        true
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        panic!("ticket query during bounded read")
    }
    fn add_unknown_ticket(&self, _: ChunkPos, _: ChunkStatus) -> Result<(), LevelError> {
        panic!("load during bounded read")
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        panic!("distance update during bounded read")
    }
    fn request_status(
        &self,
        _: &Level,
        _: ChunkPos,
        _: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        panic!("request during bounded read")
    }
    fn acquisition_result(&self, _: AcquireHandle) -> Result<AcquireResult, LevelError> {
        panic!("generation during bounded read")
    }
    fn poll_acquisition_step(&self, _: &Level) -> Result<AcquireProgress, LevelError> {
        panic!("pump during bounded read")
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        panic!("park during bounded read")
    }
    fn chunk_if_present(&self, pos: ChunkPos, status: ChunkStatus) -> Option<ChunkReference> {
        assert_eq!(status, ChunkStatus::Full);
        self.reads.borrow_mut().push(pos);
        self.chunks.get(&pos).copied()
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        panic!("unexpected unchecked lookup")
    }
    fn acquire_on_owner(
        &self,
        _: ChunkPos,
        _: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        panic!("forward during bounded read")
    }
}
fn install_no_load_driver(level: &mut Level) {
    let chunks = level
        .region()
        .chunks
        .values()
        .map(|chunk| {
            (
                chunk.pos(),
                ChunkReference {
                    pos: chunk.pos(),
                    generation: chunk.publication_generation,
                    kind: ChunkKind::Full,
                },
            )
        })
        .collect();
    level
        .install_acquire_driver(
            Box::new(NoLoadDriver {
                chunks,
                reads: RefCell::new(Vec::new()),
            }),
            -64..320,
        )
        .unwrap();
}
fn reads(level: &Level) -> Vec<ChunkPos> {
    level
        .acquire_driver
        .as_ref()
        .unwrap()
        .as_any()
        .downcast_ref::<NoLoadDriver>()
        .unwrap()
        .reads
        .borrow()
        .clone()
}

#[test]
fn native_capture_order_nullable_slots_and_air_height_boundaries() {
    let mut cases = 0;
    for row in VECTORS.lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] != "capture" {
            continue;
        }
        let mut level = level(f[2], &EMPTY);
        match f[1] {
            "bounded" => {
                level
                    .set_block_state(POS, Block::STONE.default_state())
                    .unwrap();
                level
                    .set_block_state(BlockPos::new(16, 64, 8), Block::WATER.default_state())
                    .unwrap();
            }
            name if name.starts_with("section-stepping-") => {
                level
                    .set_block_state(POS, Block::STONE.default_state())
                    .unwrap();
            }
            _ => {}
        }
        install_no_load_driver(&mut level);
        let region = PathNavigationRegion::capture(&level, pos(f[3]), pos(f[4])).unwrap();
        assert_eq!(region.all_empty(), f[7] == "true", "{row}");
        let expected_reads: Vec<_> = f[8]
            .split(';')
            .map(|v| {
                let p = ints(v);
                ChunkPos::new(p[0], p[1])
            })
            .collect();
        assert_eq!(reads(&level), expected_reads, "{row}");
        let reader = region.reader(&level).unwrap();
        for (point, expected) in f[5].split(';').zip(f[6].split(';')) {
            let p = pos(point);
            let expected = ints(expected);
            assert_eq!(
                i32::from(reader.get_block_state(p).unwrap().id()),
                expected[0],
                "{row}, {point}"
            );
            assert_eq!(
                i32::from(reader.get_fluid_state(p).unwrap().fluid_type.protocol_id()),
                expected[1],
                "{row}, {point}"
            );
        }
        assert_eq!(
            reads(&level),
            expected_reads,
            "region query escaped getChunkNow capture"
        );
        cases += 1;
    }
    assert_eq!(cases, 5);
}

#[test]
fn native_live_mutations_are_visible_but_absent_and_stale_slots_never_rebind() {
    let mut cases = 0;
    for row in VECTORS.lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] != "mutation" {
            continue;
        }
        let mut level = level(if f[1] == "late-load" { "" } else { "0,0" }, &EMPTY);
        if f[1] != "late-load" {
            level
                .set_block_state(POS, Block::STONE.default_state())
                .unwrap();
        }
        let snapshot = PathNavigationRegion::capture(
            &level,
            BlockPos::new(0, 64, 0),
            BlockPos::new(15, 64, 15),
        )
        .unwrap();
        match f[1] {
            "block-change" => {
                level
                    .set_block_state(POS, Block::DIRT.default_state())
                    .unwrap();
            }
            "late-load" => {
                level.add_chunk(empty_chunk(ChunkPos::ZERO)).unwrap();
                level
                    .set_block_state(POS, Block::DIRT.default_state())
                    .unwrap();
            }
            "unload-replace" => {
                // Java keeps the original chunk object. Native createPath is
                // synchronous; Rust rejects this invalid reuse across unload.
                assert_eq!(
                    f[2].parse::<u16>().unwrap(),
                    Block::STONE.default_state().id()
                );
                level.remove_chunk(ChunkPos::ZERO).unwrap();
                assert!(matches!(
                    snapshot.reader(&level).unwrap().get_block_state(POS),
                    Err(LevelError::PathNavigation(PathNavigationError::StaleChunk(
                        _
                    )))
                ));
                level.add_chunk(empty_chunk(ChunkPos::ZERO)).unwrap();
                level
                    .set_block_state(POS, Block::DIRT.default_state())
                    .unwrap();
                assert!(matches!(
                    snapshot.reader(&level).unwrap().get_block_state(POS),
                    Err(LevelError::PathNavigation(PathNavigationError::StaleChunk(
                        _
                    )))
                ));
                cases += 1;
                continue;
            }
            _ => panic!("{row}"),
        }
        assert_eq!(
            snapshot
                .reader(&level)
                .unwrap()
                .get_block_state(POS)
                .unwrap()
                .id(),
            f[2].parse::<u16>().unwrap(),
            "{row}"
        );
        cases += 1;
    }
    assert_eq!(cases, 3);
}

#[test]
fn owner_and_acquisition_failures_are_distinct_from_nullable_snapshot_entries() {
    let mut first = level("0,0", &EMPTY);
    let second = level("0,0", &EMPTY);
    let snapshot = PathNavigationRegion::capture(&first, POS, POS).unwrap();
    assert!(matches!(
        snapshot.reader(&second),
        Err(LevelError::PathNavigation(
            PathNavigationError::WrongOwner { .. }
        ))
    ));
    let owner = first.region.take().unwrap();
    assert!(matches!(
        snapshot.reader(&first),
        Err(LevelError::PathNavigation(
            PathNavigationError::MissingOwner
        ))
    ));
    assert!(matches!(
        PathNavigationRegion::capture(&first, POS, POS),
        Err(LevelError::PathNavigation(
            PathNavigationError::MissingOwner
        ))
    ));
    first.region = Some(owner);
    install_no_load_driver(&mut first);
    let driver = first
        .acquire_driver
        .as_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NoLoadDriver>()
        .unwrap();
    driver.chunks.get_mut(&ChunkPos::ZERO).unwrap().generation += 1;
    assert!(matches!(
        PathNavigationRegion::capture(&first, POS, POS),
        Err(LevelError::ChunkAcquisition(
            ChunkPos::ZERO,
            ChunkStatus::Full,
            AcquireFailure::Publication
        ))
    ));
}

struct NeighborShape;
impl BlockBehavior for NeighborShape {
    fn collision_shape(
        &self,
        _: BlockState,
        pos: BlockPos,
        _: &BlockCollisionContext<'_>,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(
            if reader
                .get_block_state(pos.relative(Direction::East, 1))?
                .block()
                == Block::STONE
            {
                shapes::block()
            } else {
                shapes::empty()
            },
        )
    }
}
static NEIGHBOR: Behaviors = Behaviors::EMPTY.with_block(Block::WATER, &NeighborShape);
#[test]
fn nested_shape_reads_are_bounded_and_never_request_a_live_neighbor() {
    let mut level = level("0,0;1,0", &NEIGHBOR);
    let p = BlockPos::new(15, 64, 8);
    level
        .set_block_state(p, Block::WATER.default_state())
        .unwrap();
    level
        .set_block_state(p.relative(Direction::East, 1), Block::STONE.default_state())
        .unwrap();
    install_no_load_driver(&mut level);
    let snapshot =
        PathNavigationRegion::capture(&level, BlockPos::new(0, 64, 0), BlockPos::new(15, 64, 15))
            .unwrap();
    let captured_reads = reads(&level);
    let reader = snapshot.reader(&level).unwrap();
    assert!(
        reader
            .collision_shape(
                reader.get_block_state(p).unwrap(),
                p,
                &BlockCollisionContext::empty()
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        reader
            .no_collision(None, Aabb::new(15.1, 64.1, 8.1, 15.9, 64.9, 8.9))
            .unwrap()
    );
    assert_eq!(reads(&level), captured_reads);
}

#[test]
fn native_collision_context_empty_entities_and_world_border_order() {
    let mut cases = 0;
    for row in VECTORS.lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] != "collision" {
            continue;
        }
        let mut level = level("0,0;1,0", &EMPTY);
        let p = if f[1] == "absent" {
            BlockPos::new(16, 64, 8)
        } else {
            POS
        };
        let block = match f[1] {
            "slab" => Block::STONE_SLAB,
            "powder" => Block::POWDER_SNOW,
            "border" => Block::AIR,
            _ => Block::STONE,
        };
        level.set_block_state(p, block.default_state()).unwrap();
        let mut border = crate::level::border::WorldBorder::default();
        border.set_size(if f[1] == "border" { 10.0 } else { 59_999_968.0 });
        level.set_entity_world_border(border);
        let source = if f[2] == "true" {
            let entity = level.next_entity_ref();
            let mut item = ItemEntity::new(level.next_entity_id().unwrap(), 0);
            item.parent.set_pos(if f[1] == "border" {
                ferraloom_util::Vec3::new(4.5, 64.0, 0.5)
            } else {
                ferraloom_util::Vec3::new(8.5, 64.0, 8.5)
            });
            item.parent.fall_distance = 3.0;
            level
                .add_entity(entity, EntityState::Item(Box::new(item)))
                .unwrap();
            Some(entity)
        } else {
            None
        };
        install_no_load_driver(&mut level);
        let snapshot = PathNavigationRegion::capture(
            &level,
            BlockPos::new(0, 0, 0),
            BlockPos::new(15, 319, 15),
        )
        .unwrap();
        let reader = snapshot.reader(&level).unwrap();
        let coords: Vec<f64> = f[3].split(',').map(|v| v.parse().unwrap()).collect();
        let area = Aabb::new(
            coords[0], coords[1], coords[2], coords[3], coords[4], coords[5],
        );
        assert_eq!(
            reader.no_collision(source, area).unwrap(),
            f[4] == "true",
            "{row}"
        );
        let shape = reader
            .collision_shape(
                reader.get_block_state(p).unwrap(),
                p,
                &BlockCollisionContext::empty(),
            )
            .unwrap();
        assert_eq!(
            if shape.is_empty() {
                0.0
            } else {
                shape.max(ferraloom_util::Axis::Y)
            },
            f[5].parse::<f64>().unwrap(),
            "{row}"
        );
        assert_eq!(
            reader.get_entity_collisions(source, area).unwrap().len(),
            f[6].parse::<usize>().unwrap()
        );
        cases += 1;
    }
    assert_eq!(cases, 10);
}

#[test]
fn native_moving_piston_reads_only_the_captured_chunks_block_entities() {
    let mut level = level("0,0;1,0", &EMPTY);
    let p = BlockPos::new(16, 64, 8);
    let state = Block::MOVING_PISTON.default_state();
    level.set_block_state(p, state).unwrap();
    let mut entity = crate::block::entity::create(p, state).unwrap();
    let piston = PistonMovingBlockEntity::view_mut(&mut entity).unwrap();
    piston.moved_state = Block::STONE.default_state();
    piston.extending = true;
    piston.direction = Direction::East;
    level.set_block_entity(entity).unwrap();
    install_no_load_driver(&mut level);
    let mut cases = 0;
    for row in VECTORS.lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] != "piston" {
            continue;
        }
        let end = f[1].parse().unwrap();
        let snapshot = PathNavigationRegion::capture(
            &level,
            BlockPos::new(0, 64, 0),
            BlockPos::new(end, 64, 15),
        )
        .unwrap();
        let reader = snapshot.reader(&level).unwrap();
        assert_eq!(
            reader
                .collision_shape(state, p, &BlockCollisionContext::empty())
                .unwrap()
                .is_empty(),
            f[2] == "true",
            "{row}"
        );
        cases += 1;
    }
    assert_eq!(cases, 2);
}

#[test]
fn non_block_getter_queries_remain_required_errors_under_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level("0,0", &EMPTY);
        level.set_unported_behavior(policy);
        let snapshot = PathNavigationRegion::capture(&level, POS, POS).unwrap();
        let reader = snapshot.reader(&level).unwrap();
        assert_eq!(
            reader.get_raw_brightness(POS, 0),
            Err(LevelError::UnsupportedPathNavigationQuery(
                PathNavigationQuery::Light
            ))
        );
        assert_eq!(
            reader.sky_darken(),
            Err(LevelError::UnsupportedPathNavigationQuery(
                PathNavigationQuery::Environment
            ))
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
