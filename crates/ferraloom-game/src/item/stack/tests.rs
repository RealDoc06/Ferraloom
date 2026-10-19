use super::*;
use crate::{
    block::{PlacementHit, UnportedBehavior},
    entity::player::{GameType, ServerPlayer},
    inventory::StackOperation,
    level::{
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
    },
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::{
    codec::Holder,
    component::{
        BlockItemStateProperties, CustomData, DataComponentPatch, ItemStack, TypedBlockEntityData,
    },
    play::{InteractionHand, clientbound as cb},
};
use ferraloom_util::{
    BlockPos, Direction, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::io::Read;

pub(in crate::item) fn level() -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let data = level.context().generator.access().data();
    let (blocks, fluids) = (data.block_tags.clone(), data.fluid_tags.clone());
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    let player = level.next_entity_ref();
    let network_id = level.next_entity_id();
    let state = ServerPlayer::new(
        Uuid::new(0, 1),
        "block-item".into(),
        network_id,
        Vec3::new(12.5, 5.0, 8.5),
        GameType::Survival,
    );
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    (level, player)
}

fn context(receiver: StackRef, face: Direction) -> UseOnContext<'static> {
    UseOnContext::new_retained(
        None,
        InteractionHand::MainHand,
        receiver,
        PlacementHit {
            pos: BlockPos::new(8, 5, 8),
            face,
            location: Vec3::new(8.5, 5.5, 8.5),
            inside: false,
        },
    )
}

fn native_vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/block-item.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}

