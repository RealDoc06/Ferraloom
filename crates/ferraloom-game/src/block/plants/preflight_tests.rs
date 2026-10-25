use crate::block::{BlockMethod as M, GameWorldgenBlockBehavior, LevelAccessRef};
use ferraloom_data::{Block, BlockState, Item, ItemMethod, registries::fluid::Id as Fluid};
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use ferraloom_worldgen::region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior};
use std::io::Read;

/// The live scenario's explicit blocks and native grass/tree/column outputs.
/// Snowy-plains flower_default selects poppy/dandelion; grass can become tall
/// grass, and the flower-free oak variants have only dirt/log/leaf providers.
const BLOCKS: &[Block] = &[
    Block::AIR,
    Block::BAMBOO,
    Block::BEETROOTS,
    Block::BIG_DRIPLEAF,
    Block::BLUE_ICE,
    Block::CACTUS,
    Block::CACTUS_FLOWER,
    Block::CARROTS,
    Block::CLAY,
    Block::DANDELION,
    Block::DIRT,
    Block::DIRT_PATH,
    Block::FARMLAND,
    Block::FERN,
    Block::FROSTED_ICE,
    Block::GLASS,
    Block::GLOWSTONE,
    Block::GLOW_LICHEN,
    Block::GRASS_BLOCK,
    Block::ICE,
    Block::KELP,
    Block::KELP_PLANT,
    Block::LARGE_FERN,
    Block::LIGHT,
    Block::MANGROVE_LEAVES,
    Block::MANGROVE_PROPAGULE,
    Block::MYCELIUM,
    Block::OAK_LEAVES,
    Block::OAK_LOG,
    Block::OAK_SAPLING,
    Block::PACKED_ICE,
    Block::PITCHER_CROP,
    Block::POPPY,
    Block::POTATOES,
    Block::REDSTONE_BLOCK,
    Block::SAND,
    Block::SHORT_GRASS,
    Block::SNOW,
    Block::STONE,
    Block::SUGAR_CANE,
    Block::SUNFLOWER,
    Block::SWEET_BERRY_BUSH,
    Block::TALL_GRASS,
    Block::VINE,
    Block::WATER,
    Block::WHEAT,
];

#[test]
fn live_plant_scenario_resolves_all_required_dispatch_before_server_start() {
    let mut missing = Vec::new();
    let mut check = |block: Block, method: M| {
        if crate::block::dispatch::registered_behavior(block, method).is_none() {
            missing.push(format!("{} {method:?}", block.name()));
        }
    };
    for &block in BLOCKS {
        // Commands, feature writes, shape-triggered removal, neighboring power,
        // precipitation and the real player fall all enter these shared paths.
        for method in [
            M::CanSurvive,
            M::OnPlace,
            M::AffectNeighborsAfterRemoval,
            M::NeighborChanged,
            M::UpdateShape,
            M::UpdateFromNeighbourShapes,
            M::UpdateIndirectNeighborShapes,
            M::GetDrops,
            M::SpawnAfterBreak,
            M::SpawnDestroyParticles,
            M::IsSignalSource,
            M::OwnSignal,
            M::Signal,
            M::DirectSignal,
            M::HasAnalogOutputSignal,
            M::HandlePrecipitation,
            M::FallOn,
            M::StepOn,
            M::BounceOn,
            M::EntityInsideCollisionShape,
        ] {
            check(block, method);
        }
        let data = block.data();
        for id in data.first_state..data.first_state + data.state_count {
            let state = BlockState::from_id(id).unwrap();
            if state.data().random_ticking {
                check(block, M::RandomTick);
            }
            // Cached state queries legitimately avoid virtual dispatch, as in
            // native BlockStateBase. Dynamic states still require their ports.
            if state.collision_shape().is_none() {
                check(block, M::CollisionShape);
            }
            // Base collision delegates to outline only for colliding blocks.
            // LightBlock's held-item outline is not read by these fixtures.
            if block.has_collision() && state.shape().is_none() {
                check(block, M::OutlineShape);
            }
            if state.cached_collision_full_block().is_none() {
                check(block, M::IsCollisionShapeFullBlock);
            }
            if Direction::VALUES
                .into_iter()
                .any(|face| state.cached_sturdy_face(face).is_none())
            {
                check(block, M::IsFaceSturdy);
            }
        }
    }
    for block in [
        Block::SAND,
        Block::OAK_LEAVES,
        Block::MANGROVE_LEAVES,
        Block::FARMLAND,
        Block::DIRT_PATH,
        Block::FROSTED_ICE,
        Block::KELP,
        Block::KELP_PLANT,
        Block::CACTUS,
        Block::SUGAR_CANE,
        Block::BAMBOO,
        Block::BIG_DRIPLEAF,
    ] {
        check(block, M::Tick);
    }
    // Only these blocks intersect the actor in the scenario. The cactus,
    // berry and crop fixtures are remote from the player; their unported
    // ravager/damaging contacts are separate behavior coverage.
    for block in [
        Block::AIR,
        Block::STONE,
        Block::DIRT,
        Block::CLAY,
        Block::FARMLAND,
        Block::BIG_DRIPLEAF,
        Block::SHORT_GRASS,
        Block::FERN,
        Block::TALL_GRASS,
        Block::DANDELION,
        Block::POPPY,
        Block::SNOW,
    ] {
        check(block, M::EntityInside);
    }
    for block in [
        Block::GRASS_BLOCK,
        Block::OAK_SAPLING,
        Block::PITCHER_CROP,
        Block::SWEET_BERRY_BUSH,
        Block::MANGROVE_LEAVES,
    ] {
        check(block, M::UseItemOn);
        check(block, M::UseWithoutItem);
        assert!(
            super::bonemealable_behavior(block).is_some(),
            "{} bonemeal",
            block.name()
        );
    }
    crate::item::required_behavior(Item::BONE_MEAL, ItemMethod::UseOn).unwrap();
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "live plant dispatch gaps:\n{}",
        missing.join("\n")
    );
}

