use super::*;

#[test]
fn initializing_menu_is_canonical_before_it_becomes_the_current_menu() {
    let mut menus = MenuSession::default();
    let inventory = menus.current_ref();
    let opening = menus.register_container_menu(
        AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap(),
    );
    assert_eq!(menus.current_ref(), inventory);
    menus.menu_mut(opening).unwrap().carried = ItemStack::new(Item::STONE, 3, Default::default());
    // initMenu may reenter openMenu; the outer Java frame still publishes its
    // original receiver after that callback returns.
    let nested = menus.install_container_menu(
        AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap(),
    );
    menus.retain_menu(nested).unwrap();
    menus.publish_container_menu(opening).unwrap();
    menus.release_menu(opening);
    assert_eq!(menus.current_ref(), opening);
    assert_eq!(count(&menus.current().carried), 3);
    assert!(menus.menu(nested).unwrap().carried.is_empty());
    menus.release_menu(nested);
    assert!(matches!(menus.menu(nested), Err(MenuError::MissingMenu(_))));
}

#[test]
fn retained_menu_receiver_survives_replacement_with_the_same_container_id() {
    let mut menus = MenuSession::default();
    let first = menus.install_container_menu(
        AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap(),
    );
    menus.retain_menu(first).unwrap();
    let replacement = menus.install_container_menu(
        AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 7).unwrap(),
    );
    assert_ne!(first, replacement);
    menus.menu_mut(first).unwrap().carried = ItemStack::new(Item::STONE, 3, Default::default());
    assert!(menus.menu(replacement).unwrap().carried.is_empty());
    assert_eq!(
        menus.menu(first).unwrap().carried,
        ItemStack::new(Item::STONE, 3, Default::default())
    );
    menus.detach_menu(first);
    assert_eq!(menus.current_ref(), replacement);
    menus.release_menu(first);
    assert!(
        matches!(menus.menu(first), Err(MenuError::MissingMenu(reference)) if reference == first)
    );
    assert_eq!(menus.current_ref(), replacement);
}
#[test]
fn broadcast_reads_each_native_slot_once_including_when_suppressed() {
    struct CountingContainer {
        reads: std::cell::Cell<usize>,
    }
    impl Container for CountingContainer {
        fn size(&self) -> usize {
            27
        }
        fn get_item(&self, _: usize) -> ItemStack {
            self.reads.set(self.reads.get() + 1);
            ItemStack::default()
        }
        fn set_item(&mut self, _: usize, _: ItemStack) {
            unreachable!()
        }
        fn remove_item_no_update(&mut self, _: usize) -> ItemStack {
            unreachable!()
        }
    }
    let mut inventory = Inventory::default();
    let mut equipment = EntityEquipment::default();
    let mut container = CountingContainer {
        reads: std::cell::Cell::new(0),
    };
    let mut menu = AbstractContainerMenu::new(MenuKind::Chest { rows: 3 }, 1).unwrap();
    for slot in &mut menu.slots {
        if slot.container == ContainerId::Local(0) {
            slot.container = ContainerId::External(0);
        }
    }
    let mut checked = 0;
    for line in include_str!("../../tests/java/blocks/entities/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("menu-broadcast-reads\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        container.reads.set(0);
        if fields[1] == "suppressed" {
            menu.suppress_remote_updates();
        }
        {
            let mut external: [&mut dyn Container; 1] = [&mut container];
            let mut context = MenuContext::player(&mut inventory, &mut equipment, false);
            context.external = &mut external;
            menu.broadcast_changes(&context).unwrap();
        }
        assert_eq!(
            container.reads.get(),
            fields[2].parse::<usize>().unwrap(),
            "{line}"
        );
        checked += 1;
    }
    assert_eq!(checked, 2);
}
use crate::entity::player::{
    equipment::EntityEquipment,
    inventory::{Inventory, component},
};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::VarInt,
    component::{ComponentTypeId, DataComponent, DataComponentPatch, HashedStack},
    play::ContainerInput,
};

