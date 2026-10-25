use super::*;
use crate::{
    block::BlockCollisionContext,
    entity::{
        EntityState,
        player::{GameType, ServerPlayer},
    },
};
use ferraloom_data::Block;
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{
    Aabb, BlockPos, Uuid, Vec3,
    shapes::{self, CollisionContext},
};

#[test]
fn nullable_placement_context_matches_native_constructor_capture() {
    let mut actual = Vec::new();
    for scenario in 0..8 {
        let mut level = crate::entity::tests::level(false);
        let player = if scenario == 0 {
            None
        } else {
            let id = level.next_entity_ref();
            let mut player = ServerPlayer::new(
                Uuid::new(3, 4),
                "placement".into(),
                1,
                Vec3::new(8.5, if scenario == 5 { -2.5 } else { 64.0 }, 8.5),
                GameType::Survival,
            );
            player.set_bounding_box(Aabb::new(8.0, 71.0, 8.0, 9.0, 73.0, 9.0));
            player.set_shared_flag(1, scenario == 2);
            player
                .inventory
                .set_selected_slot(if scenario == 6 { 5 } else { 0 });
            player.set_item_in_hand(
                InteractionHand::MainHand,
                if scenario == 4 {
                    ItemStack::default()
                } else {
                    ItemStack::new(
                        Item::STONE,
                        match scenario {
                            3 => 0,
                            6 => -2,
                            _ => 3,
                        },
                        Default::default(),
                    )
                },
            );
            level
                .add_entity(id, EntityState::Player(Box::new(player)))
                .unwrap();
            Some(id)
        };
        let context = BlockCollisionContext::placement_context(player, &level).unwrap();
        if scenario == 7 {
            level
                .entity_mut::<ServerPlayer>(player.unwrap())
                .unwrap()
                .set_item_in_hand(
                    InteractionHand::MainHand,
                    ItemStack::new(Item::DIRT, 1, Default::default()),
                );
        }
        let BlockCollisionContext::Entity(context) = context else {
            unreachable!()
        };
        let pos = BlockPos::new(8, 63, 8);
        actual.push(format!(
            "placement-context\t{scenario}\t{},{},{:x},{},{},{},{},{},{},{},{},{},{}",
            context.is_descending(),
            context.is_placement(),
            context.entity_bottom.to_bits(),
            context.is_holding_item(&Item::AIR),
            context.is_holding_item(&Item::STONE),
            context.is_holding_item(&Item::DIRT),
            context.always_collide_with_fluid(),
            context.can_stand_on_fluid(
                &Block::AIR.default_state().data().fluid,
                &Block::WATER.default_state().data().fluid
            ),
            context.entity.entity == player,
            context.is_above(&shapes::block(), pos, false),
            context.is_above(&shapes::block(), pos, true),
            context.is_above(&shapes::empty(), pos, false),
            context.is_above(&shapes::empty(), pos, true),
        ));
    }
    let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("placement-context\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn native_entity_context_reads_position_y_and_effective_empty_hand() {
    use crate::entity::item::item_entity::ItemEntity;
    let mut actual = Vec::new();
    for scenario in 0..8 {
        let mut level = crate::entity::tests::level(false);
        let id = level.next_entity_ref();
        let mut state = if scenario < 4 {
            let mut player = ServerPlayer::new(
                Uuid::new(3, 4),
                "context".into(),
                level.next_entity_id(),
                Vec3::ZERO,
                GameType::Survival,
            );
            player.set_item_in_hand(
                InteractionHand::MainHand,
                ItemStack::new(
                    Item::STONE,
                    if scenario == 1 { 3 } else { 0 },
                    Default::default(),
                ),
            );
            EntityState::Player(Box::new(player))
        } else {
            EntityState::Item(Box::new(ItemEntity::new(level.next_entity_id(), 0)))
        };
        let base = state.as_entity_mut();
        base.set_pos(Vec3::new(
            8.5,
            if scenario % 2 == 0 { 63.5 } else { 64.0 },
            8.5,
        ));
        base.set_bounding_box(Aabb::new(8.0, 71.0, 8.0, 9.0, 73.0, 9.0));
        base.set_shared_flag(1, scenario % 2 != 0);
        level.add_entity(id, state).unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        let context = EntityCollisionView::capture(id, &level, scenario % 3 == 0).unwrap();
        if scenario >= 4 {
            let untracked = EntityCollisionView::capture_untracked_nonliving(id, base);
            assert_eq!(
                untracked.entity_bottom.to_bits(),
                context.entity_bottom.to_bits()
            );
            assert_eq!(
                untracked.is_holding_item(&Item::AIR),
                context.is_holding_item(&Item::AIR)
            );
        }
        actual.push(format!(
            "entity-collision-context\t{scenario}\t{:x},{:x},{:x},{},{},{},{},{},{}",
            base.position.y.to_bits(),
            base.bounding_box.min_y.to_bits(),
            context.entity_bottom.to_bits(),
            context.is_descending(),
            context.is_placement(),
            context.is_holding_item(&Item::AIR),
            context.is_holding_item(&Item::STONE),
            context.always_collide_with_fluid(),
            context.is_above(&shapes::block(), BlockPos::new(8, 63, 8), false)
        ));
    }
    let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("entity-collision-context\t"))
        .collect();
    assert_eq!(actual, expected);
}
