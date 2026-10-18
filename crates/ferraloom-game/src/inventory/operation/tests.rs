use super::*;
use crate::{
    Behaviors,
    entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelContext, Region},
};
use ferraloom_data::Item;
use ferraloom_util::{Uuid, Vec3};

struct ReentrantListeners<'a> {
    live: LiveMenuContext<'a>,
    replace: bool,
    nested: bool,
    events: Vec<String>,
    packets: Vec<String>,
}
fn describe(stack: &ItemStack) -> String {
    format!(
        "{},{},{}",
        stack.item().protocol_id(),
        count(stack),
        max_stack_size(stack)
    )
}
impl ReentrantListeners<'_> {
    fn describe_ref(&self, reference: StackRef) -> String {
        describe(self.live.level.stack(reference).unwrap())
    }
}
impl MenuSynchronizationContext for ReentrantListeners<'_> {
    type Receiver = StackRef;
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.live.menu()
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        self.live.menu_mut()
    }
    fn capture_slot(&mut self, slot: usize) -> Result<StackRef> {
        self.live.capture_slot(slot)
    }
    fn capture_carried(&mut self) -> Result<StackRef> {
        self.live.capture_carried()
    }
    fn current_stack<'a>(&'a self, receiver: &'a StackRef) -> Result<&'a ItemStack> {
        self.live.level.stack(*receiver)
    }
    fn release_current(&mut self, receiver: StackRef) {
        self.live.release_current(receiver);
    }
    fn listener_copies(&self) -> Result<&ListenerCopyArena> {
        self.live.listener_copies()
    }
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena> {
        self.live.listener_copies_mut()
    }
    fn data_at(&mut self, index: usize) -> Result<i32> {
        self.live.data_at(index)
    }
    fn slot_changed(&mut self, index: usize, copy: ListenerCopyRef) -> Result<()> {
        if index != 36 {
            return Ok(());
        }
        let changed = StackRef {
            container: ContainerHandle::MenuListenerCopies {
                player: self.live.player,
            },
            slot: copy.slot,
            generation: copy.generation,
        };
        let slot = self.live.slot_address(index)?;
        let current = self.live.level.stack_ref(slot)?;
        self.events.push(format!(
            "A:{}:{}",
            self.describe_ref(changed),
            changed == current
        ));
        if !self.nested {
            self.nested = true;
            if self.replace {
                self.live.level.replace_stack_object(
                    slot,
                    StackValue::New(ItemStack::new(Item::DIRT, 2, Default::default())),
                )?;
            } else {
                let stack = self.live.level.stack_mut(current)?;
                let amount = count(stack) + 1;
                set_count(stack, amount);
            }
            broadcast_menu_changes(self)?;
            let stack = self.live.level.stack_mut(changed)?;
            let amount = count(stack) + 3;
            set_count(stack, amount);
            self.events
                .push(format!("held:{}", self.describe_ref(changed)));
        }
        self.events
            .push(format!("B:{}", self.describe_ref(changed)));
        Ok(())
    }
    fn send_update(&mut self, update: ContainerUpdate) -> Result<()> {
        if let ContainerUpdate::Slot {
            state_id,
            slot,
            stack,
            ..
        } = update
        {
            self.packets
                .push(format!("S:{state_id}:{slot}:{}", describe(&stack)));
        }
        Ok(())
    }
}

