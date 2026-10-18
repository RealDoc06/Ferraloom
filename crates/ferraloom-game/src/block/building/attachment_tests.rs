use super::{connected_tests::connected_level, stair_slab_tests::ticks, tests::POS};
use crate::{
    block::{
        BlockMethod, BlockPlaceContext, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef,
        PlacementHit, UseOnContext,
    },
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::Level,
};
use ferraloom_data::{Block, BlockState, Item, properties as p};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{Direction, Uuid, Vec3};
use std::{collections::BTreeSet, io::Read, sync::LazyLock};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/attachments.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
fn cases(prefix: &str) -> impl Iterator<Item = &str> {
    VECTORS.lines().filter(move |line| line.starts_with(prefix))
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
fn block(id: &str) -> Block {
    Block::from_protocol_id(id.parse().unwrap()).unwrap()
}
fn direction(id: &str) -> Direction {
    Direction::VALUES[id.parse::<usize>().unwrap()]
}
fn mask(level: &mut Level, mask: u8) {
    for (index, side) in Direction::VALUES.into_iter().enumerate() {
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
}
fn player_context<'stack>(level: &Level, item: &'stack ItemStack) -> BlockPlaceContext<'stack> {
    BlockPlaceContext::new(
        &LevelReaderRef::Server(level),
        UseOnContext::new(
            Some(EntityRef(1)),
            InteractionHand::MainHand,
            item,
            PlacementHit {
                pos: POS,
                face: Direction::Up,
                location: Vec3::new(8.5, 64.5, 8.5),
                inside: false,
            },
        ),
    )
    .unwrap()
}
#[test]
fn attachments_match_every_variant_face_support_mask_and_actual_player_look_order() {
    let mut level = connected_level();
    level
        .add_entity(
            EntityRef(1),
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "building".into(),
                1,
                Vec3::new(8.5, 64.5, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    let item = ItemStack::default();
    let mut matched = 0;
    let mut blocks = BTreeSet::new();
    for line in cases("place\t").chain(cases("player\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let block = block(f[1]);
        mask(&mut level, f[2].parse().unwrap());
        level.set_block_state(POS, state(f[5])).unwrap();
        let actual = if f[0] == "place" {
            let context = DirectionalPlaceContext::new(
                &LevelReaderRef::Server(&level),
                POS,
                direction(f[3]),
                &item,
                direction(f[4]),
            )
            .unwrap();
            level.get_state_for_placement(block, &context)
        } else {
            let actor = level.entity_mut::<EntityData>(EntityRef(1)).unwrap();
            actor.yaw = f[3].parse().unwrap();
            actor.pitch = f[4].parse().unwrap();
            let context = player_context(&level, &item);
            level.get_state_for_placement(block, &context)
        }
        .unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[6].parse::<i32>().unwrap(),
            "{line}"
        );
        blocks.insert(block);
        matched += 1;
    }
    assert_eq!(blocks.len(), 20);
    assert_eq!(matched, 193536);
}
#[test]
fn attachment_survival_matches_native_full_face_center_and_unstable_support_footprints() {
    let mut level = connected_level();
    let mut matched = 0;
    for line in cases("survive\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        for side in Direction::VALUES {
            level
                .set_block_state(POS.relative(side, 1), state(f[2]))
                .unwrap();
        }
        let behavior = level
            .required_block_behavior(current.block(), BlockMethod::CanSurvive)
            .unwrap();
        assert_eq!(
            behavior
                .can_survive(current, POS, &LevelReaderRef::Server(&level))
                .unwrap(),
            f[3].parse::<bool>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 6600);
}
#[test]
fn attachment_support_loss_and_scaffold_updates_keep_native_state_and_tick_delays() {
    let mut matched = 0;
    for line in cases("update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let mut level = connected_level();
        mask(&mut level, f[3].parse().unwrap());
        level.set_block_state(POS, current).unwrap();
        let side = direction(f[2]);
        let behavior = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap();
        let result = behavior
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                state(f[4]),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(result.id(), f[5].parse::<u16>().unwrap(), "{line}");
        // Chunk serialization exposes the block and fluid containers separately.
        // Compare their delays without inventing a joint serialization order.
        let actual = ticks(&mut level);
        let mut actual: Vec<_> = actual.split(',').filter(|s| !s.is_empty()).collect();
        let mut expected: Vec<_> = f[6].split(',').filter(|s| !s.is_empty()).collect();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 3072);
}
#[test]
fn end_rod_pairing_and_ladder_duplicate_exclusions_match_native() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("rod\t").chain(cases("ladder\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let side = direction(f[1]);
        let rod = f[0] == "rod";
        mask(&mut level, if rod { 0 } else { 63 });
        level
            .set_block_state(
                POS,
                if rod || f[3] == "1" {
                    Block::AIR.default_state()
                } else {
                    Block::STONE.default_state()
                },
            )
            .unwrap();
        level
            .set_block_state(POS.relative(side.opposite(), 1), state(f[2]))
            .unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            side,
        )
        .unwrap();
        let actual = level
            .get_state_for_placement(if rod { Block::END_ROD } else { Block::LADDER }, &context)
            .unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[if rod { 3 } else { 4 }].parse::<i32>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 68);
}
#[test]
fn scaffold_all_below_and_four_neighbor_distance_combinations_match_native() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let scaffolds: Vec<_> = std::iter::once(Block::AIR.default_state())
        .chain((0..=7).map(|distance| {
            Block::SCAFFOLDING
                .default_state()
                .with(p::DISTANCE, distance)
                .unwrap()
        }))
        .collect();
    let mut matched = 0;
    for line in cases("scaffold\t").chain(cases("scaffold-water\t")) {
        let f: Vec<_> = line.split('\t').collect();
        mask(&mut level, 0);
        level.set_block_state(POS.below(1), state(f[1])).unwrap();
        let water = f[0] == "scaffold-water";
        level
            .set_block_state(
                POS,
                if water {
                    state(f[2])
                } else {
                    Block::AIR.default_state()
                },
            )
            .unwrap();
        if !water {
            let mut digits = f[2].parse::<usize>().unwrap();
            for side in [
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ] {
                level
                    .set_block_state(POS.relative(side, 1), scaffolds[digits % 9])
                    .unwrap();
                digits /= 9;
            }
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
            .get_state_for_placement(Block::SCAFFOLDING, &context)
            .unwrap()
            .unwrap();
        assert_eq!(actual.id(), f[3].parse::<u16>().unwrap(), "{line}");
        if !water {
            assert_eq!(
                level
                    .required_block_behavior(Block::SCAFFOLDING, BlockMethod::CanSurvive)
                    .unwrap()
                    .can_survive(
                        Block::SCAFFOLDING.default_state(),
                        POS,
                        &LevelReaderRef::Server(&level)
                    )
                    .unwrap(),
                f[4].parse::<bool>().unwrap(),
                "survive {line}"
            );
        }
        matched += 1;
    }
    assert_eq!(matched, 65660);
    for line in cases("replace\t") {
        let f: Vec<_> = line.split('\t').collect();
        let item = ItemStack::new(
            Item::from_protocol_id(f[2].parse().unwrap()).unwrap(),
            f[1].parse().unwrap(),
            Default::default(),
        );
        level
            .set_block_state(POS, Block::SCAFFOLDING.default_state())
            .unwrap();
        let context = BlockPlaceContext::new(
            &LevelReaderRef::Server(&level),
            UseOnContext::new(
                None,
                InteractionHand::MainHand,
                &item,
                PlacementHit {
                    pos: POS,
                    face: Direction::Up,
                    location: Vec3::new(8.5, 64.5, 8.5),
                    inside: false,
                },
            ),
        )
        .unwrap();
        assert_eq!(
            LevelReaderRef::Server(&level)
                .can_be_replaced(Block::SCAFFOLDING.default_state(), &context)
                .unwrap(),
            f[3].parse::<bool>().unwrap(),
            "{line}"
        );
    }
}