fn kind(name: &str) -> MenuKind {
    match name {
        "inventory" => MenuKind::Inventory,
        "crafter" => MenuKind::Crafter,
        "anvil" => MenuKind::Anvil,
        "beacon" => MenuKind::Beacon,
        "brewing" => MenuKind::BrewingStand,
        "crafting" => MenuKind::Crafting,
        "enchantment" => MenuKind::Enchantment,
        "grindstone" => MenuKind::Grindstone,
        "lectern" => MenuKind::Lectern,
        "loom" => MenuKind::Loom,
        "merchant" => MenuKind::Merchant,
        "smithing" => MenuKind::Smithing,
        "cartography" => MenuKind::Cartography,
        "stonecutter" => MenuKind::Stonecutter,
        "dispenser" => MenuKind::Dispenser,
        "hopper" => MenuKind::Hopper,
        "shulker" => MenuKind::ShulkerBox,
        "furnace" => MenuKind::Furnace,
        "smoker" => MenuKind::Smoker,
        "blast" => MenuKind::BlastFurnace,
        name if name.starts_with("chest") => MenuKind::Chest {
            rows: name[5..].parse().unwrap(),
        },
        _ => panic!("unknown fixture menu {name}"),
    }
}
fn item(stack: &ItemStack) -> String {
    if stack.is_empty() {
        "-".into()
    } else {
        format!(
            "{},{},{}",
            stack.0.as_ref().unwrap().item.protocol_id(),
            count(stack),
            max_stack_size(stack)
        )
    }
}
fn parse_item(value: &str) -> ItemStack {
    if value == "-" {
        return ItemStack::default();
    }
    let f: Vec<_> = value.split(',').collect();
    let item = Item::from_protocol_id(f[0].parse().unwrap()).unwrap();
    let amount = f[1].parse().unwrap();
    let max = f[2].parse().unwrap();
    let mut patch = DataComponentPatch::default();
    let base = ItemStack::new(item, 1, patch.clone());
    if max != max_stack_size(&base) {
        patch.added.push(DataComponent::MaxStackSize(VarInt(max)));
    }
    ItemStack::new(item, amount, patch)
}
fn slots(menu: &AbstractContainerMenu, ctx: &MenuContext<'_>) -> String {
    let result = menu
        .items(ctx)
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| format!("{i}:{}", item(s)))
        .collect::<Vec<_>>()
        .join(";");
    if result.is_empty() {
        "-".into()
    } else {
        result
    }
}
fn events(updates: Vec<ContainerUpdate>) -> String {
    updates
        .into_iter()
        .map(|u| match u {
            ContainerUpdate::RecipeBookAdd(packet) => format!("B:{}", packet.entries.len()),
            ContainerUpdate::Initial { state_id, .. } => format!("I:{state_id}"),
            ContainerUpdate::Slot {
                state_id,
                slot,
                stack,
                ..
            } => format!("S:{state_id}:{slot}:{}", item(&stack)),
            ContainerUpdate::Carried(stack) => format!("C:{}", item(&stack)),
            ContainerUpdate::Data { id, value, .. } => format!("D:{id}:{value}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}
// The oracle level deliberately has empty recipe property sets and is not a
// ServerLevel, so result achievements and recipes are outside this storage test.
struct OracleHooks;
impl MenuHooks for OracleHooks {
    fn prepare_recipe_click(&mut self, _kind: MenuKind) -> Result<()> {
        Ok(())
    }
    fn may_pickup_armor(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(true)
    }
    fn result_available(&self, _kind: ResultKind, _stack: &ItemStack) -> Result<bool> {
        Ok(true)
    }
    fn on_take(&mut self, _kind: ResultKind, _stack: &ItemStack, _count: i32) -> Result<()> {
        Ok(())
    }
    fn accepts_stonecutter(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(false)
    }
    fn can_smelt(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(false)
    }
    fn may_place(&self, kind: SlotKind, stack: &ItemStack) -> Result<bool> {
        if kind == SlotKind::FurnaceFuel {
            return Ok(component(stack, ComponentTypeId::COOKING_FUEL).is_some()
                || stack.0.as_ref().is_some_and(|s| s.item == Item::BUCKET));
        }
        match kind {
            SlotKind::BeaconPayment => Ok(stack.0.as_ref().is_some_and(|s| {
                matches!(
                    s.item,
                    Item::IRON_INGOT
                        | Item::GOLD_INGOT
                        | Item::DIAMOND
                        | Item::EMERALD
                        | Item::NETHERITE_INGOT
                )
            })),
            SlotKind::BrewingBottle => Ok(stack.0.as_ref().is_some_and(|s| {
                matches!(
                    s.item,
                    Item::POTION
                        | Item::SPLASH_POTION
                        | Item::LINGERING_POTION
                        | Item::GLASS_BOTTLE
                )
            })),
            SlotKind::BrewingIngredient
            | SlotKind::SmithingTemplate
            | SlotKind::SmithingBase
            | SlotKind::SmithingAddition => Ok(false),
            _ => Err(MenuError::Unsupported("oracle predicate")),
        }
    }
}

#[test]
fn real_vanilla_click_sequences_slots_carried_state_and_remote_updates() {
    let mut inventory = Inventory::default();
    let mut equipment = EntityEquipment::default();
    let mut menu = AbstractContainerMenu::new(MenuKind::Inventory, 0).unwrap();
    let mut creative = false;
    let mut clicks = 0;
    let mut scenarios = 0;
    for row in include_str!("../../tests/java/inventory/vectors.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        match f[0] {
            "start" => {
                inventory = Inventory::default();
                equipment = EntityEquipment::default();
                creative = f[3] == "true";
                let k = kind(f[2]);
                menu = AbstractContainerMenu::new(k, if k == MenuKind::Inventory { 0 } else { 7 })
                    .unwrap();
                let mut ctx = MenuContext::player(&mut inventory, &mut equipment, creative);
                for entry in f[4].split(';').filter(|s| !s.is_empty() && *s != "-") {
                    let (slot, stack) = entry.split_once(':').unwrap();
                    menu.set_item(&mut ctx, slot.parse().unwrap(), parse_item(stack));
                }
                menu.set_synchronizer(&ctx);
                menu.updates.clear();
                scenarios += 1;
            }
            "click" => {
                let mut ctx = MenuContext::player(&mut inventory, &mut equipment, creative);
                let step: usize = f[2].parse().unwrap();
                let index = f[3].parse().unwrap();
                let button = f[4].parse().unwrap();
                let input = match f[5] {
                    "0" => ContainerInput::Pickup,
                    "1" => ContainerInput::QuickMove,
                    "2" => ContainerInput::Swap,
                    "3" => ContainerInput::Clone,
                    "4" => ContainerInput::Throw,
                    "5" => ContainerInput::QuickCraft,
                    "6" => ContainerInput::PickupAll,
                    _ => unreachable!(),
                };
                menu.suppress_remote_updates();
                menu.clicked(&mut ctx, index, button, input, &mut OracleHooks)
                    .unwrap_or_else(|e| panic!("{} step {step}: {e}", f[1]));
                if step % 4 == 1 {
                    for (i, stack) in menu.items(&ctx).iter().enumerate() {
                        menu.set_remote_slot_unsafe(i as i32, HashedStack::create(stack).unwrap());
                    }
                    menu.set_remote_carried(HashedStack::create(&menu.carried).unwrap());
                }
                menu.resume_remote_updates();
                if step % 4 == 3 {
                    menu.broadcast_full_state(&ctx);
                } else {
                    menu.broadcast_changes(&ctx).unwrap();
                }
                let actual = [
                    slots(&menu, &ctx),
                    item(&menu.carried),
                    menu.state_id().to_string(),
                    events(std::mem::take(&mut menu.updates)),
                    if ctx.drops.is_empty() {
                        "-".into()
                    } else {
                        ctx.drops
                            .iter()
                            .map(|drop| item(&drop.stack))
                            .collect::<Vec<_>>()
                            .join(";")
                    },
                ];
                assert_eq!(
                    actual.iter().map(String::as_str).collect::<Vec<_>>(),
                    f[6..],
                    "{} step {step} slot {index} button {button} input {input:?}",
                    f[1]
                );
                clicks += 1;
            }
            _ => {}
        }
    }
    assert_eq!(scenarios, 33);
    assert_eq!(clicks, 2958);
}

#[test]
fn all_registered_menu_slot_layouts_match_real_vanilla_constructors() {
    let kinds = [
        MenuKind::Chest { rows: 1 },
        MenuKind::Chest { rows: 2 },
        MenuKind::Chest { rows: 3 },
        MenuKind::Chest { rows: 4 },
        MenuKind::Chest { rows: 5 },
        MenuKind::Chest { rows: 6 },
        MenuKind::Dispenser,
        MenuKind::Crafter,
        MenuKind::Anvil,
        MenuKind::Beacon,
        MenuKind::BlastFurnace,
        MenuKind::BrewingStand,
        MenuKind::Crafting,
        MenuKind::Enchantment,
        MenuKind::Furnace,
        MenuKind::Grindstone,
        MenuKind::Hopper,
        MenuKind::Lectern,
        MenuKind::Loom,
        MenuKind::Merchant,
        MenuKind::ShulkerBox,
        MenuKind::Smithing,
        MenuKind::Smoker,
        MenuKind::Cartography,
        MenuKind::Stonecutter,
    ];
    let mut layouts = 0;
    for row in include_str!("../../tests/java/inventory/vectors.tsv")
        .lines()
        .filter(|s| s.starts_with("layout\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let id: usize = f[1].parse().unwrap();
        let menu = AbstractContainerMenu::new(kinds[id], 7).unwrap();
        assert_eq!(menu.kind.registry_id().unwrap().0, id as i32);
        let actual = menu
            .slots
            .iter()
            .map(|s| format!("{},{},{}", s.container_slot, s.x, s.y))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, f[2], "layout {}", f[1]);
        layouts += 1;
    }
    assert_eq!(layouts, 25);
}

#[test]
fn remote_slot_promotes_matching_hash_and_suppression_preserves_listener_changes() {
    let mut inventory = Inventory::default();
    let mut equipment = EntityEquipment::default();
    let mut ctx = MenuContext::player(&mut inventory, &mut equipment, false);
    let mut menu = AbstractContainerMenu::new(MenuKind::Inventory, 0).unwrap();
    menu.set_synchronizer(&ctx);
    menu.updates.clear();
    let stack = ItemStack::new(Item::STONE, 5, DataComponentPatch::default());
    menu.set_item(&mut ctx, 9, stack.clone());
    menu.suppress_remote_updates();
    menu.broadcast_changes(&ctx).unwrap();
    assert!(menu.updates.is_empty());
    assert_eq!(menu.changes, vec![ContainerChange::Slot(9, stack.clone())]);
    menu.resume_remote_updates();
    menu.set_remote_slot_unsafe(9, HashedStack::create(&stack).unwrap());
    menu.broadcast_changes(&ctx).unwrap();
    assert!(menu.updates.is_empty());
    assert_eq!(menu.state_id(), 1);
    menu.set_remote_slot_unsafe(9, HashedStack::default());
    menu.broadcast_changes(&ctx).unwrap();
    assert_eq!(menu.state_id(), 2);
    for _ in 0..32766 {
        menu.increment_state_id();
    }
    assert_eq!(menu.state_id(), 0);
}

#[test]
fn real_vanilla_button_acceptance_and_lectern_data_updates() {
    let mut cases = 0;
    for row in include_str!("../../tests/java/inventory/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("button\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mut inventory = Inventory::default();
        let mut equipment = EntityEquipment::default();
        let mut context = MenuContext::player(&mut inventory, &mut equipment, false);
        let mut menu = AbstractContainerMenu::new(kind(f[1]), 7).unwrap();
        menu.set_synchronizer(&context);
        menu.updates.clear();
        let accepted = menu
            .click_menu_button(
                &mut context,
                f[2].parse().unwrap(),
                true,
                &mut UnsupportedMenuHooks,
            )
            .unwrap();
        if accepted {
            menu.broadcast_changes(&context).unwrap();
        }
        assert_eq!(accepted, f[3] == "true", "{} button {}", f[1], f[2]);
        assert_eq!(menu.state_id().to_string(), f[4]);
        let actual = events(menu.updates);
        assert_eq!(if actual.is_empty() { "-" } else { &actual }, f[5]);
        cases += 1;
    }
    assert_eq!(cases, 42);
}

#[test]
fn real_vanilla_specialized_slot_component_predicates() {
    use ferraloom_protocol::{
        Identifier,
        codec::HolderSet,
        component::{EntityTypeSet, EquipmentSlot, Unit},
    };
    let mut cases = 0;
    for row in include_str!("../../tests/java/inventory/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("slot-rule\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let armor = f[1].starts_with("armor-") || f[1] == "helmet";
        let item = if armor {
            Item::DIAMOND_HELMET
        } else {
            Item::DIAMOND_PICKAXE
        };
        let mut stack = ItemStack::new(item, 1, DataComponentPatch::default());
        if f[1] == "unbreakable" {
            stack
                .0
                .as_mut()
                .unwrap()
                .components
                .added
                .push(DataComponent::Unbreakable(Unit));
        }
        if f[1] == "removed-damage" {
            stack
                .0
                .as_mut()
                .unwrap()
                .components
                .removed
                .push(ComponentTypeId::DAMAGE);
        }
        if f[1].starts_with("armor-") {
            let mut equippable = component(&stack, ComponentTypeId::EQUIPPABLE)
                .unwrap()
                .clone();
            let DataComponent::Equippable(value) = &mut equippable else {
                unreachable!()
            };
            value.allowed_entities = Some(EntityTypeSet(HolderSet::Direct(vec![
                Identifier::parse(if f[1] == "armor-cow" {
                    "minecraft:cow"
                } else {
                    "minecraft:player"
                })
                .unwrap(),
            ])));
            stack.0.as_mut().unwrap().components.added.push(equippable);
        }
        let rule = if armor {
            SlotKind::Armor(EquipmentSlot::Head)
        } else {
            SlotKind::GrindstoneInput
        };
        let slot = Slot::new(ContainerId::Local(0), 0, 0, 0).with_kind(rule);
        assert_eq!(
            slot.may_place(&stack, &UnsupportedMenuHooks).unwrap(),
            f[2] == "true",
            "{}",
            f[1]
        );
        cases += 1;
    }
    assert_eq!(cases, 6);
}

#[derive(Default)]
struct CraftingObservations {
    stats: Vec<String>,
    awards: Vec<String>,
}
impl CraftingCallbacks for CraftingObservations {
    fn validate_take_result(&self) -> Result<()> {
        Ok(())
    }
    fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(true)
    }
    fn item_crafted(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        amount: Option<i32>,
    ) -> Result<()> {
        if let Some(amount) = amount {
            self.stats.push(format!(
                "{}:{amount}",
                stack.0.as_ref().unwrap().item.protocol_id()
            ));
        }
        Ok(())
    }
    fn recipe_crafted(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        recipe: &crate::crafting::RecipeHolder,
        _input: &[ItemStack],
    ) -> Result<()> {
        self.awards.push(recipe.id.to_string());
        Ok(())
    }
    fn recipe_unlocked(
        &mut self,
        _menu: &mut AbstractContainerMenu,
        _ctx: &mut MenuContext<'_>,
        recipe: &crate::crafting::RecipeHolder,
        book: &crate::crafting::ServerRecipeBook,
    ) -> Result<()> {
        assert!(book.contains(&recipe.id));
        Ok(())
    }
}
struct CraftingTestContext;
impl crate::crafting::CraftingContext for CraftingTestContext {}
fn crafting_manager() -> crate::crafting::RecipeManager {
    use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/home/doc/code/Ferraloom/.cache/vanilla/26.3/server.jar".into());
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(&path).unwrap()]);
    crate::crafting::RecipeManager::load(&resources, &TagSet::load(&resources).unwrap()).unwrap()
}
fn nonempty_events(events: String) -> String {
    if events.is_empty() {
        "-".into()
    } else {
        events
    }
}

