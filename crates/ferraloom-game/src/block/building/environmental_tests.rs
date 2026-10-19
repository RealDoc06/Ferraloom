use super::{connected_tests::connected_level, stair_slab_tests::ticks, tests::POS};
use crate::{
    block::{BlockMethod, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef, entity},
    level::{
        Level, LevelError,
        environment::{ClockSnapshot, EnvironmentDefinitions},
    },
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
    properties as p,
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::Direction;
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/environmental.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static DEFINITIONS: LazyLock<Arc<EnvironmentDefinitions>> = LazyLock::new(|| {
    let mut pack = Pack::new("building-constant-environment");
    pack.insert(
        "worldgen/biome/case.json".parse().unwrap(),
        br#"{"attributes":{}}"#.to_vec(),
    );
    Arc::new(EnvironmentDefinitions::load(&ResourceManager::new(vec![pack])).unwrap())
});
fn cases(prefix: &str) -> impl Iterator<Item = &str> {
    VECTORS.lines().filter(move |line| line.starts_with(prefix))
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
fn side(id: &str) -> Direction {
    Direction::VALUES[id.parse::<usize>().unwrap()]
}
fn environment(level: &mut Level, active: bool) {
    let dimension = DEFINITIONS
        .dimension(&serde_json::json!({"attributes":{"gameplay/creaking_active":active}}))
        .unwrap();
    level
        .set_environment_attributes(
            Arc::clone(&DEFINITIONS),
            &dimension,
            "overworld".parse().unwrap(),
            &["case".parse().unwrap()],
            ClockSnapshot::new([]),
        )
        .unwrap();
}
#[test]
fn creaking_heart_placement_uses_log_axes_live_tag_and_canonical_environment() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let mut matched = 0;
    let mut active = None;
    for line in cases("heart\t") {
        let f: Vec<_> = line.split('\t').collect();
        let next = f[1] == "1";
        if active != Some(next) {
            environment(&mut level, next);
            active = Some(next);
        }
        let face = side(f[2]);
        for side in Direction::VALUES {
            level
                .set_block_state(POS.relative(side, 1), Block::AIR.default_state())
                .unwrap();
        }
        level
            .set_block_state(POS, Block::AIR.default_state())
            .unwrap();
        let axis = face.axis();
        level
            .set_block_state(POS.relative(axis.positive(), 1), state(f[3]))
            .unwrap();
        level
            .set_block_state(POS.relative(axis.negative(), 1), state(f[4]))
            .unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            face,
        )
        .unwrap();
        assert_eq!(
            level
                .get_state_for_placement(Block::CREAKING_HEART, &context)
                .unwrap()
                .unwrap()
                .id(),
            f[5].parse::<u16>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 3468);
    let mut matched = 0;
    for line in cases("heart-update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = side(f[2]);
        let mut level = connected_level();
        let actual = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                state(f[3]),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(ticks(&mut level), f[5], "{line}");
        matched += 1;
    }
    assert_eq!(matched, 108);
}
#[test]
fn heart_queries_environment_only_after_both_required_logs_match() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let context = DirectionalPlaceContext::new(
        &LevelReaderRef::Server(&level),
        POS,
        Direction::North,
        &item,
        Direction::Up,
    )
    .unwrap();
    assert_eq!(
        level
            .get_state_for_placement(Block::CREAKING_HEART, &context)
            .unwrap()
            .unwrap()
            .get(p::CREAKING_HEART_STATE),
        Some(p::CreakingHeartState::Uprooted)
    );
    for side in [Direction::Up, Direction::Down] {
        level
            .set_block_state(POS.relative(side, 1), Block::PALE_OAK_LOG.default_state())
            .unwrap();
    }
    assert!(matches!(
        level.get_state_for_placement(Block::CREAKING_HEART, &context),
        Err(LevelError::MissingEnvironmentContext)
    ));
}
fn sulfur_world(top: BlockState, base: BlockState, origin: BlockState) -> Level {
    let mut level = connected_level();
    level.set_block_state(POS, origin).unwrap();
    level.set_block_state(POS.above(1), top).unwrap();
    level.set_block_state(POS.below(1), base).unwrap();
    if origin.block() == Block::COMPARATOR {
        level
            .set_block_entity(entity::create(POS, origin).unwrap())
            .unwrap();
    }
    level
}
#[test]
fn sulfur_placement_and_shape_transitions_match_without_or_with_wrong_receiver() {
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("sulfur\t") {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = sulfur_world(state(f[1]), state(f[2]), state(f[3]));
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            Direction::Up,
        )
        .unwrap();
        assert_eq!(
            level
                .get_state_for_placement(Block::POTENT_SULFUR, &context)
                .unwrap()
                .unwrap()
                .id(),
            f[4].parse::<u16>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 84);
    let mut matched = 0;
    for line in cases("sulfur-update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = side(f[5]);
        let mut level = sulfur_world(state(f[2]), state(f[3]), state(f[4]));
        let actual = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                Block::STONE.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[6].parse::<u16>().unwrap(), "{line}");
        assert!(ticks(&mut level).is_empty(), "{line}");
        matched += 1;
    }
    assert_eq!(matched, 2520);
}
