use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, BlockMethod, UnportedBehavior},
    entity::{
        EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
use ferraloom_util::{ChunkPos, Uuid};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

static EMPTY: Behaviors = Behaviors::EMPTY;
const VECTORS: &str = include_str!("../../../tests/java/level/clip.tsv");
const POS: BlockPos = BlockPos::new(8, 64, 8);
fn ints(s: &str) -> Vec<i32> {
    s.split(',').map(|v| v.parse().unwrap()).collect()
}
fn vec(s: &str) -> Vec3 {
    let v: Vec<f64> = s.split(',').map(|v| v.parse().unwrap()).collect();
    Vec3::new(v[0], v[1], v[2])
}
fn pos(s: &str) -> BlockPos {
    let v = ints(s);
    BlockPos::new(v[0], v[1], v[2])
}
fn tags(level: &mut Level) {
    struct Tags {
        block: Arc<TagSet<Block>>,
        fluid: Arc<TagSet<fluid::Id>>,
    }
    static TAGS: OnceLock<Tags> = OnceLock::new();
    let tags = TAGS.get_or_init(|| {
        let jar =
            std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native datapack");
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
        Tags {
            block: Arc::new(TagSet::<Block>::load(&resources).unwrap()),
            fluid: Arc::new(TagSet::<fluid::Id>::load(&resources).unwrap()),
        }
    });
    level.set_block_tags(tags.block.clone());
    level.set_fluid_tags(tags.fluid.clone());
}
fn level(loaded: &str, states: &str, behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(Region::new(behaviors), LevelContext::fixture(0, 63));
    tags(&mut level);
    for key in loaded.split(';').filter(|key| !key.is_empty()) {
        let xz = ints(key);
        let p = ChunkPos::new(xz[0], xz[1]);
        let mut chunk = LevelChunk::new(
            p,
            -4..20,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        );
        for state in states.split(';').filter(|s| !s.is_empty()) {
            let v = ints(state);
            let pos = BlockPos::new(v[0], v[1], v[2]);
            if ChunkPos::containing(pos) == p {
                chunk
                    .set_block_state(pos, BlockState::from_id(v[3] as u16).unwrap())
                    .unwrap();
            }
        }
        level.add_chunk(chunk).unwrap();
    }
    level
}
fn context(from: Vec3, to: Vec3, block: &str, fluid: &str) -> ClipContext<'static> {
    ClipContext::new(
        from,
        to,
        match block {
            "COLLIDER" => ClipBlock::Collider,
            "OUTLINE" => ClipBlock::Outline,
            "VISUAL" => ClipBlock::Visual,
            "FALLDAMAGE_RESETTING" => ClipBlock::FallDamageResetting,
            _ => panic!("unknown mode {block}"),
        },
        match fluid {
            "NONE" => ClipFluid::None,
            "SOURCE_ONLY" => ClipFluid::SourceOnly,
            "ANY" => ClipFluid::Any,
            "WATER" => ClipFluid::Water,
            _ => panic!("unknown mode {fluid}"),
        },
        BlockCollisionContext::empty(),
    )
}
fn assert_hit(actual: BlockHitResult, expected: &str) {
    let f: Vec<_> = expected.split('|').collect();
    assert_eq!(actual.miss, f[0] == "MISS", "{expected}");
    assert_eq!(actual.location, vec(f[1]), "{expected}");
    assert_eq!(
        actual.direction.data_3d(),
        f[2].parse::<i32>().unwrap(),
        "{expected}"
    );
    assert_eq!(actual.block_pos, pos(f[3]), "{expected}");
    assert_eq!(actual.inside, f[4] == "true", "{expected}");
    assert_eq!(actual.world_border_hit, f[5] == "true", "{expected}");
}

#[test]
fn native_clip_modes_fluid_ties_misses_inside_and_grazing() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "clip" {
            continue;
        }
        let level = level(f[2], f[3], &EMPTY);
        let query = context(vec(f[4]), vec(f[5]), f[6], f[7]);
        match level.clip(&query) {
            Ok(hit) => assert_hit(hit, f[8]),
            Err(LevelError::MissingChunk(p)) => {
                assert_eq!(format!("missing:{},{}", p.x, p.z), f[8], "{line}")
            }
            Err(err) => panic!("{line}: {err:?}"),
        }
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        cases += 1;
    }
    assert_eq!(cases, 517);
}

#[test]
fn native_traversal_start_cells_ties_negative_faces_and_early_exit() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "traverse" {
            continue;
        }
        let mut visited = Vec::new();
        let stop = f[4].parse::<usize>().unwrap();
        traverse_blocks(vec(f[2]), vec(f[3]), |pos| {
            visited.push(pos);
            Ok(visited.len() == stop)
        })
        .unwrap();
        let expected: Vec<_> = f[5].split(';').filter(|p| !p.is_empty()).map(pos).collect();
        assert_eq!(visited, expected, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 18);
}