#[test]
fn real_server_crafting_clicks_remainders_stats_and_packet_order() {
    use crate::crafting::{RecipeSynchronization, ServerRecipeBook};
    let manager = crafting_manager();
    let sync = RecipeSynchronization::new(&manager, |_| true);
    let mut inventory = Inventory::default();
    let mut equipment = EntityEquipment::default();
    let mut menu = AbstractContainerMenu::new(MenuKind::Inventory, 0).unwrap();
    let mut book = ServerRecipeBook::default();
    let mut callbacks = CraftingObservations::default();
    let mut clicks = 0;
    let mut placements = 0;
    for row in include_str!("../../tests/java/inventory/crafting.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        if f[0] == "start" {
            inventory = Inventory::default();
            equipment = EntityEquipment::default();
            book = ServerRecipeBook::default();
            callbacks = CraftingObservations::default();
            menu = AbstractContainerMenu::new(kind(f[2]), if f[2] == "inventory" { 0 } else { 7 })
                .unwrap();
            let mut ctx = MenuContext::player(&mut inventory, &mut equipment, false);
            menu.set_synchronizer(&ctx);
            let mut hooks = RecipeMenuHooks {
                manager: &manager,
                context: &CraftingTestContext,
                synchronization: &sync,
                book: &mut book,
                limited_crafting: Some(false),
                callbacks: &mut callbacks,
            };
            let mut expected_result = ItemStack::default();
            for entry in f[3].split(';').filter(|s| *s != "-") {
                let (index, value) = entry.split_once(':').unwrap();
                let index = index.parse().unwrap();
                if index == 0 {
                    expected_result = parse_item(value);
                } else {
                    menu.set_item_live(&mut ctx, index, parse_item(value), &mut hooks)
                        .unwrap();
                }
            }
            if f[1].starts_with("fallback") {
                menu.set_item(&mut ctx, 0, expected_result);
            }
            menu.broadcast_full_state(&ctx);
            assert_eq!(slots(&menu, &ctx), f[3], "{} setup slots", f[1]);
            assert_eq!(menu.state_id().to_string(), f[4], "{} setup state", f[1]);
            menu.updates.clear();
        } else if f[0] == "click" || f[0] == "place" {
            let mut ctx = MenuContext::player(&mut inventory, &mut equipment, false);
            let mut hooks = RecipeMenuHooks {
                manager: &manager,
                context: &CraftingTestContext,
                synchronization: &sync,
                book: &mut book,
                limited_crafting: Some(false),
                callbacks: &mut callbacks,
            };
            if f[0] == "click" {
                let input = match f[5] {
                    "0" => ContainerInput::Pickup,
                    "1" => ContainerInput::QuickMove,
                    "2" => ContainerInput::Swap,
                    "4" => ContainerInput::Throw,
                    "5" => ContainerInput::QuickCraft,
                    _ => unreachable!(),
                };
                menu.suppress_remote_updates();
                menu.clicked(
                    &mut ctx,
                    f[3].parse().unwrap(),
                    f[4].parse().unwrap(),
                    input,
                    &mut hooks,
                )
                .unwrap_or_else(|e| panic!("{} step {}: {e}", f[1], f[2]));
                menu.resume_remote_updates();
                clicks += 1;
            } else {
                let id = format!("minecraft:{}", f[3]).parse().unwrap();
                let recipe = manager.by_key(&id).unwrap();
                let action = menu
                    .handle_crafting_placement(&mut ctx, recipe, f[4] == "true", false, &mut hooks)
                    .unwrap();
                assert_eq!(
                    match action {
                        crate::crafting::PostPlaceAction::Nothing => "NOTHING",
                        crate::crafting::PostPlaceAction::PlaceGhostRecipe => "PLACE_GHOST_RECIPE",
                    },
                    f[5],
                    "{} placement {}",
                    f[1],
                    f[2]
                );
                placements += 1;
            }
            menu.broadcast_changes(&ctx).unwrap();
            let actual = [
                slots(&menu, &ctx),
                item(&menu.carried),
                menu.state_id().to_string(),
                nonempty_events(events(std::mem::take(&mut menu.updates))),
                nonempty_events(std::mem::take(&mut callbacks.stats).join(";")),
                nonempty_events(std::mem::take(&mut callbacks.awards).join(";")),
            ];
            assert_eq!(
                actual.iter().map(String::as_str).collect::<Vec<_>>(),
                f[6..],
                "{} step {} operation {} button/amount {}",
                f[1],
                f[2],
                f[3],
                f[4]
            );
        }
    }
    assert_eq!(clicks, 239);
    assert_eq!(placements, 40);
}