#[test]
fn retained_broadcast_receivers_and_memoized_copies_match_native_reentry() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut checked = 0;
    for line in include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .filter(|line| {
            line.starts_with("listener\treplace\t") || line.starts_with("listener\tmutate\t")
        })
    {
        let fields: Vec<_> = line.split('\t').collect();
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        let player = EntityRef(1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "listener".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let menus = &mut level.entity_mut::<ServerPlayer>(player).unwrap().menus;
        let menu = menus.current_ref();
        menus.bind_listener_copies(menu).unwrap();
        let mut context = ReentrantListeners {
            live: LiveMenuContext {
                level: &mut level,
                player,
                menu,
            },
            replace: fields[1] == "replace",
            nested: false,
            events: Vec::new(),
            packets: Vec::new(),
        };
        set_menu_synchronizer(&mut context).unwrap();
        context.packets.clear();
        let slot = context.live.slot_address(36).unwrap();
        context
            .live
            .level
            .replace_stack_object(
                slot,
                StackValue::New(ItemStack::new(Item::STONE, 5, Default::default())),
            )
            .unwrap();
        broadcast_menu_changes(&mut context).unwrap();
        broadcast_menu_changes(&mut context).unwrap();
        assert_eq!(
            context.events.join(";"),
            fields[2],
            "{} listeners",
            fields[1]
        );
        assert_eq!(
            context.packets.join(";"),
            fields[3],
            "{} packets",
            fields[1]
        );
        let current = context.live.level.stack_ref(slot).unwrap();
        assert_eq!(
            context.describe_ref(current),
            fields[4],
            "{} current",
            fields[1]
        );
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn menu_transfer_keeps_the_native_copy_alias_when_either_cache_is_replaced() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut checked = 0;
    for line in include_str!("../../../tests/java/inventory/listeners.tsv")
        .lines()
        .filter(|line| line.starts_with("listener\talias-"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        let player = EntityRef(1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "alias".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let menus = &mut level.entity_mut::<ServerPlayer>(player).unwrap().menus;
        let target = menus.inventory_menu.reference();
        let source = menus
            .install_container_menu(AbstractContainerMenu::new(MenuKind::Inventory, 7).unwrap());
        menus.bind_listener_copies(source).unwrap();
        let address = ContainerSlot {
            container: ContainerHandle::PlayerInventory(player),
            slot: 0,
        };
        level
            .replace_stack_object(
                address,
                StackValue::New(ItemStack::new(Item::STONE, 5, Default::default())),
            )
            .unwrap();
        with_menu_callback(player, source, &mut level, |context| {
            broadcast_menu_changes(context)
        })
        .unwrap();
        let copy = level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .menu(source)
            .unwrap()
            .last_slots[36]
            .unwrap();
        let held = StackRef {
            container: ContainerHandle::MenuListenerCopies { player },
            slot: copy.slot,
            generation: copy.generation,
        };
        level.retain_stack(held).unwrap();
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .menus
            .transfer_to_inventory_menu(source)
            .unwrap();
        let get_copy = |level: &Level, menu| {
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .menu(menu)
                .unwrap()
                .last_slots[36]
                .unwrap()
        };
        let value = |level: &Level, menu| {
            let copy = get_copy(level, menu);
            describe(
                level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .menus
                    .listener_copies
                    .get(copy)
                    .unwrap(),
            )
        };
        let mut events = vec![format!("alias:{}", copy == get_copy(&level, target))];
        set_count(level.stack_mut(held).unwrap(), 8);
        events.push(format!(
            "shared:{}:{}",
            value(&level, source),
            value(&level, target)
        ));
        if fields[1] == "alias-source" {
            level
                .replace_stack_object(
                    address,
                    StackValue::New(ItemStack::new(Item::DIRT, 2, Default::default())),
                )
                .unwrap();
            with_menu_callback(player, source, &mut level, |context| {
                broadcast_menu_changes(context)
            })
            .unwrap();
        } else {
            with_menu_callback(player, target, &mut level, |context| {
                broadcast_menu_changes(context)
            })
            .unwrap();
        }
        events.push(format!(
            "owners:{}:{}",
            copy == get_copy(&level, source),
            copy == get_copy(&level, target)
        ));
        set_count(level.stack_mut(held).unwrap(), 9);
        events.push(format!("held:{}", describe(level.stack(held).unwrap())));
        events.push(format!(
            "final:{}:{}",
            value(&level, source),
            value(&level, target)
        ));
        assert_eq!(events.join(";"), fields[2], "{}", fields[1]);
        level.release_stack(held);
        checked += 1;
    }
    assert_eq!(checked, 2);
}

struct CrafterListener<'a> {
    live: LiveMenuContext<'a>,
    events: Vec<String>,
}
impl MenuSynchronizationContext for CrafterListener<'_> {
    type Receiver = StackRef;
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.live.menu()
    }
    fn menu_mut(&mut self) -> Result<&mut AbstractContainerMenu> {
        self.live.menu_mut()
    }
    fn capture_slot(&mut self, slot: usize) -> Result<StackRef> {
        self.live.capture_slot(slot)
    }
    fn capture_carried(&mut self) -> Result<StackRef> {
        self.live.capture_carried()
    }
    fn current_stack<'a>(&'a self, receiver: &'a StackRef) -> Result<&'a ItemStack> {
        self.live.current_stack(receiver)
    }
    fn release_current(&mut self, receiver: StackRef) {
        self.live.release_current(receiver);
    }
    fn listener_copies(&self) -> Result<&ListenerCopyArena> {
        self.live.listener_copies()
    }
    fn listener_copies_mut(&mut self) -> Result<&mut ListenerCopyArena> {
        self.live.listener_copies_mut()
    }
    fn data_at(&mut self, index: usize) -> Result<i32> {
        self.live.data_at(index)
    }
    fn internal_slot_changed(&mut self, index: usize) -> Result<()> {
        self.live.internal_slot_changed(index)
    }
    fn slot_changed(&mut self, index: usize, copy: ListenerCopyRef) -> Result<()> {
        let copy = self.listener_copies()?.get(copy).unwrap();
        let output = self.live.level.stack_ref(self.live.slot_address(45)?)?;
        self.events.push(format!(
            "{index}:{}:{}",
            short_item(copy),
            short_item(self.live.level.stack(output)?)
        ));
        Ok(())
    }
}
fn short_item(stack: &ItemStack) -> String {
    if stack.is_empty() {
        "-".into()
    } else {
        describe(stack)
    }
}

