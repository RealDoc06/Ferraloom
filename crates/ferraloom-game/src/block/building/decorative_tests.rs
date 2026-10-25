use super::{connected_tests::connected_level, stair_slab_tests::ticks, tests::POS};
use crate::{
    block::{
        BlockMethod, BlockPlaceContext, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef,
        PlacementHit, StackView, UseOnContext,
    },
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockState, Item, registries::sound_event::Id as SoundEvent};
use ferraloom_protocol::{
    codec::Holder,
    component::ItemStack,
    play::{InteractionHand, clientbound::Packet},
};
use ferraloom_util::{Direction, Uuid, Vec3};
use std::{collections::BTreeSet, io::Read, sync::LazyLock};
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/decorative.tsv.gz").as_slice(),
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
fn side(id: &str) -> Direction {
    Direction::VALUES[id.parse::<usize>().unwrap()]
}
#[test]
fn decorative_placement_preserves_native_stacking_and_fluid_direction_masks() {
    let mut level = connected_level();
    let item = ItemStack::default();
    let mut matched = 0;
    let mut blocks = BTreeSet::new();
    for line in cases("place\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(POS, state(f[2])).unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            side(f[3]),
            &item,
            side(f[4]),
        )
        .unwrap();
        let actual = level
            .get_state_for_placement(block, &context)
            .unwrap()
            .unwrap();
        assert_eq!(actual.id(), f[5].parse::<u16>().unwrap(), "{line}");
        matched += 1;
        blocks.insert(block);
    }
    assert_eq!(blocks.len(), 23);
    assert_eq!(matched, 18504);
}
#[test]
fn candle_and_egg_replacement_keep_secondary_use_empty_stack_and_limit_semantics() {
    let mut level = connected_level();
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
    for line in cases("replace\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let face = side(f[2]);
        for side in Direction::VALUES {
            level
                .set_block_state(POS.relative(side, 1), Block::AIR.default_state())
                .unwrap();
        }
        level.set_block_state(POS, current).unwrap();
        level
            .entity_mut::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
            .unwrap()
            .set_shared_flag(1, f[3] == "1");
        let stack = match f[4] {
            "0" => ItemStack::default(),
            "1" => ItemStack::new(current.block().as_item(), 1, Default::default()),
            "2" => ItemStack::new(current.block().as_item(), 0, Default::default()),
            "3" => ItemStack::new(Item::STONE, 1, Default::default()),
            _ => unreachable!(),
        };
        let context = BlockPlaceContext::new(
            &LevelReaderRef::Server(&level),
            UseOnContext::new(
                Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)),
                InteractionHand::MainHand,
                &stack,
                PlacementHit {
                    pos: POS,
                    face,
                    location: Vec3::new(8.5, 64.5, 8.5),
                    inside: false,
                },
            ),
        )
        .unwrap();
        let reader = LevelReaderRef::Server(&level);
        assert_eq!(
            context.replacing_clicked_on_block(&reader).unwrap(),
            f[5].parse::<bool>().unwrap(),
            "context {line}"
        );
        assert_eq!(
            reader.can_be_replaced(current, &context).unwrap(),
            f[6].parse::<bool>().unwrap(),
            "replace {line}"
        );
        assert_eq!(
            level
                .get_state_for_placement(current.block(), &context)
                .unwrap()
                .unwrap()
                .id(),
            f[7].parse::<u16>().unwrap(),
            "place {line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 13632);
}
#[test]
fn decorative_water_ticks_and_candle_center_support_match_native() {
    let mut matched = 0;
    for line in cases("update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = side(f[2]);
        let mut level = connected_level();
        level.set_block_state(POS, current).unwrap();
        let result = level
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
        assert_eq!(result.id(), f[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(ticks(&mut level), f[5], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 1836);
    let mut level = connected_level();
    let mut matched = 0;
    for line in cases("survive\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        level.set_block_state(POS.below(1), state(f[2])).unwrap();
        assert_eq!(
            level
                .required_block_behavior(current.block(), BlockMethod::CanSurvive)
                .unwrap()
                .can_survive(current, POS, &LevelReaderRef::Server(&level))
                .unwrap(),
            f[3].parse::<bool>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 1088);
}
fn sounds(level: &mut Level) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            if let LevelEffect::PlayerPacket { packet, .. } = effect
                && let Packet::Sound(sound) = *packet
            {
                let Holder::Reference(id) = sound.sound.0 else {
                    panic!("registered dried ghast sound")
                };
                Some(format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}",
                    SoundEvent::from_name(id.as_str()).unwrap().protocol_id(),
                    sound.source as i32,
                    sound.volume.to_bits(),
                    sound.pitch.to_bits(),
                    sound.seed,
                    sound.x,
                    sound.y,
                    sound.z
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn dried_ghast_post_placement_uses_state_sound_and_each_native_sound_seed() {
    let mut matched = 0;
    for line in cases("sound\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let mut level = connected_level();
        level.set_sound_seed(123);
        level.take_effects();
        let stack = ItemStack::default();
        for _ in 0..2 {
            level
                .set_placed_by(current, POS, None, StackView::Borrowed(&stack))
                .unwrap();
        }
        assert_eq!(sounds(&mut level), f[2], "{line}");
        matched += 1;
    }
    assert_eq!(matched, 32);
}
