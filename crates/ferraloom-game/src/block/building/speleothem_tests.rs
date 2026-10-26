use super::{connected_tests::connected_level, stair_slab_tests::ticks, tests::POS};
use crate::{
    block::{
        BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef, PlacementHit, UseOnContext,
    },
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{BlockPos, Direction, Uuid, Vec3};
use std::{io::Read, sync::LazyLock};
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/speleothem.tsv.gz").as_slice(),
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
#[test]
fn speleothem_placement_keeps_native_support_fallback_and_cross_type_thickness_rules() {
    let mut level = connected_level();
    level.install_shared(Box::default());
    let item = ItemStack::default();
    level
        .add_entity(
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "building".into(),
                1,
                Vec3::new(8.5, 64.5, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    let mut matched = 0;
    for line in cases("place\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(POS, state(f[6])).unwrap();
        level.set_block_state(POS.below(1), state(f[2])).unwrap();
        level.set_block_state(POS.above(1), state(f[3])).unwrap();
        let actor = level
            .entity_mut::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
            .unwrap();
        actor.pitch = f[4].parse().unwrap();
        actor.set_shared_flag(1, f[5] == "1");
        let context = BlockPlaceContext::new(
            &LevelReaderRef::Server(&level),
            UseOnContext::new(
                Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)),
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
        let actual = level.get_state_for_placement(block, &context).unwrap();
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[7].parse::<i32>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 13824);
}
#[test]
fn speleothem_updates_keep_existing_tick_guard_support_delays_and_merged_tips() {
    let mut matched = 0;
    for line in cases("update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = Direction::VALUES[f[4].parse::<usize>().unwrap()];
        let mut level = connected_level();
        level.set_block_state(POS, current).unwrap();
        level.set_block_state(POS.below(1), state(f[2])).unwrap();
        level.set_block_state(POS.above(1), state(f[3])).unwrap();
        if f[5] == "1" {
            level
                .schedule_block_tick_after(POS, current.block(), 7)
                .unwrap();
        }
        let actual = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(actual.id(), f[6].parse::<u16>().unwrap(), "{line}");
        assert_eq!(
            level
                .required_block_behavior(current.block(), BlockMethod::CanSurvive)
                .unwrap()
                .can_survive(current, POS, &LevelReaderRef::Server(&level))
                .unwrap(),
            f[7].parse::<bool>().unwrap(),
            "survive {line}"
        );
        assert_eq!(ticks(&mut level), f[8], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 92480);
}
#[test]
fn speleothem_shapes_preserve_clamped_native_float_offset_and_dynamic_support() {
    let level = connected_level();
    let reader = LevelReaderRef::Server(&level);
    let mut matched = 0;
    for line in cases("shape\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let pos = BlockPos::new(
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
            f[4].parse().unwrap(),
        );
        let bounds = reader.shape(current, pos).unwrap().bounds();
        let actual = [
            bounds.min_x,
            bounds.min_y,
            bounds.min_z,
            bounds.max_x,
            bounds.max_y,
            bounds.max_z,
        ]
        .map(f64::to_bits);
        let expected: Vec<_> = f[5]
            .split(',')
            .map(|value| value.parse::<i64>().unwrap() as u64)
            .collect();
        assert_eq!(actual.as_slice(), expected.as_slice(), "{line}");
        assert_eq!(
            reader
                .collision_shape_without_context(current, pos)
                .unwrap()
                .bounds(),
            bounds,
            "collision {line}"
        );
        assert!(
            !reader.is_collision_shape_full_block(current, pos).unwrap(),
            "full {line}"
        );
        for side in Direction::VALUES {
            assert!(
                !reader.is_face_sturdy(current, pos, side).unwrap(),
                "support {line}"
            );
        }
        matched += 1;
    }
    assert_eq!(matched, 280);
}
