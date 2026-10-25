use crate::{
    block::{BlockPlaceContext, LevelReaderRef, PlacementHit, StackView, UseOnContext},
    entity::{
        EntityState,
        player::{
            GameType, ServerPlayer,
            inventory::{count, set_count},
        },
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackValue},
};
use ferraloom_data::Item;
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{BlockPos, Direction, Uuid, Vec3};

#[test]
fn use_context_keeps_its_receiver_when_callbacks_replace_the_hand() {
    let mut level = crate::entity::tests::level(false);
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::new(0, 1),
        "item-use".into(),
        1,
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    state
        .inventory
        .set_selected_item(ItemStack::new(Item::STONE, 3, Default::default()));
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    let source = ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot: 0,
    };
    let captured;
    let context;
    {
        let mut operation = StackOperation::new(&mut level);
        captured = operation.pin(source).unwrap();
        context = UseOnContext::new_retained(
            Some(player),
            InteractionHand::MainHand,
            captured,
            PlacementHit {
                pos: BlockPos::new(8, 64, 8),
                face: Direction::Up,
                location: Vec3::new(8.5, 64.0, 8.5),
                inside: false,
            },
        );
        let placement =
            BlockPlaceContext::new(&LevelReaderRef::Server(operation.level), context.clone())
                .unwrap();
        // A synchronous callback assigns a different object to the hand, then
        // consumes from the original receiver supplied by the outer operation.
        operation
            .level
            .replace_stack_object(
                source,
                StackValue::New(ItemStack::new(Item::DIRT, 8, Default::default())),
            )
            .unwrap();
        set_count(operation.level.stack_mut(captured).unwrap(), 2);
        let reader = LevelReaderRef::Server(operation.level);
        let view = StackView::Retained(captured);
        for stack in [
            context.item_in_hand(&reader).unwrap(),
            placement.item_in_hand(&reader).unwrap(),
            view.read(&reader).unwrap(),
        ] {
            assert_eq!(stack.item(), Item::STONE);
            assert_eq!(count(stack), 2);
        }
        let current = operation
            .level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .get_main_hand_item();
        assert_eq!(current.item(), Item::DIRT);
        assert_eq!(count(current), 8);
    }
    assert!(
        context
            .item_in_hand(&LevelReaderRef::Server(&level))
            .is_err()
    );
    assert!(level.stack(captured).is_err());
}
