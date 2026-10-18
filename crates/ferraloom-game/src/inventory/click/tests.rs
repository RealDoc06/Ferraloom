use super::*;
use crate::{
    entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelContext, Region},
};
use ferraloom_data::Item;
use ferraloom_util::{Uuid, Vec3};

fn describe(stack: &ItemStack) -> String {
    format!(
        "{},{},{}",
        stack.item().protocol_id(),
        count(stack),
        max_stack_size(stack)
    )
}
fn pickup_timeline<C: MenuClickContext>(
    context: &mut C,
    same: impl Fn(&C, C::Stack, C::Stack) -> bool,
) -> Result<String> {
    let hooks = &mut UnsupportedMenuHooks;
    let carried = context.carried_stack()?;
    let target = context.slot_stack(0)?;
    pickup_menu_item(context, 0, 0, hooks)?;
    let now_carried = context.carried_stack()?;
    let now_target = context.slot_stack(0)?;
    let mut events = vec![format!(
        "merge:{}:{}:{}:{}",
        same(context, carried, now_carried),
        same(context, target, now_target),
        describe(&context.stack(carried)?),
        describe(&context.stack(target)?)
    )];
    context.set_by_player(0, ItemStack::new(Item::DIRT, 2, Default::default()), hooks)?;
    pickup_menu_item(context, 0, 0, hooks)?;
    let now_carried = context.carried_stack()?;
    let now_target = context.slot_stack(0)?;
    events.push(format!(
        "swap:{}:{}:{}:{}",
        same(context, carried, now_target),
        describe(&context.stack(now_carried)?),
        describe(&context.stack(carried)?),
        describe(&context.stack(target)?)
    ));
    let amount = count(&context.stack(carried)?) + 1;
    context.set_stack_count(carried, amount)?;
    let current = context.slot_stack(0)?;
    events.push(format!("held:{}", describe(&context.stack(current)?)));
    let key = ferraloom_protocol::component::ComponentTypeId::BUNDLE_CONTENTS;
    let bundle = ItemStack::new(Item::BUNDLE, 1, Default::default());
    let mut patch = ferraloom_protocol::component::DataComponentPatch::default();
    patch.set(
        bundle
            .component(key)?
            .expect("bundle prototype contents")
            .clone(),
    );
    context.set_by_player(1, ItemStack::new(Item::STONE, 3, patch), hooks)?;
    let empty = context.own_stack(ItemStack::default())?;
    context.set_carried_stack(empty)?;
    pickup_menu_item(context, 1, 0, hooks)?;
    let carried = context.carried_stack()?;
    let carried = context.stack(carried)?;
    events.push(format!(
        "component:{}:{}",
        carried.component(key)?.is_some(),
        describe(&carried)
    ));
    Ok(events.join(";"))
}

#[test]
fn pure_and_live_pickup_share_native_cursor_and_target_identity() {
    let expected = include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .find_map(|line| line.strip_prefix("listener\tpickup\t"))
        .unwrap();
    let make_player = || {
        ServerPlayer::new(
            Uuid::new(0, 1),
            "pickup".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        )
    };
    let make_menu = || {
        let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 1 }, 7).unwrap();
        menu.local[0].set_item(0, ItemStack::new(Item::STONE, 60, Default::default()));
        menu.set_carried(ItemStack::new(Item::STONE, 10, Default::default()));
        menu
    };
    let mut player = make_player();
    let mut menu = make_menu();
    let mut pure = MenuContext::player(&mut player.inventory, &mut player.parent.equipment, false);
    {
        let mut context = super::super::in_memory::InMemoryMenuOperation::new(&mut menu, &mut pure);
        assert_eq!(
            pickup_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.read(a).unwrap(),
                context.read(b).unwrap()
            ))
            .unwrap(),
            expected
        );
    }
    assert!(pure.in_memory_stacks.forwards_are_empty());

    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let player = EntityRef(1);
    let mut state = make_player();
    let menu = state.menus.install_container_menu(make_menu());
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    with_menu_callback(player, menu, &mut level, |context| {
        let mut context = super::super::transfer::LiveMenuTransfer {
            operation: StackOperation::new(context.level),
            player,
            menu,
        };
        assert_eq!(
            pickup_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.operation.level.stack(a).unwrap(),
                context.operation.level.stack(b).unwrap()
            ))?,
            expected
        );
        Ok(())
    })
    .unwrap();
}
