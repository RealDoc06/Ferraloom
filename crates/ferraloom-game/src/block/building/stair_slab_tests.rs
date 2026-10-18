use super::{
    facing, stairs,
    tests::{POS, level},
};
use crate::{
    block::{
        BlockMethod, BlockPlaceContext, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef,
        PlacementHit, UseOnContext,
    },
    level::Level,
};
use ferraloom_data::{Block, BlockState, Item, properties as p};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{ChunkPos, Direction, Vec3};
use std::{io::Read, sync::LazyLock};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/stairs-slabs.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
fn cases(prefix: &str) -> impl Iterator<Item = &str> {
    VECTORS.lines().filter(move |line| line.starts_with(prefix))
}
fn context<'stack>(
    level: &Level,
    item: &'stack ItemStack,
    face: Direction,
    height: usize,
) -> BlockPlaceContext<'stack> {
    let y = match height {
        0 => 64.0,
        1 => 64.5,
        2 => f64::from_bits(64.5_f64.to_bits() + 1),
        3 => 65.0,
        4 => f64::NAN,
        _ => unreachable!(),
    };
    BlockPlaceContext::new(
        &LevelReaderRef::Server(level),
        UseOnContext::new(
            None,
            InteractionHand::MainHand,
            item,
            PlacementHit {
                pos: POS,
                face,
                location: Vec3::new(8.5, y, 8.5),
                inside: false,
            },
        ),
    )
    .unwrap()
}
pub(super) fn ticks(level: &mut Level) -> String {
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
        .unwrap();
    let mut ticks = saved
        .block_ticks
        .unwrap_or_default()
        .into_iter()
        .map(|tick| format!("schedule:{}", tick.delay.unwrap()))
        .collect::<Vec<_>>();
    ticks.extend(
        saved
            .fluid_ticks
            .unwrap_or_default()
            .into_iter()
            .map(|tick| format!("fluid:{}", tick.delay.unwrap())),
    );
    ticks.join(",")
}

#[test]
fn stairs_and_slabs_match_native_directional_placements_and_click_height_boundaries() {
    let mut level = level();
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("place\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let face = Direction::VALUES[f[3].parse::<usize>().unwrap()];
        let old = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        level.set_block_state(POS, old).unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            direction,
            &item,
            face,
        )
        .unwrap();
        let actual = level.get_state_for_placement(block, &context).unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[5].parse::<i32>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    for line in cases("height\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let face = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let old = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        level.set_block_state(POS, old).unwrap();
        let context = context(&level, &item, face, f[3].parse().unwrap());
        let actual = level.get_state_for_placement(block, &context).unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[5].parse::<i32>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 89532);
}

#[test]
fn slab_replacement_and_stack_joining_match_native_context_masks() {
    let mut level = level();
    let mut matched = 0;
    for line in cases("replace\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let face = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        for direction in Direction::VALUES {
            level
                .set_block_state(POS.relative(direction, 1), Block::AIR.default_state())
                .unwrap();
        }
        level.set_block_state(POS, state).unwrap();
        let item = match f[4] {
            "0" => ItemStack::default(),
            "1" => ItemStack::new(state.block().as_item(), 1, Default::default()),
            "2" => ItemStack::new(state.block().as_item(), 0, Default::default()),
            "3" => ItemStack::new(Item::STONE, 1, Default::default()),
            _ => unreachable!(),
        };
        let context = context(&level, &item, face, f[3].parse().unwrap());
        let reader = LevelReaderRef::Server(&level);
        assert_eq!(
            context.replacing_clicked_on_block(&reader).unwrap(),
            f[5].parse::<bool>().unwrap(),
            "context {line}"
        );
        assert_eq!(
            reader.can_be_replaced(state, &context).unwrap(),
            f[6].parse::<bool>().unwrap(),
            "replace {line}"
        );
        let actual = level
            .get_state_for_placement(state.block(), &context)
            .unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[7].parse::<i32>().unwrap(),
            "state {line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 72720);
}