const POS: BlockPos = BlockPos::new(8, 5, 8);
fn point(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

#[test]
fn native_light_block_updates_preserve_state_water_ticks_and_rng_on_both_arms() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/light-updates.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (_, generator) = super::worldgen_tests::setup();
    let mut chunks = [RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Features,
    )];
    let mut calls = 0;
    for line in text.lines().filter(|line| line.starts_with("light\t")) {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let neighbor = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        let direction = fields[3].parse::<i32>().unwrap();
        let mut live = super::tests::level_with_height(2);
        live.set_block_state(POS, state).unwrap();
        for side in Direction::VALUES {
            live.set_block_state(POS.relative(side, 1), neighbor)
                .unwrap();
        }
        *live.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let mut access = LevelAccessRef::Server(&mut live);
        let result = if direction < 0 {
            access.update_from_neighbor_shapes(state, POS).unwrap()
        } else {
            let side = Direction::VALUES[direction as usize];
            access
                .as_reader()
                .required_behavior(Block::LIGHT, M::UpdateShape)
                .unwrap()
                .update_shape(
                    state,
                    POS,
                    side,
                    POS.relative(side, 1),
                    neighbor,
                    &mut access,
                )
                .unwrap()
        };
        assert_eq!(result.id(), fields[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(
            live.random().next_long(),
            fields[6].parse::<i64>().unwrap(),
            "{line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        live.save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        assert!(saved.block_ticks.unwrap_or_default().is_empty(), "{line}");
        let actual = saved
            .fluid_ticks
            .unwrap_or_default()
            .iter()
            .map(|tick| {
                assert_eq!(
                    [tick.x.unwrap(), tick.y.unwrap(), tick.z.unwrap()],
                    point(POS)
                );
                format!(
                    "{},{},{}",
                    tick.id.as_ref().unwrap().to_string_checked().unwrap(),
                    tick.delay.unwrap(),
                    tick.priority.unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, fields[5], "{line}");
        assert_eq!(live.unported_behavior_stats().skipped_calls, 0);

        chunks[0].fluid_ticks = Default::default();
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Features],
            0,
            None,
            &GameWorldgenBlockBehavior,
            0,
            63,
        )
        .unwrap();
        region.set_block_in_section(point(POS), state).unwrap();
        for side in Direction::VALUES {
            region
                .set_block_in_section(point(POS.relative(side, 1)), neighbor)
                .unwrap();
        }
        region.set_random(Random::Legacy(LegacyRandomSource::new(1234)));
        let result = if direction < 0 {
            GameWorldgenBlockBehavior
                .update_from_neighbor_shapes(&mut region, point(POS), state)
                .unwrap()
        } else {
            let side = Direction::VALUES[direction as usize];
            GameWorldgenBlockBehavior
                .update_shape(
                    &mut region,
                    point(POS),
                    side,
                    point(POS.relative(side, 1)),
                    neighbor,
                )
                .unwrap()
        };
        assert_eq!(result.id(), fields[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(
            region.random_mut().unwrap().next_long(),
            fields[6].parse::<i64>().unwrap(),
            "{line}"
        );
        drop(region);
        let expected: Vec<_> = fields[5]
            .split(';')
            .filter(|t| !t.is_empty())
            .map(|t| {
                let fields: Vec<_> = t.split(',').collect();
                (fields[0].to_owned(), fields[2].parse::<i32>().unwrap())
            })
            .collect();
        let actual: Vec<_> = chunks[0]
            .fluid_ticks
            .ticks()
            .iter()
            .map(|tick| {
                assert_eq!(tick.position, point(POS));
                assert_eq!(tick.kind, Fluid::WATER);
                (tick.kind.name().to_owned(), tick.priority)
            })
            .collect();
        assert_eq!(actual, expected, "{line}");
        calls += 1;
    }
    assert_eq!(calls, 448);
}

#[test]
fn native_light_contexts_keep_outline_and_inherited_collision_distinct() {
    use crate::block::{BlockCollisionContext, LevelReaderRef};
    use crate::entity::collision_context::EntityCollisionView;
    use ferraloom_util::shapes::{self, BooleanOp, PositionCollisionContext};

    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/light-updates.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (_, generator) = super::worldgen_tests::setup();
    let mut chunks = [RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Features,
    )];
    let mut live = super::tests::level_with_height(2);
    let mut calls = 0;
    for line in text
        .lines()
        .filter(|line| line.starts_with("light-shape\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        live.set_block_state(POS, state).unwrap();
        *live.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let position = PositionCollisionContext::<Item, ferraloom_data::FluidState>::new(2.0);
        let context = match fields[2].parse::<u8>().unwrap() {
            0 => BlockCollisionContext::empty(),
            index @ 1..=4 => {
                // Same scalar arguments as the native protected constructor;
                // holding-item queries do not resolve the nullable entity.
                let mut captured = EntityCollisionView::placement_context(None, &live).unwrap();
                captured.placement = false;
                captured.entity_bottom = 0.0;
                captured.held_item = Some(
                    [Item::AIR, Item::LIGHT, Item::STONE, Item::BONE_MEAL][usize::from(index - 1)],
                );
                BlockCollisionContext::Entity(captured)
            }
            5 => BlockCollisionContext::Position(&position),
            6 => BlockCollisionContext::placement_context(None, &live).unwrap(),
            _ => panic!("unknown native collision context"),
        };
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Features],
            0,
            None,
            &GameWorldgenBlockBehavior,
            0,
            63,
        )
        .unwrap();
        region.set_block_in_section(point(POS), state).unwrap();
        region.set_random(Random::Legacy(LegacyRandomSource::new(1234)));
        for reader in [
            LevelReaderRef::Server(&live),
            LevelReaderRef::Worldgen(&region),
        ] {
            let shapes = [
                reader.shape_with_context(state, POS, &context).unwrap(),
                reader.collision_shape(state, POS, &context).unwrap(),
                reader.shape(state, POS).unwrap(),
                reader.collision_shape_without_context(state, POS).unwrap(),
            ];
            for (index, actual) in shapes.into_iter().enumerate() {
                let expected = if fields[3 + index].parse::<bool>().unwrap() {
                    shapes::block()
                } else {
                    shapes::empty()
                };
                assert!(
                    !shapes::join_is_not_empty(&actual, &expected, BooleanOp::NotSame),
                    "shape{index}: {line}"
                );
            }
        }
        assert_eq!(
            live.random().next_long(),
            fields[7].parse::<i64>().unwrap(),
            "{line}"
        );
        assert_eq!(
            region.random_mut().unwrap().next_long(),
            fields[7].parse::<i64>().unwrap(),
            "{line}"
        );
        calls += 1;
    }
    assert_eq!(calls, 224);
}