#[test]
fn native_block_item_placement_matches_states_context_consumption_effects_and_rng() {
    let text = native_vectors();
    let (mut level, player) = level();
    game_events::observe(&mut level);
    let pos = BlockPos::new(8, 5, 8);
    let mut calls = [0; 2];
    for line in text
        .lines()
        .filter(|line| line.starts_with("place\t") || line.starts_with("plant\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let plant = f[0] == "plant";
        let item = Item::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let face = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let occupied = f[3].parse::<u8>().unwrap();
        let separate = f[4].parse::<bool>().unwrap();
        for direction in Direction::VALUES {
            level
                .set_block_state(
                    pos.relative(direction, 1),
                    if occupied == 2 {
                        Block::STONE
                    } else {
                        Block::AIR
                    }
                    .default_state(),
                )
                .unwrap();
        }
        level
            .set_block_state(
                pos,
                if occupied == 0 {
                    Block::AIR
                } else {
                    Block::STONE
                }
                .default_state(),
            )
            .unwrap();
        if plant {
            let support = BlockState::from_id(f[5].parse().unwrap()).unwrap();
            level.set_block_state(pos.below(1), support).unwrap();
            if occupied != 0 {
                level.set_block_state(pos, support).unwrap();
            }
        }
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(player, ItemStack::new(item, 3, Default::default()))
            .unwrap();
        let held = if separate {
            operation
                .own(player, ItemStack::new(Item::DIRT, 7, Default::default()))
                .unwrap()
        } else {
            receiver
        };
        if !plant && f[5] != "0" {
            operation
                .level
                .stack_mut(held)
                .unwrap()
                .0
                .as_mut()
                .unwrap()
                .components
                .added
                .push(DataComponent::BlockState(BlockItemStateProperties(vec![(
                    "north".into(),
                    if f[5] == "1" { "false" } else { "invalid" }.into(),
                )])));
        }
        game_events::take(operation.level);
        *operation.level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let result = use_on(receiver, &context(held, face), &mut operation)
            .unwrap()
            .unwrap();
        let outcome = match result {
            InteractionResult::Success {
                swing_source: crate::block::interaction::SwingSource::Predicted,
                item_context,
            } => format!(
                "success:PREDICTED:{}:{}",
                item_context.was_item_interaction,
                item_context.held_item_transformed_to.is_some()
            ),
            InteractionResult::Fail => "fail".into(),
            result => panic!("unexpected result {result:?}: {line}"),
        };
        assert_eq!(outcome, f[6], "{line}");
        assert_eq!(
            inventory::count(operation.level.stack(receiver).unwrap()),
            f[7].parse::<i32>().unwrap(),
            "receiver {line}"
        );
        assert_eq!(
            inventory::count(operation.level.stack(held).unwrap()),
            f[8].parse::<i32>().unwrap(),
            "context {line}"
        );
        for (at, expected) in [(pos, f[9]), (pos.relative(face, 1), f[10])] {
            assert_eq!(
                operation.level.get_block_state(at).unwrap().id(),
                expected.parse::<u16>().unwrap(),
                "{line}"
            );
        }
        let mut actions = Vec::new();
        for effect in game_events::take(operation.level) {
            match effect {
                Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => {
                    if let cb::Packet::Sound(sound) = packet.as_ref() {
                        let Holder::Reference(name) = &sound.sound.0 else {
                            panic!("expected registered sound")
                        };
                        actions.push(format!(
                            "sound:{name}:{}:{}",
                            sound.volume.to_bits() as i32,
                            sound.pitch.to_bits() as i32
                        ));
                    }
                }
                Observation::GameEvent {
                    event,
                    affected_state,
                    ..
                } => actions.push(format!(
                    "event:{}:{}",
                    event.name(),
                    affected_state.unwrap().id()
                )),
                _ => {}
            }
        }
        let expected: Vec<_> = f[11]
            .split(';')
            .filter(|a| *a != "entities" && !a.is_empty())
            .collect();
        assert_eq!(actions, expected, "{line}");
        assert_eq!(
            operation.level.random().next_long(),
            f[12].parse::<i64>().unwrap(),
            "{line}"
        );
        assert_eq!(operation.level.unported_behavior_stats().skipped_calls, 0);
        calls[usize::from(plant)] += 1;
    }
    assert_eq!(calls, [540, 160]);
}

#[test]
fn native_block_item_state_properties_preserve_typed_values_and_invalid_inputs() {
    let mut calls = [0; 2];
    for line in native_vectors()
        .lines()
        .filter(|line| line.starts_with("property\t") || line.starts_with("duplicate\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let duplicate = f[0] == "duplicate";
        let properties = BlockItemStateProperties(if duplicate {
            vec![("age".into(), f[2].into()), ("age".into(), f[3].into())]
        } else {
            vec![(f[2].into(), f[3].into())]
        });
        assert_eq!(
            super::super::block_item::apply_block_state(&properties, state).id(),
            f[4].parse::<u16>().unwrap(),
            "{line}"
        );
        calls[usize::from(duplicate)] += 1;
    }
    assert_eq!(calls, [3806, 72]);
}

#[test]
fn native_block_item_custom_data_merges_and_retains_the_block_entity_receiver() {
    use crate::block::{StackView, UPDATE_LIMIT, UpdateFlags, entity::BlockEntityType};
    use ferraloom_nbt::{CompoundTag, Tag};
    let (mut level, player) = level();
    let pos = BlockPos::new(8, 5, 8);
    let mut calls = 0;
    let mut original = None;
    for line in native_vectors()
        .lines()
        .filter(|line| line.starts_with("be\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let kind = f[1].parse::<usize>().unwrap();
        let block = [Block::CHEST, Block::BARREL, Block::FURNACE][kind];
        let entity_type = [
            BlockEntityType::CHEST,
            BlockEntityType::BARREL,
            BlockEntityType::FURNACE,
        ][kind];
        if f[3] == "0" {
            for block in [Block::AIR, block] {
                level
                    .set_block(
                        pos,
                        block.default_state(),
                        UpdateFlags::SKIP_ALL_SIDEEFFECTS,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
            }
            original = level.block_entity_ref(pos).unwrap();
        }
        let mut tag = CompoundTag::new();
        if matches!(f[2], "1" | "2") {
            tag.insert("CustomName", Tag::String("native".into()));
        }
        if f[2] == "3" {
            tag.insert("ignored", Tag::Int(1));
        }
        if f[2] == "4" {
            tag.insert("id", Tag::String("minecraft:furnace".into()));
        }
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(
                player,
                ItemStack::new(
                    block.as_item(),
                    3,
                    DataComponentPatch {
                        added: vec![DataComponent::BlockEntityData(TypedBlockEntityData {
                            entity_type: if f[2] == "2" {
                                BlockEntityType::SIGN
                            } else {
                                entity_type
                            },
                            tag: CustomData(Tag::Compound(tag)),
                        })],
                        removed: Vec::new(),
                    },
                ),
            )
            .unwrap();
        let result = super::super::block_item::update_custom_block_entity_tag(
            block.as_item(),
            None,
            pos,
            StackView::Retained(receiver),
            operation.level,
        )
        .unwrap();
        assert_eq!(result, f[4].parse::<bool>().unwrap(), "{line}");
        let current = operation.level.block_entity_ref(pos).unwrap();
        assert_eq!(current, original, "{line}");
        let actual = &operation
            .level
            .block_entity_state(current.unwrap())
            .unwrap()
            .container()
            .unwrap()
            .custom_name;
        assert_eq!(
            *actual,
            (!f[5].is_empty()).then(|| ferraloom_protocol::chat::Component::literal(f[5])),
            "{line}"
        );
        calls += 1;
    }
    assert_eq!(calls, 30);
}

#[test]
fn unported_item_virtual_is_required_under_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, _) = level();
        level.set_unported_behavior(policy);
        assert_eq!(
            super::super::required_behavior(Item::SHEARS, ItemMethod::MineBlock).err(),
            Some(LevelError::UnsupportedItemMethod(
                Item::SHEARS,
                ItemMethod::MineBlock
            ))
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}

#[test]
fn consume_preserves_empty_receivers_for_creative_restoration() {
    let (mut level, player) = level();
    let mut operation = StackOperation::new(&mut level);
    let receiver = operation
        .own(
            player,
            ItemStack::new(Item::STONE, 1, DataComponentPatch::default()),
        )
        .unwrap();
    consume(receiver, 1, None, &mut operation).unwrap();
    assert!(operation.level.stack(receiver).unwrap().is_empty());
    inventory::set_count(operation.level.stack_mut(receiver).unwrap(), 1);
    assert_eq!(operation.level.stack(receiver).unwrap().item(), Item::STONE);
    operation
        .level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .instabuild = true;
    consume(receiver, 1, Some(player), &mut operation).unwrap();
    assert_eq!(
        inventory::count(operation.level.stack(receiver).unwrap()),
        1
    );
}

#[test]
fn placement_includes_the_placing_player_in_the_obstruction_query() {
    let (mut level, player) = level();
    let mut operation = StackOperation::new(&mut level);
    let receiver = operation
        .own(player, ItemStack::new(Item::STONE, 3, Default::default()))
        .unwrap();
    let mut context = context(receiver, Direction::Up);
    context.player = Some(player);
    let data = operation
        .level
        .entity_mut::<crate::entity::EntityData>(player)
        .unwrap();
    data.set_pos(Vec3::new(8.5, 5.0, 8.5));
    data.blocks_building = true;
    let result = super::super::block_item::use_on(Item::STONE, &context, &mut operation)
        .unwrap()
        .unwrap();
    assert_eq!(result, InteractionResult::Fail);
    assert_eq!(
        operation
            .level
            .get_block_state(BlockPos::new(8, 5, 8))
            .unwrap(),
        Block::AIR.default_state()
    );
    assert_eq!(
        inventory::count(operation.level.stack(receiver).unwrap()),
        3
    );
}
