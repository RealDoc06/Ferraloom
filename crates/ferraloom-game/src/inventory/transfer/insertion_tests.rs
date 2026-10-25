use super::*;
use ferraloom_data::Item;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Receiver {
    Input,
    Target,
}
struct InsertionFixture {
    source: SimpleContainer,
    target: SimpleContainer,
    input: StackSlotRef,
    old_target: StackSlotRef,
    fired: bool,
    replace: bool,
    events: Vec<String>,
}
fn describe(stack: &ItemStack) -> String {
    format!(
        "{},{},{}",
        stack.item().protocol_id(),
        count(stack),
        max_stack_size(stack)
    )
}
fn held(container: &SimpleContainer, reference: StackSlotRef) -> &ItemStack {
    let (generations, current, _) = container.stack_parts(0).unwrap();
    generations.get(reference, current).unwrap()
}
fn held_mut(container: &mut SimpleContainer, reference: StackSlotRef) -> &mut ItemStack {
    let (generations, current, _) = container.stack_parts_mut(0).unwrap();
    generations.get_mut(reference, current).unwrap()
}
impl InsertionFixture {
    fn changed(&mut self) {
        let (generations, _, _) = self.target.stack_parts(0).unwrap();
        self.events.push(format!(
            "changed:{}:{}:{}:{}",
            describe(held(&self.source, self.input)),
            describe(&self.source.get_item(0)),
            describe(&self.target.get_item(0)),
            generations.current(0) == Some(self.old_target)
        ));
        if !self.fired {
            self.fired = true;
            if self.replace {
                self.source
                    .set_item(0, ItemStack::new(Item::DIAMOND, 7, Default::default()));
                self.target
                    .set_item(0, ItemStack::new(Item::DIRT, 2, Default::default()));
                self.changed();
            }
            let value = held_mut(&mut self.source, self.input);
            let amount = count(value) + 1;
            set_count(value, amount);
        }
    }
}
impl MenuTransferContext for InsertionFixture {
    type Stack = Receiver;
    fn slot_stack(&mut self, _: usize) -> Result<Receiver> {
        Ok(Receiver::Target)
    }
    fn stack(&self, reference: Receiver) -> Result<ItemStack> {
        Ok(match reference {
            Receiver::Input => held(&self.source, self.input).clone(),
            Receiver::Target => held(&self.target, self.old_target).clone(),
        })
    }
    fn set_stack_count(&mut self, reference: Receiver, amount: i32) -> Result<()> {
        set_count(
            match reference {
                Receiver::Input => held_mut(&mut self.source, self.input),
                Receiver::Target => held_mut(&mut self.target, self.old_target),
            },
            amount,
        );
        Ok(())
    }
    fn split_stack(&mut self, reference: Receiver, amount: i32) -> Result<ItemStack> {
        assert_eq!(reference, Receiver::Input);
        Ok(split(held_mut(&mut self.source, self.input), amount))
    }
    fn stack_limit(&mut self, _: usize, stack: Receiver) -> Result<i32> {
        Ok(self
            .target
            .max_stack_size()
            .min(max_stack_size(&self.stack(stack)?)))
    }
    fn may_place(&mut self, _: usize, _: Receiver, _: &dyn MenuHooks) -> Result<bool> {
        Ok(true)
    }
    fn set_by_player(&mut self, _: usize, stack: ItemStack, _: &mut dyn MenuHooks) -> Result<()> {
        self.target.set_item(0, stack);
        self.changed();
        self.slot_changed(0)
    }
    fn reassign_by_player(
        &mut self,
        _: usize,
        stack: Receiver,
        _: &mut dyn MenuHooks,
    ) -> Result<()> {
        assert_eq!(stack, Receiver::Target);
        self.target.reassign_item(0)?;
        self.changed();
        self.slot_changed(0)
    }
    fn slot_changed(&mut self, _: usize) -> Result<()> {
        self.target.set_changed();
        self.changed();
        Ok(())
    }
}

#[test]
fn safe_insert_keeps_original_receivers_through_native_setter_callbacks() {
    let mut checked = 0;
    for line in include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .filter(|line| line.starts_with("listener\tinsert-"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let mut source =
            SimpleContainer::from_items(vec![ItemStack::new(Item::STONE, 10, Default::default())]);
        let mut target =
            SimpleContainer::from_items(vec![ItemStack::new(Item::STONE, 60, Default::default())]);
        let pin = |container: &mut SimpleContainer| {
            let (generations, _, _) = container.stack_parts_mut(0).unwrap();
            let reference = generations.current(0).unwrap();
            assert!(generations.retain(reference));
            reference
        };
        let input = pin(&mut source);
        let old_target = pin(&mut target);
        let mut context = InsertionFixture {
            source,
            target,
            input,
            old_target,
            fired: false,
            replace: fields[1] == "insert-replace",
            events: Vec::new(),
        };
        let returned = safe_insert_menu_item_stack(
            &mut context,
            0,
            Receiver::Input,
            10,
            &mut UnsupportedMenuHooks,
        )
        .unwrap();
        context.events.push(format!(
            "returned:{}:{}",
            returned == Receiver::Input,
            describe(held(&context.source, context.input))
        ));
        context.events.push(format!(
            "heldTarget:{}",
            describe(held(&context.target, context.old_target))
        ));
        assert_eq!(context.events.join(";"), fields[2], "{line}");
        assert!(
            context
                .source
                .stack_parts_mut(0)
                .unwrap()
                .0
                .release(input)
                .is_none()
        );
        assert!(
            context
                .target
                .stack_parts_mut(0)
                .unwrap()
                .0
                .release(old_target)
                .is_none()
        );
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn live_safe_insert_preserves_target_generation_and_runs_both_change_calls() {
    use crate::{
        entity::{
            EntityRef, EntityState,
            player::{GameType, ServerPlayer},
        },
        level::{Level, LevelContext, Region},
    };
    use ferraloom_util::{Uuid, Vec3};
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut state = ServerPlayer::new(
        Uuid::new(0, 1),
        "insert".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    state
        .inventory
        .set_selected_item(ItemStack::new(Item::STONE, 10, Default::default()));
    let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 1 }, 7).unwrap();
    menu.local[0].set_item(0, ItemStack::new(Item::STONE, 60, Default::default()));
    let menu = state.menus.install_container_menu(menu);
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    let mut operation = StackOperation::new(&mut level);
    let input = operation
        .pin(ContainerSlot {
            container: ContainerHandle::PlayerInventory(player),
            slot: 0,
        })
        .unwrap();
    let target = operation
        .pin(ContainerSlot {
            container: ContainerHandle::MenuLocal {
                player,
                menu,
                container: 0,
            },
            slot: 0,
        })
        .unwrap();
    assert_eq!(
        safe_insert_live_menu_item_stack(
            operation.level,
            player,
            menu,
            0,
            input,
            10,
            &mut UnsupportedMenuHooks
        )
        .unwrap(),
        input
    );
    assert_eq!(operation.level.stack_ref(target.address()).unwrap(), target);
    assert_eq!(count(operation.level.stack(input).unwrap()), 6);
    assert_eq!(count(operation.level.stack(target).unwrap()), 64);
    let MenuContainer::Simple(container) = &operation
        .level
        .entity::<ServerPlayer>(player)
        .unwrap()
        .menus
        .menu(menu)
        .unwrap()
        .local[0]
    else {
        panic!("simple chest fixture")
    };
    assert_eq!(container.times_changed, 3); // Initial set, class setItem, Slot#setChanged.
}
