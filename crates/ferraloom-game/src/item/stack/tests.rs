use super::*;
use crate::{
    block::UnportedBehavior,
    entity::player::{GameType, ServerPlayer},
    inventory::StackOperation,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::component::{
    BlockItemStateProperties, CustomData, DataComponentPatch, ItemStack, TypedBlockEntityData,
};
use ferraloom_util::{BlockPos, Uuid, Vec3};
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
                            entity_type: ferraloom_protocol::component::BlockEntityType(
                                if f[2] == "2" {
                                    BlockEntityType::SIGN
                                } else {
                                    entity_type
                                }
                                .name()
                                .parse()
                                .unwrap(),
                            ),
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
        .abilities
        .instabuild = true;
    consume(receiver, 1, Some(player), &mut operation).unwrap();
    assert_eq!(
        inventory::count(operation.level.stack(receiver).unwrap()),
        1
    );
}