#[test]
fn stair_corners_and_neighbor_obstructions_match_native_masks() {
    let mut level = level();
    let mut matched = 0;
    for line in cases("corner\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let behind = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let front = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        let mask = f[4].parse::<u8>().unwrap();
        let direction = facing(state);
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_state(POS.relative(direction, 1), behind)
            .unwrap();
        level
            .set_block_state(POS.relative(direction.opposite(), 1), front)
            .unwrap();
        for (side, bit) in [
            (direction.counter_clockwise(), 1),
            (direction.clockwise(), 2),
        ] {
            level
                .set_block_state(
                    POS.relative(side, 1),
                    if mask & bit == 0 {
                        Block::AIR.default_state()
                    } else {
                        state
                    },
                )
                .unwrap();
        }
        let actual = stairs::stairs_shape(state, &LevelReaderRef::Server(&level), POS).unwrap();
        assert_eq!(
            state.with(p::SHAPE, actual).unwrap().id(),
            f[5].parse::<u16>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    for line in cases("take-shape\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let neighbor = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        level.set_block_state(POS.east(1), neighbor).unwrap();
        assert_eq!(
            stairs::can_take_shape(state, &LevelReaderRef::Server(&level), POS, Direction::East)
                .unwrap(),
            f[3].parse::<bool>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 67288);
}

#[test]
fn stair_and_slab_shape_updates_match_native_states_and_water_ticks() {
    let mut matched = 0;
    for line in cases("update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let mut level = level();
        level.set_block_state(POS, state).unwrap();
        let behavior = level
            .required_block_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap();
        let actual = behavior
            .update_shape(
                state,
                POS,
                direction,
                POS.relative(direction, 1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[3].parse::<u16>().unwrap(), "state {line}");
        assert_eq!(ticks(&mut level), f[4], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 50196);
}

#[test]
fn slab_waterlogging_rejects_double_and_flowing_water_like_vanilla() {
    let mut matched = 0;
    for line in cases("liquid\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let fluid = BlockState::from_id(f[2].parse().unwrap())
            .unwrap()
            .data()
            .fluid;
        let mut level = level();
        level.set_block_state(POS, state).unwrap();
        let query = level
            .required_block_behavior(state.block(), BlockMethod::CanPlaceLiquid)
            .unwrap();
        assert_eq!(
            query.can_place_liquid(state, fluid.fluid_type),
            f[3].parse::<bool>().unwrap(),
            "allowed {line}"
        );
        let behavior = level
            .required_block_behavior(state.block(), BlockMethod::PlaceLiquid)
            .unwrap();
        assert_eq!(
            behavior
                .place_liquid(state, POS, fluid, &mut LevelAccessRef::Server(&mut level))
                .unwrap(),
            f[4].parse::<bool>().unwrap(),
            "placed {line}"
        );
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[5].parse::<u16>().unwrap(),
            "state {line}"
        );
        assert_eq!(ticks(&mut level), f[6], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 1818);
}

#[test]
fn falling_block_landing_context_checks_native_slab_replacement() {
    let mut level = level();
    let empty = ItemStack::default();
    let mut matched = 0;
    for line in cases("landing\t") {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(POS, state).unwrap();
        let reader = LevelReaderRef::Server(&level);
        let context =
            DirectionalPlaceContext::new(&reader, POS, Direction::Down, &empty, Direction::Up)
                .unwrap();
        assert_eq!(
            reader.can_be_replaced(state, &context).unwrap(),
            f[2].parse::<bool>().unwrap(),
            "receiver {line}"
        );
        assert_eq!(
            context.can_place(&reader).unwrap(),
            f[3].parse::<bool>().unwrap(),
            "canPlace {line}"
        );
        assert_eq!(
            context.replacing_clicked_on_block(&reader).unwrap(),
            f[4].parse::<bool>().unwrap(),
            "replacing {line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 606);
}