#[test]
fn pending_crafting_services_reject_before_mutating_live_stacks() {
    use crate::crafting::{RecipeSynchronization, ServerRecipeBook};
    let manager = crafting_manager();
    let sync = RecipeSynchronization::new(&manager, |_| true);
    let mut inventory = Inventory::default();
    let mut equipment = EntityEquipment::default();
    let mut ctx = MenuContext::player(&mut inventory, &mut equipment, true);
    let mut menu = AbstractContainerMenu::new(MenuKind::Inventory, 0).unwrap();
    let mut book = ServerRecipeBook::default();
    let mut callbacks = PendingCraftingCallbacks;
    let mut hooks = RecipeMenuHooks {
        manager: &manager,
        context: &CraftingTestContext,
        synchronization: &sync,
        book: &mut book,
        limited_crafting: Some(false),
        callbacks: &mut callbacks,
    };
    menu.set_synchronizer(&ctx);
    menu.set_item_live(
        &mut ctx,
        1,
        ItemStack::new(Item::OAK_LOG, 1, Default::default()),
        &mut hooks,
    )
    .unwrap();
    menu.updates.clear();
    let before = menu.items(&ctx);
    let state = menu.state_id();
    for (input, button) in [
        (ContainerInput::Pickup, 0),
        (ContainerInput::QuickMove, 0),
        (ContainerInput::Swap, 40),
        (ContainerInput::Throw, 0),
        (ContainerInput::Clone, 0),
    ] {
        assert!(matches!(
            menu.clicked(&mut ctx, 0, button, input, &mut hooks),
            Err(MenuError::Unsupported(_))
        ));
        assert_eq!(menu.items(&ctx), before, "{input:?}");
        assert!(menu.carried.is_empty());
        assert_eq!(menu.state_id(), state);
        assert!(menu.updates.is_empty());
    }
    hooks.limited_crafting = None;
    assert!(matches!(
        menu.set_item_live(
            &mut ctx,
            2,
            ItemStack::new(Item::OAK_LOG, 1, Default::default()),
            &mut hooks
        ),
        Err(MenuError::Unsupported(_))
    ));
    assert_eq!(menu.items(&ctx), before);
}