#[test]
fn crafter_internal_listener_refreshes_before_later_listeners_and_inside_slot_setter() {
    let mut level = crate::entity::tests::level(false);
    crate::level::drop_fixtures::install(&mut level);
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::new(1, 2),
        "crafter".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    let mut menu = AbstractContainerMenu::new(MenuKind::Crafter, 7).unwrap();
    menu.crafter_listener = true;
    let menu = state.menus.install_container_menu(menu);
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    with_menu_callback(player, menu, &mut level, |live| {
        let mut listener = CrafterListener {
            live: LiveMenuContext {
                level: live.level,
                player,
                menu,
            },
            events: Vec::new(),
        };
        let input = listener.live.slot_address(0)?;
        listener.live.level.replace_stack_object(
            input,
            StackValue::New(ItemStack::new(Item::OAK_LOG, 1, Default::default())),
        )?;
        broadcast_menu_changes(&mut listener)?;
        let result = listener.live.slot_address(45)?;
        let mut operation = StackOperation::new(listener.live.level);
        let held = operation.pin(result)?;
        listener
            .events
            .push(format!("set:{}", short_item(operation.level.stack(held)?)));
        operation.level.replace_stack_object(
            ContainerSlot {
                container: ContainerHandle::PlayerInventory(player),
                slot: 0,
            },
            StackValue::New(ItemStack::new(Item::DIRT, 1, Default::default())),
        )?;
        // The result is held across a second native broadcast, whose inventory
        // listener replaces it even though the crafting grid did not change.
        let mut nested = CrafterListener {
            live: LiveMenuContext {
                level: operation.level,
                player,
                menu,
            },
            events: Vec::new(),
        };
        broadcast_menu_changes(&mut nested)?;
        listener.events.append(&mut nested.events);
        let current = nested.live.level.stack_ref(result)?;
        listener.events.push(format!(
            "refresh:{}:{}",
            std::ptr::eq(
                nested.live.level.stack(held)?,
                nested.live.level.stack(current)?
            ),
            short_item(nested.live.level.stack(held)?)
        ));
        let expected = include_str!("../../../tests/java/inventory/listeners.tsv")
            .lines()
            .find_map(|line| line.strip_prefix("listener\tcrafter-internal\t"))
            .unwrap();
        assert_eq!(listener.events.join(";"), expected);
        drop(operation);
        let mut transfer = super::super::transfer::LiveMenuTransfer {
            player,
            menu,
            operation: StackOperation::new(listener.live.level),
        };
        transfer.set_by_player(
            0,
            ItemStack::new(Item::DIRT, 1, Default::default()),
            &mut UnsupportedMenuHooks,
        )?;
        drop(transfer);
        // CrafterSlot#setChanged has refreshed the result before the setter
        // returns, independently of a later handler broadcast.
        let output = listener.live.level.stack_ref(result)?;
        let actual = format!("empty:{}", short_item(listener.live.level.stack(output)?));
        let expected = include_str!("../../../tests/java/inventory/listeners.tsv")
            .lines()
            .find_map(|line| line.strip_prefix("listener\tcrafter-slot\t"))
            .unwrap();
        assert_eq!(actual, expected);
        Ok(())
    })
    .unwrap();
}
