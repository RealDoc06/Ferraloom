use super::{required_behavior, stack::tests::level};
use crate::{
    block::{
        BlockPlaceContext, DirectionalPlaceContext, InteractionResult, LevelReaderRef,
        PlacementHit, UseOnContext,
    },
    inventory::StackOperation,
};
use ferraloom_data::{Block, BlockState, Item, ItemMethod};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{
    BlockPos, Direction, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::io::Read;

fn vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../tests/java/items/placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}

#[test]
fn native_standing_and_wall_item_stage_keeps_native_direction_order_and_supports() {
    let (mut level, _) = level();
    let pos = BlockPos::new(8, 5, 8);
    // Audit every receiver the standing/wall native vectors can select before
    // running cases, so a missing family registration is reported in one pass.
    let reader = LevelReaderRef::Server(&level);
    let mut missing = Vec::new();
    for item in [
        Item::TORCH,
        Item::SOUL_TORCH,
        Item::OAK_SIGN,
        Item::OAK_HANGING_SIGN,
        Item::TUBE_CORAL_FAN,
        Item::DEAD_TUBE_CORAL_FAN,
    ] {
        for block in [item.data().block.unwrap(), item.wall_block().unwrap()] {
            for method in [
                crate::block::BlockMethod::CanSurvive,
                crate::block::BlockMethod::UpdateShape,
                crate::block::BlockMethod::UpdateFromNeighbourShapes,
            ] {
                if let Err(error) = reader.required_behavior(block, method) {
                    missing.push(error);
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "standing/wall dispatch gaps: {missing:?}"
    );
    let mut calls = 0;
    for line in vectors()
        .lines()
        .filter(|line| line.starts_with("standing\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let item = Item::from_protocol_id(fields[1].parse().unwrap()).unwrap();
        let mask = fields[2].parse::<u32>().unwrap();
        for (index, direction) in Direction::VALUES.into_iter().enumerate() {
            level
                .set_block_state(
                    pos.relative(direction, 1),
                    if mask & (1 << index) == 0 {
                        Block::AIR
                    } else {
                        Block::STONE
                    }
                    .default_state(),
                )
                .unwrap();
        }
        level
            .set_block_state(pos, Block::AIR.default_state())
            .unwrap();
        let stack = ItemStack::new(item, 1, Default::default());
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            Direction::VALUES[fields[3].parse::<usize>().unwrap()],
            &stack,
            Direction::VALUES[fields[4].parse::<usize>().unwrap()],
        )
        .unwrap();
        *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let mut operation = StackOperation::new(&mut level);
        let state = required_behavior(item, ItemMethod::GetPlacementState)
            .unwrap()
            .get_placement_state(item, &context, &mut operation)
            .unwrap();
        assert_eq!(
            state.map_or(-1, |s| i32::from(s.id())),
            fields[5].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            operation.level.random().next_long(),
            fields[6].parse::<i64>().unwrap(),
            "{line}"
        );
        assert_eq!(operation.level.unported_behavior_stats().skipped_calls, 0);
        calls += 1;
    }
    assert_eq!(calls, 13_824);
}

#[test]
fn native_double_high_item_clears_upper_water_before_base_placement() {
    let (mut level, _) = level();
    let pos = BlockPos::new(8, 5, 8);
    let mut calls = 0;
    for line in vectors()
        .lines()
        .filter(|line| line.starts_with("double\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let item = Item::from_protocol_id(fields[1].parse().unwrap()).unwrap();
        let above = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        for direction in Direction::VALUES {
            level
                .set_block_state(pos.relative(direction, 1), Block::AIR.default_state())
                .unwrap();
        }
        level
            .set_block_state(pos, Block::AIR.default_state())
            .unwrap();
        level
            .set_block_state(pos.below(1), Block::DIRT.default_state())
            .unwrap();
        level.set_block_state(pos.above(1), above).unwrap();
        let stack = ItemStack::new(item, 1, Default::default());
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            pos,
            Direction::North,
            &stack,
            Direction::Up,
        )
        .unwrap();
        *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let mut operation = StackOperation::new(&mut level);
        let result = required_behavior(item, ItemMethod::PlaceBlock)
            .unwrap()
            .place_block(
                item,
                &context,
                item.data().block.unwrap().default_state(),
                &mut operation,
            )
            .unwrap();
        assert_eq!(result, fields[3].parse::<bool>().unwrap(), "{line}");
        assert_eq!(
            operation.level.get_block_state(pos).unwrap().id(),
            fields[4].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            operation.level.get_block_state(pos.above(1)).unwrap().id(),
            fields[5].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            operation.level.random().next_long(),
            fields[6].parse::<i64>().unwrap(),
            "{line}"
        );
        calls += 1;
    }
    assert_eq!(calls, 18);
}

#[test]
fn native_scaffolding_item_walks_up_without_a_horizontal_distance_limit() {
    let (mut level, _) = level();
    let pos = BlockPos::new(8, 5, 8);
    let item = Item::SCAFFOLDING;
    let mut calls = 0;
    for line in vectors()
        .lines()
        .filter(|line| line.starts_with("scaffolding\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        for offset in 0..12 {
            for direction in [Direction::Up, Direction::North] {
                level
                    .set_block_state(pos.relative(direction, offset), Block::AIR.default_state())
                    .unwrap();
            }
        }
        let face = Direction::VALUES[fields[1].parse::<usize>().unwrap()];
        let direction = if face == Direction::Up {
            Direction::North
        } else {
            Direction::Up
        };
        let chain = fields[2].parse::<i32>().unwrap();
        for offset in 0..=chain {
            level
                .set_block_state(
                    pos.relative(direction, offset),
                    Block::SCAFFOLDING.default_state(),
                )
                .unwrap();
        }
        if fields[3] == "true" {
            level
                .set_block_state(
                    pos.relative(direction, chain + 1),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        let stack = ItemStack::new(item, 1, Default::default());
        let context = BlockPlaceContext::new(
            &LevelReaderRef::Server(&level),
            UseOnContext::new(
                None,
                InteractionHand::MainHand,
                &stack,
                PlacementHit {
                    pos,
                    face,
                    location: Vec3::new(8.5, 5.5, 8.5),
                    inside: false,
                },
            ),
        )
        .unwrap();
        *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let mut operation = StackOperation::new(&mut level);
        let result = required_behavior(item, ItemMethod::UpdatePlacementContext)
            .unwrap()
            .update_placement_context(item, context, &mut operation)
            .unwrap();
        let result = result.map_or_else(
            || "none".into(),
            |context| {
                let pos = context.clicked_pos();
                format!(
                    "{}, {}, {}:{}",
                    pos.x,
                    pos.y,
                    pos.z,
                    context.clicked_face() as usize
                )
            },
        );
        assert_eq!(result, fields[4], "{line}");
        assert_eq!(
            operation.level.random().next_long(),
            fields[5].parse::<i64>().unwrap(),
            "{line}"
        );
        calls += 1;
    }
    assert_eq!(calls, 36);
}

#[test]
fn native_place_on_water_item_defers_block_use_to_its_air_use_path() {
    let (mut level, player) = level();
    for line in vectors()
        .lines()
        .filter(|line| line.starts_with("water-use-on\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let item = Item::from_protocol_id(fields[1].parse().unwrap()).unwrap();
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(player, ItemStack::new(item, 1, Default::default()))
            .unwrap();
        let context = UseOnContext::new_retained(
            None,
            InteractionHand::MainHand,
            receiver,
            PlacementHit {
                pos: BlockPos::new(8, 5, 8),
                face: Direction::Up,
                location: Vec3::new(8.5, 5.5, 8.5),
                inside: false,
            },
        );
        assert_eq!(
            super::stack::use_on(receiver, &context, &mut operation).unwrap(),
            Some(InteractionResult::Pass)
        );
        assert_eq!(fields[2], "true");
    }
}