#[test]
fn native_block_in_line_tests_predicates_and_reports_the_endpoint() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "line" {
            continue;
        }
        let level = level(f[2], f[3], &REQUIRED);
        let tags = level.block_tags().unwrap();
        let calls = std::cell::Cell::new(0);
        let query = ClipBlockStateContext::new(vec(f[4]), vec(f[5]), |state: BlockState| {
            calls.set(calls.get() + 1);
            match f[6] {
                "occludes" => {
                    tags.contains_key(BlockTags::OCCLUDES_VIBRATION_SIGNALS, state.block())
                }
                "water" => level
                    .fluid_tags()
                    .unwrap()
                    .contains_key(FluidTags::WATER, state.data().fluid.fluid_type),
                "air" => state.is_air(),
                _ => state.block() == Block::STONE,
            }
        });
        let reads = f[8]
            .split(';')
            .filter(|s| !s.is_empty() && *s != "-")
            .count();
        match level.is_block_in_line(&query) {
            Ok(hit) => {
                assert_hit(hit, f[7]);
                assert_eq!(calls.get(), reads);
            }
            Err(LevelError::MissingChunk(p)) => {
                assert_eq!(format!("missing:{},{}", p.x, p.z), f[7], "{line}");
                assert_eq!(calls.get(), reads - 1);
            }
            Err(error) => panic!("{line}: {error:?}"),
        }
        cases += 1;
    }
    assert_eq!(cases, 13);
}

#[test]
fn native_fluid_first_height_is_memoized_without_later_neighbor_reads() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "memo" {
            continue;
        }
        let first_above = f[1] == "true";
        let mut level = level("0,0", "", &EMPTY);
        let state = Block::WATER.default_state().data().fluid;
        let expected: Vec<_> = f[2].split(',').collect();
        for (i, above) in [first_above, !first_above, first_above]
            .into_iter()
            .enumerate()
        {
            level
                .set_block_state(
                    POS.above(1),
                    if above { Block::WATER } else { Block::AIR }.default_state(),
                )
                .unwrap();
            let shape = level.fluid_shape(state, POS).unwrap();
            let (height, reads) = expected[i].split_once(':').unwrap();
            assert_eq!(
                shape.max(ferraloom_util::Axis::Y),
                height.parse::<f64>().unwrap()
            );
            assert_eq!(reads, if i == 0 { "1" } else { "0" });
            // A memo hit does not acquire even the chunk containing the above cell.
            let cached = level.fluid_shape(state, BlockPos::new(32, 64, 8)).unwrap();
            assert!(shape.ptr_eq(&cached));
        }
        cases += 1;
    }
    assert_eq!(cases, 2);
}

struct Interaction;
impl BlockBehavior for Interaction {
    fn interaction_shape(
        &self,
        _: BlockState,
        _: BlockPos,
        _: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(shapes::block())
    }
}
static INTERACTION: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &Interaction);
#[test]
fn native_interaction_override_preserves_location_and_changes_only_closer_face() {
    let level = level("0,0", "", &INTERACTION);
    let shape = shapes::box_shape(0.5, 0.0, 0.0, 1.0, 1.0, 1.0);
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "interaction" {
            continue;
        }
        let from = vec(f[1]);
        let to = vec(f[2]);
        assert_hit(shape.clip(from, to, POS).unwrap().into(), f[3]);
        assert_hit(
            level
                .clip_with_interaction_override(from, to, POS, &shape, Block::STONE.default_state())
                .unwrap()
                .unwrap(),
            f[4],
        );
        cases += 1;
    }
    assert_eq!(cases, 2);
}

#[test]
fn native_player_fall_reset_shapes_use_actual_portal_delay_and_tags() {
    let mut level = level("0,0", "", &EMPTY);
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::offline_player("Clip"),
                "Clip".into(),
                level.next_entity_id(),
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "player-shape" {
            continue;
        }
        level.rules_mut().players_nether_portal_default_delay = f[2].parse().unwrap();
        let query = ClipContext::for_entity(
            Vec3::ZERO,
            Vec3::X_AXIS,
            ClipBlock::FallDamageResetting,
            ClipFluid::None,
            Some(player),
            &level,
        )
        .unwrap();
        assert_eq!(
            !query
                .get_block_shape(
                    BlockState::from_id(f[1].parse().unwrap()).unwrap(),
                    &level,
                    POS
                )
                .unwrap()
                .is_empty(),
            f[3] == "true",
            "{line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 10);
}

#[test]
fn with_position_preserves_the_native_location_and_flags_for_misses_and_hits() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "with-position" {
            continue;
        }
        let old = Vec3::new(8.5, 64.25, 8.75);
        let hit = if cases == 0 {
            BlockHitResult::new(old, Direction::East, POS, true).hit_border()
        } else {
            BlockHitResult::miss(old, Direction::East, POS)
        };
        assert_hit(hit.with_position(BlockPos::new(-1, 0, 16)), f[1]);
        cases += 1;
    }
    assert_eq!(cases, 2);
}

