use super::*;
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
    menu.items(ctx)
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| format!("{i}:{}", item(s)))
        .collect::<Vec<_>>()
        .join(";")
}
fn events(updates: Vec<ContainerUpdate>) -> String {
    updates
        .into_iter()
        .map(|u| match u {
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
    fn can_smelt(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(false)
    }
    fn may_place(&self, kind: SlotKind, stack: &ItemStack) -> Result<bool> {
        if kind == SlotKind::FurnaceFuel {
            return Ok(component(stack, ComponentTypeId::COOKING_FUEL).is_some()
                || stack.0.as_ref().is_some_and(|s| s.item == Item::BUCKET));
        }
        Err(MenuError::Unsupported("oracle predicate"))
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
                for entry in f[4].split(';').filter(|s| !s.is_empty()) {
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
                    ctx.drops.iter().map(item).collect::<Vec<_>>().join(";"),
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
    assert_eq!(scenarios, 18);
    assert_eq!(clicks, 1512);
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