#[test]
fn player_tick_broadcasts_external_inventory_mutation_in_entity_phase() {
    use crate::{
        entity::{
            EntityRef, EntityState,
            player::{GameType, ServerPlayer},
        },
        level::{Level, Region},
    };
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    let handle = EntityRef(1);
    let player = ServerPlayer::new(
        ferraloom_util::Uuid::from_int_array([0, 0, 0, 1]),
        "menus".into(),
        1,
        ferraloom_util::Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(handle, EntityState::Player(Box::new(player)))
        .unwrap();
    {
        let player = level.entity_mut::<ServerPlayer>(handle).unwrap();
        let context =
            MenuContext::player(&mut player.inventory, &mut player.parent.equipment, false);
        player.menus.inventory_menu.set_synchronizer(&context);
        player.menus.inventory_menu.updates.clear();
        player.inventory.set_item(
            0,
            ItemStack::new(Item::STONE, 2, Default::default()),
            &mut player.parent.equipment,
        );
        assert!(player.menus.pending_packets.is_empty());
    }
    tick_container_menu(handle, &mut level).unwrap();
    let player = level.entity_mut::<ServerPlayer>(handle).unwrap();
    assert!(
        matches!(player.menus.pending_packets.as_slice(), [ferraloom_protocol::play::clientbound::Packet::ContainerSetSlot(packet)] if packet.slot == 36 && packet.state_id.0 == 2 && count(&packet.item_stack)==2)
    );
    player.menus.pending_packets.clear();
    tick_container_menu(handle, &mut level).unwrap();
    assert!(
        level
            .entity::<ServerPlayer>(handle)
            .unwrap()
            .menus
            .pending_packets
            .is_empty()
    );
}
