use super::{
    stair_slab_tests::ticks,
    tests::{POS, level},
};
use crate::{
    block::{BlockMethod, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef},
    level::Level,
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::Direction;
use std::{
    collections::BTreeSet,
    io::Read,
    sync::{Arc, LazyLock},
};

const SIDES: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/connected.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static TAGS: LazyLock<Arc<TagSet<Block>>> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native tags");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    Arc::new(TagSet::load(&resources).unwrap())
});
fn connected_level() -> Level {
    let mut level = level();
    level.set_block_tags(Arc::clone(&TAGS));
    level
}
fn cases(prefix: &str) -> impl Iterator<Item = &str> {
    VECTORS.lines().filter(move |line| line.starts_with(prefix))
}

#[test]
fn fence_pane_and_wall_placement_matches_native_connection_and_coverage_masks() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let mut matched = 0;
    let mut blocks = BTreeSet::new();
    for line in cases("place\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let mask = f[2].parse::<u8>().unwrap();
        let fluid = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        let top = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        level.set_block_state(POS, fluid).unwrap();
        level.set_block_state(POS.above(1), top).unwrap();
        for (index, side) in SIDES.into_iter().enumerate() {
            level
                .set_block_state(
                    POS.relative(side, 1),
                    if mask & (1 << index) == 0 {
                        Block::AIR.default_state()
                    } else {
                        Block::STONE.default_state()
                    },
                )
                .unwrap();
        }
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            Direction::Up,
        )
        .unwrap();
        let actual = level
            .get_state_for_placement(block, &context)
            .unwrap()
            .unwrap();
        assert_eq!(actual.id(), f[5].parse::<u16>().unwrap(), "{line}");
        blocks.insert(block);
        matched += 1;
    }
    assert_eq!(blocks.len(), 72);
    assert_eq!(matched, 54144);
}

#[test]
fn native_static_registry_neighbor_footprints_keep_fence_types_gates_and_exceptions() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("neighbor\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let neighbor = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[3].parse::<usize>().unwrap()];
        let fluid = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        for side in Direction::VALUES {
            level
                .set_block_state(POS.relative(side, 1), Block::AIR.default_state())
                .unwrap();
        }
        level.set_block_state(POS, fluid).unwrap();
        level
            .set_block_state(POS.relative(direction, 1), neighbor)
            .unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            Direction::Up,
        )
        .unwrap();
        let actual = level
            .get_state_for_placement(block, &context)
            .unwrap()
            .unwrap();
        assert_eq!(actual.id(), f[5].parse::<u16>().unwrap(), "{line}");
        matched += 1;
    }
    assert_eq!(matched, 15216);
}

#[test]
fn connected_shape_updates_use_captured_neighbors_and_preserve_other_sides() {
    let mut matched = 0;
    for line in cases("update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let neighbor = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        let top = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        let mut level = connected_level();
        let at = POS.relative(direction, 1);
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_state(
                at,
                if neighbor.is_air() {
                    Block::STONE.default_state()
                } else {
                    Block::AIR.default_state()
                },
            )
            .unwrap();
        level.set_block_state(POS.above(1), top).unwrap();
        let behavior = level
            .required_block_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap();
        let actual = behavior
            .update_shape(
                state,
                POS,
                direction,
                at,
                neighbor,
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[5].parse::<u16>().unwrap(), "state {line}");
        assert_eq!(ticks(&mut level), f[6], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 15504);
}
