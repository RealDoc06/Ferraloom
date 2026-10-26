use super::*;
use crate::{
    block::{PlacementHit, UseContext, containers::sign::SignBlockEntity, entity},
    inventory::StackOperation,
    item::stack::tests::level,
    level::{
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
    },
};
use ferraloom_data::{Block, BlockState, Item};
use ferraloom_protocol::{
    codec::Holder,
    component::ItemStack,
    play::{InteractionHand, clientbound as cb},
};
use ferraloom_util::{
    Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::io::Read;
fn vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/honeycomb/honeycomb.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}
fn pos(value: &str) -> BlockPos {
    let v = value
        .split(',')
        .map(|n| n.parse().unwrap())
        .collect::<Vec<_>>();
    BlockPos::new(v[0], v[1], v[2])
}
fn at(value: BlockPos) -> String {
    format!("{},{},{}", value.x, value.y, value.z)
}
fn state(level: &mut Level, pos: BlockPos, state: BlockState) {
    level.set_block_state(pos, state).unwrap();
    if state.block().block_entity_type().is_some() {
        let data = entity::create(pos, state).unwrap();
        level.set_block_entity(data).unwrap();
    }
}
fn actions(level: &mut Level, count: i32) -> String {
    game_events::take(level)
        .into_iter()
        .filter_map(|event| match event {
            Observation::GameEvent {
                event,
                position,
                affected_state,
                ..
            } => Some(format!(
                "event:{}:{}:{}:{count}",
                event.name(),
                at(BlockPos::new(
                    position.x.floor() as i32,
                    position.y.floor() as i32,
                    position.z.floor() as i32
                )),
                affected_state.unwrap().id()
            )),
            Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => match *packet {
                cb::Packet::LevelEvent(value) => Some(format!(
                    "level:{}:{}:{}:{count}",
                    value.event_type,
                    at(value.pos),
                    value.data
                )),
                cb::Packet::Sound(value) => {
                    let Holder::Reference(sound) = value.sound.0 else {
                        panic!("registered sound")
                    };
                    Some(format!(
                        "sound:{sound}:{:.1},{:.1},{:.1}:{count}",
                        value.x as f64 / 8.0,
                        value.y as f64 / 8.0,
                        value.z as f64 / 8.0
                    ))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn native_wax_and_inverse_keep_every_state_property() {
    let mut count = 0;
    for line in vectors().lines().filter(|line| line.starts_with("state\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        assert_eq!(
            state.waxed().map_or(-1, |s| i32::from(s.id())),
            f[2].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            state.unwaxed().map_or(-1, |s| i32::from(s.id())),
            f[3].parse::<i32>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, ferraloom_data::BLOCK_STATES.len());
}
#[test]
fn native_honeycomb_use_keeps_context_receiver_chest_effect_order_and_rng() {
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    let mut pending_statues = 0;
    for line in vectors().lines().filter(|line| line.starts_with("use\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let old = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let separate = f[2] == "true";
        let statue = old
            .block()
            .is_instance_of(BlockClass::CopperGolemStatueBlock);
        let (mut level, player) = level();
        level
            .set_block_state(pos.below(1), Block::STONE.default_state())
            .unwrap();
        state(&mut level, pos, old);
        let original_statue = if statue {
            Some(
                level
                    .block_entity_ref(pos)
                    .unwrap()
                    .expect("real statue factory"),
            )
        } else {
            None
        };
        let neighbor = super::tests::pos(f[7]);
        if old.block().is_instance_of(BlockClass::ChestBlock)
            && old.get(p::TYPE_).is_some_and(|v| v.as_str() != "single")
        {
            level
                .set_block_state(neighbor.below(1), Block::STONE.default_state())
                .unwrap();
            let half = old.get(p::TYPE_).unwrap();
            let opposite = old
                .with(
                    p::TYPE_,
                    if half.as_str() == "left" {
                        p::Type::Right
                    } else {
                        p::Type::Left
                    },
                )
                .unwrap();
            state(&mut level, neighbor, opposite);
        }
        game_events::observe(&mut level);
        game_events::take(&mut level);
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(
                player,
                ItemStack::new(Item::HONEYCOMB, 3, Default::default()),
            )
            .unwrap();
        let held = if separate {
            operation
                .own(player, ItemStack::new(Item::DIRT, 7, Default::default()))
                .unwrap()
        } else {
            receiver
        };
        let context = UseOnContext::new_retained(
            None,
            InteractionHand::OffHand,
            held,
            PlacementHit {
                pos,
                face: Direction::Up,
                location: center(pos),
                inside: false,
            },
        );
        *operation.level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let result = crate::item::stack::use_on(receiver, &context, &mut operation);
        if statue {
            // The canonical factory is real now. Every captured statue case
            // changes its block with UPDATE_NEIGHBORS, so full waxing still
            // requires CopperGolemStatueBlock#affectNeighborsAfterRemoval.
            // Verify the native mutation prefix without counting it as a match.
            assert!(
                operation
                    .level
                    .required_block_behavior(
                        old.block(),
                        crate::block::BlockMethod::AffectNeighborsAfterRemoval
                    )
                    .is_err(),
                "remove this pending branch when the callback lands"
            );
            let error = result.expect_err("statue removal callback remains required");
            assert_eq!(
                error.root(),
                &LevelError::UnsupportedBlock(old.block()),
                "{line}"
            );
            assert_eq!(
                operation.level.block_entity_ref(pos).unwrap(),
                original_statue,
                "{line}"
            );
            assert_eq!(
                inventory::count(operation.level.stack(receiver).unwrap()),
                f[4].parse::<i32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                inventory::count(operation.level.stack(held).unwrap()),
                f[5].parse::<i32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                operation.level.get_block_state(pos).unwrap().id(),
                f[6].parse::<u16>().unwrap(),
                "{line}"
            );
            let remaining = inventory::count(operation.level.stack(held).unwrap());
            assert_eq!(
                actions(operation.level, remaining),
                "",
                "effects after the required callback must not run: {line}"
            );
            pending_statues += 1;
            continue;
        }
        let result = result.unwrap().unwrap();
        assert_eq!(
            if result.consumes_action() {
                "success"
            } else {
                "pass"
            },
            f[3],
            "{line}"
        );
        assert_eq!(
            inventory::count(operation.level.stack(receiver).unwrap()),
            f[4].parse::<i32>().unwrap(),
            "{line}"
        );
        let remaining = inventory::count(operation.level.stack(held).unwrap());
        assert_eq!(remaining, f[5].parse::<i32>().unwrap(), "{line}");
        assert_eq!(
            operation.level.get_block_state(pos).unwrap().id(),
            f[6].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            operation.level.get_block_state(neighbor).unwrap().id(),
            f[8].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(actions(operation.level, remaining), f[9], "{line}");
        assert_eq!(
            operation.level.random().next_long(),
            f[10].parse::<i64>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 302);
    assert_eq!(pending_statues, 8);
}
#[test]
fn native_honeycomb_sign_application_marks_wax_once_without_consuming() {
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in vectors().lines().filter(|line| line.starts_with("sign\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let old = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let (mut level, _) = level();
        state(&mut level, pos, old);
        let handle = level.block_entity_ref(pos).unwrap().unwrap();
        level
            .block_entity_mut::<SignBlockEntity>(handle)
            .unwrap()
            .is_waxed = f[2] == "true";
        game_events::observe(&mut level);
        game_events::take(&mut level);
        *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let applied = super::try_apply_to_sign(handle, &mut level).unwrap();
        assert_eq!(applied, f[3] == "true", "{line}");
        assert_eq!(
            level
                .block_entity::<SignBlockEntity>(handle)
                .unwrap()
                .is_waxed,
            f[4] == "true",
            "{line}"
        );
        assert_eq!(actions(&mut level, 3), f[6], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn live_sign_honeycomb_consumes_original_hand_and_keeps_other_applicators_required() {
    let (mut level, player) = level();
    let pos = BlockPos::new(8, 5, 8);
    let old = Block::OAK_SIGN.default_state();
    state(&mut level, pos, old);
    let slot =
        crate::block::interaction::hand_slot(&level, player, InteractionHand::MainHand).unwrap();
    {
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation.pin(slot).unwrap();
        let stack = operation.level.stack_mut(receiver).unwrap();
        *stack = ItemStack::new(Item::HONEYCOMB, 3, Default::default());
    }
    let context = UseContext {
        player,
        hand: InteractionHand::MainHand,
        hit: ferraloom_protocol::play::BlockHitResult {
            direction: Direction::Up,
            block_pos: pos,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    };
    let behavior = level
        .required_block_behavior(old.block(), crate::block::BlockMethod::UseItemOn)
        .unwrap();
    let result = behavior
        .use_item_on(old, pos, &context, &mut level)
        .unwrap();
    assert_eq!(result, InteractionResult::SUCCESS);
    let handle = level.block_entity_ref(pos).unwrap().unwrap();
    assert!(
        level
            .block_entity::<SignBlockEntity>(handle)
            .unwrap()
            .is_waxed
    );
    assert_eq!(
        inventory::count(
            level
                .entity::<crate::entity::player::ServerPlayer>(player)
                .unwrap()
                .get_item_in_hand(context.hand)
        ),
        2
    );
    level
        .block_entity_mut::<SignBlockEntity>(handle)
        .unwrap()
        .is_waxed = false;
    level
        .entity_mut::<crate::entity::player::ServerPlayer>(player)
        .unwrap()
        .set_item_in_hand(
            context.hand,
            ItemStack::new(Item::GLOW_INK_SAC, 3, Default::default()),
        );
    assert!(matches!(
        behavior.use_item_on(old, pos, &context, &mut level),
        Err(LevelError::UnsupportedBlockMethod(
            _,
            crate::block::BlockMethod::UseItemOn
        ))
    ));
}