struct RequiredShapes;
impl BlockBehavior for RequiredShapes {
    fn visual_shape(
        &self,
        state: BlockState,
        _: BlockPos,
        _: &BlockCollisionContext<'_>,
        _: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::VisualShape,
        ))
    }
    fn interaction_shape(
        &self,
        state: BlockState,
        _: BlockPos,
        _: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::InteractionShape,
        ))
    }
}
static REQUIRED: Behaviors = Behaviors::EMPTY.with_block(Block::WATER, &RequiredShapes);
#[test]
fn shape_queries_are_required_under_both_policies_and_misses_skip_interaction_query() {
    let states = format!("8,64,8,{}", Block::WATER.default_state().id());
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level("0,0", &states, &REQUIRED);
        level.set_unported_behavior(policy);
        for (mode, method) in [
            (ClipBlock::Visual, BlockMethod::VisualShape),
            (ClipBlock::Outline, BlockMethod::InteractionShape),
        ] {
            let query = ClipContext::new(
                Vec3::new(8.5, 66.0, 8.5),
                Vec3::new(8.5, 64.5, 8.5),
                mode,
                ClipFluid::None,
                BlockCollisionContext::empty(),
            );
            assert_eq!(
                level.clip(&query),
                Err(LevelError::UnsupportedBlockMethod(Block::WATER, method))
            );
        }
        assert_eq!(
            level
                .clip_with_interaction_override(
                    Vec3::ZERO,
                    Vec3::X_AXIS,
                    POS,
                    &shapes::empty(),
                    Block::WATER.default_state()
                )
                .unwrap(),
            None
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
#[test]
fn tag_modes_require_the_installed_context_under_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level("0,0", "", &EMPTY);
        level.set_unported_behavior(policy);
        level.region_mut().tags = None;
        level.region_mut().fluid_tags = None;
        let from = Vec3::new(8.0, 64.0, 8.0);
        let to = Vec3::new(9.0, 64.0, 8.0);
        assert_eq!(
            level.clip(&context(from, to, "FALLDAMAGE_RESETTING", "NONE")),
            Err(LevelError::MissingBlockTags)
        );
        assert_eq!(
            level.clip(&context(from, to, "OUTLINE", "WATER")),
            Err(LevelError::MissingFluidTags)
        );
        assert!(
            level
                .clip(&context(from, to, "OUTLINE", "NONE"))
                .unwrap()
                .miss
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}

struct ContextShape;
impl BlockBehavior for ContextShape {
    fn shape_with_context(
        &self,
        _: BlockState,
        _: BlockPos,
        context: &BlockCollisionContext<'_>,
        _: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        use ferraloom_util::CollisionContext;
        Ok(if context.is_holding_item(&ferraloom_data::Item::STICK) {
            shapes::block()
        } else {
            shapes::empty()
        })
    }
    fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        self.shape_with_context(state, pos, context, reader)
    }
}
static CONTEXT: Behaviors = Behaviors::EMPTY.with_block(Block::WATER, &ContextShape);
#[test]
fn all_shape_modes_receive_the_supplied_collision_context() {
    let states = format!("8,64,8,{}", Block::WATER.default_state().id());
    let mut level = level("0,0", &states, &CONTEXT);
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::offline_player("Context"),
                "Context".into(),
                level.next_entity_id(),
                Vec3::ZERO,
                GameType::Survival,
            ))),
        )
        .unwrap();
    for mode in [ClipBlock::Collider, ClipBlock::Outline, ClipBlock::Visual] {
        for held in [None, Some(ferraloom_data::Item::STICK)] {
            let mut entity = EntityCollisionView::capture(player, &level, false).unwrap();
            entity.held_item = held;
            let query = ClipContext::new(
                Vec3::new(7.0, 64.25, 8.5),
                Vec3::new(10.0, 64.25, 8.5),
                mode,
                ClipFluid::None,
                BlockCollisionContext::Entity(entity),
            );
            assert_eq!(level.clip(&query).unwrap().miss, held.is_none());
        }
    }
}
