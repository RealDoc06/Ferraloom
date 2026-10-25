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
    if stack.is_empty() {
        return "-".into();
    }
    format!(
        "{},{},{}",
        stack.item().protocol_id(),
        count(stack),
        max_stack_size(stack)
    )
}

#[test]
fn outside_primary_click_moves_the_original_cursor_into_the_item_entity() {
    use crate::{
        entity::item::ItemEntity,
        stats::{Stat, custom},
    };
    for amount in [0, 7] {
        let mut level = crate::entity::tests::level(false);
        if level.shared().is_err() {
            level.install_shared(Box::default());
        }
        let player = level.next_entity_ref();
        let mut state = ServerPlayer::new(
            Uuid::new(1, 2),
            "cursor".into(),
            level.next_entity_id().unwrap(),
            Vec3::new(8.5, 64.0, 8.5),
            GameType::Survival,
        );
        let menu = state.menus.current_ref();
        state
            .menus
            .menu_mut(menu)
            .unwrap()
            .set_carried(ItemStack::new(Item::STONE, amount, Default::default()));
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        let mut outer = StackOperation::new(&mut level);
        let held = outer
            .pin(ContainerSlot {
                container: ContainerHandle::MenuCarried { player, menu },
                slot: 0,
            })
            .unwrap();
        clicked_live_menu(
            player,
            menu,
            -999,
            0,
            ferraloom_protocol::play::ContainerInput::Pickup,
            &mut UnsupportedMenuHooks,
            outer.level,
        )
        .unwrap();
        assert!(
            outer
                .level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .menu(menu)
                .unwrap()
                .carried
                .is_empty()
        );
        let items: Vec<_> = outer
            .level
            .all_entities()
            .into_iter()
            .filter(|entity| outer.level.entity::<ItemEntity>(*entity).is_ok())
            .collect();
        let stats = &outer.level.entity::<ServerPlayer>(player).unwrap().stats;
        assert_eq!(stats.get_value(Stat::Dropped(Item::STONE)), amount);
        assert_eq!(
            stats.get_value(Stat::Custom(custom::Id::DROP)),
            i32::from(amount > 0)
        );
        if amount == 0 {
            assert!(items.is_empty());
            continue;
        }
        let [entity] = items.as_slice() else {
            panic!("expected one canonical dropped item: {items:?}")
        };
        assert_eq!(
            count(outer.level.entity::<ItemEntity>(*entity).unwrap().item()),
            amount
        );
        assert!(std::ptr::eq(
            outer.level.stack(held).unwrap(),
            outer.level.entity::<ItemEntity>(*entity).unwrap().item()
        ));
        set_count(outer.level.stack_mut(held).unwrap(), 3);
        assert_eq!(
            count(outer.level.entity::<ItemEntity>(*entity).unwrap().item()),
            3
        );
        outer.level.remove_entity(*entity).unwrap();
        outer.level.finish_entity_management();
        assert_eq!(count(outer.level.stack(held).unwrap()), 3);
        drop(outer);
        level.finish_entity_management();
        assert!(level.entity_state(*entity).is_err());
    }
}

fn swap_timeline<C: MenuClickContext>(
    context: &mut C,
    same: impl Fn(&C, C::Stack, C::Stack) -> bool,
) -> Result<String> {
    let hooks = &mut UnsupportedMenuHooks;
    let source = context.own_stack(ItemStack::new(Item::STONE, 70, Default::default()))?;
    context.set_inventory_stack(0, source)?;
    let held = context.inventory_stack(0)?;
    swap_menu_item(context, 36, 0, hooks)?;
    let main = context.inventory_stack(0)?;
    let other = context.inventory_stack(1)?;
    let mut events = vec![format!(
        "self:{}:{}:{}",
        describe(&context.stack(held)?),
        describe(&context.stack(main)?),
        describe(&context.stack(other)?),
    )];
    let total = count(&context.stack(held)?) + 2;
    context.set_stack_count(held, total)?;
    events.push(format!(
        "detached:{}:{}",
        describe(&context.stack(held)?),
        describe(&context.stack(main)?),
    ));
    let source = context.own_stack(ItemStack::new(Item::STONE, 10, Default::default()))?;
    context.set_inventory_stack(0, source)?;
    context.set_by_player(0, ItemStack::new(Item::DIRT, 2, Default::default()), hooks)?;
    let source = context.inventory_stack(0)?;
    let target = context.slot_stack(0)?;
    swap_menu_item(context, 0, 0, hooks)?;
    let now_slot = context.slot_stack(0)?;
    let now_main = context.inventory_stack(0)?;
    events.push(format!(
        "alias:{}:{}",
        same(context, source, now_slot),
        same(context, target, now_main),
    ));
    for held in [source, target] {
        let total = count(&context.stack(held)?) + 1;
        context.set_stack_count(held, total)?;
    }
    events.push(format!(
        "current:{}:{}",
        describe(&context.stack(now_slot)?),
        describe(&context.stack(now_main)?),
    ));
    Ok(events.join(";"))
}
fn quickcraft_timeline<C: MenuClickContext>(
    context: &mut C,
    same: impl Fn(&C, C::Stack, C::Stack) -> bool,
) -> Result<String> {
    let hooks = &mut UnsupportedMenuHooks;
    let source = context.own_stack(ItemStack::new(Item::STONE, 10, Default::default()))?;
    context.set_carried_stack(source)?;
    for slot in [0, 1] {
        context.set_by_player(slot, ItemStack::default(), hooks)?;
    }
    let held = context.carried_stack()?;
    for (slot, button) in [(-999, 0), (0, 1), (1, 1), (-999, 2)] {
        quick_craft_menu_items(context, slot, button, hooks)?;
    }
    let current = context.carried_stack()?;
    let mut events = vec![format!(
        "copy:{}:{}:{}",
        same(context, held, current),
        describe(&context.stack(held)?),
        describe(&context.stack(current)?),
    )];
    for reference in [held, current] {
        let total = count(&context.stack(reference)?) + 2;
        context.set_stack_count(reference, total)?;
    }
    events.push(format!(
        "held:{}:{}",
        describe(&context.stack(held)?),
        describe(&context.stack(current)?),
    ));
    let first = context.slot_stack(0)?;
    let second = context.slot_stack(1)?;
    events.push(format!(
        "targets:{}:{}",
        describe(&context.stack(first)?),
        describe(&context.stack(second)?),
    ));
    Ok(events.join(";"))
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
    let expected_swap = include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .find_map(|line| line.strip_prefix("listener\tswap\t"))
        .unwrap();
    let expected_quickcraft = include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .find_map(|line| line.strip_prefix("listener\tquickcraft\t"))
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
        assert_eq!(
            swap_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.read(a).unwrap(),
                context.read(b).unwrap()
            ))
            .unwrap(),
            expected_swap
        );
        assert_eq!(
            quickcraft_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.read(a).unwrap(),
                context.read(b).unwrap()
            ))
            .unwrap(),
            expected_quickcraft
        );
    }
    assert!(pure.in_memory_stacks.forwards_are_empty());

    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = {
        let mut fixture_level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        fixture_level.install_shared(Box::default());
        fixture_level
    };
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
        assert_eq!(
            swap_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.operation.level.stack(a).unwrap(),
                context.operation.level.stack(b).unwrap()
            ))?,
            expected_swap
        );
        assert_eq!(
            quickcraft_timeline(&mut context, |context, a, b| std::ptr::eq(
                context.operation.level.stack(a).unwrap(),
                context.operation.level.stack(b).unwrap()
            ))?,
            expected_quickcraft
        );
        Ok(())
    })
    .unwrap();
}
