//! Ordinary crafting contract checks through real menus and the registered `PlayerFixture`.
//! These assert bounded local behavior, not complete Java crafting/statistics parity.

use crate::data::player_server::test::{PlayerFixture, close_fixture, player_fixture};
use pumpkin_data::data_component_impl::{CustomDataImpl, UseRemainderImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::statistic::StatisticCategory;
use pumpkin_inventory::crafting::crafting_screen_handler::CraftingTableScreenHandler;
use pumpkin_inventory::inventory::Clearable;
use pumpkin_inventory::screen_handler::SharedScreenHandler;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::java::server::play::SlotActionType;
use pumpkin_util::GameMode;
use std::sync::{Arc, Mutex};

fn stack(item: &'static Item, count: u8) -> ItemStack {
    ItemStack::new(count, item)
}

fn tagged(item: &'static Item, count: u8) -> ItemStack {
    let mut data = NbtCompound::new();
    data.put_int("crafting_probe", 17);
    let mut result = stack(item, count);
    result.set_data_component(CustomDataImpl::new(data));
    result
}

fn assert_stack(actual: &ItemStack, expected: &ItemStack) {
    if expected.is_empty() {
        assert!(actual.is_empty(), "expected an empty stack");
    } else {
        assert_eq!(actual.item_count, expected.item_count);
        assert!(
            actual.are_items_and_components_equal(expected),
            "item or components differ"
        );
    }
}

fn assert_stacks(actual: &[ItemStack], expected: &[ItemStack]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual.is_empty(), expected.is_empty(), "slot {index}");
        assert_stack(actual, expected);
    }
}

fn empty_inventory() -> Vec<ItemStack> {
    vec![ItemStack::EMPTY.clone(); 41]
}

fn full_inventory() -> Vec<ItemStack> {
    let mut items = empty_inventory();
    items[..36].fill(stack(&Item::STONE, 64));
    items
}

fn dropped_stacks(fixture: &PlayerFixture) -> Vec<ItemStack> {
    fixture
        .player
        .world()
        .entities
        .load()
        .iter()
        .filter_map(|entity| entity.get_item_entity())
        .map(|entity| entity.get_item_stack().lock().unwrap().clone())
        .collect()
}

/// A menu driver only: ingredients, expected results and accounting remain explicit in tests.
/// Reuses the existing fixture lifecycle and the player's registered 2x2 handler.
struct CraftCase<'a> {
    fixture: &'a PlayerFixture,
    menu: SharedScreenHandler,
    width: usize,
    output: &'static Item,
    crafted_before: i32,
    dropped_before: usize,
}

impl<'a> CraftCase<'a> {
    fn new(fixture: &'a PlayerFixture, width: usize, output: &'static Item) -> Self {
        fixture.player.inventory.clear();
        fixture.player.inventory.set_selected_slot(0);
        let menu: SharedScreenHandler = if width == 2 {
            fixture.player.player_screen_handler.clone()
        } else {
            assert_eq!(width, 3);
            Arc::new(Mutex::new(CraftingTableScreenHandler::new(
                1,
                &fixture.player.inventory,
                None,
            )))
        };
        {
            let mut handler = menu.lock().unwrap();
            for slot in &handler.get_behaviour().slots[..=width * width] {
                slot.set_stack(ItemStack::EMPTY.clone());
            }
            *handler.get_behaviour().cursor_stack.lock().unwrap() = ItemStack::EMPTY.clone();
            handler.send_content_updates();
        };
        Self {
            fixture,
            menu,
            width,
            output,
            crafted_before: fixture
                .player
                .get_stat(StatisticCategory::Crafted, i32::from(output.id)),
            dropped_before: dropped_stacks(fixture).len(),
        }
    }

    fn input(&self, index: usize, value: ItemStack) {
        assert!(index < self.width * self.width);
        self.menu.lock().unwrap().get_behaviour().slots[index + 1].set_stack(value);
    }

    fn sync(&self) {
        self.menu.lock().unwrap().send_content_updates();
    }

    fn cursor(&self, value: ItemStack) {
        *self
            .menu
            .lock()
            .unwrap()
            .get_behaviour()
            .cursor_stack
            .lock()
            .unwrap() = value;
    }

    fn inventory(&self, values: &[ItemStack]) {
        assert_eq!(values.len(), 41);
        for (index, value) in values.iter().enumerate() {
            self.fixture.player.inventory.set_slot(index, value.clone());
        }
    }

    fn click_slot(&self, slot: i32, button: i32, action: SlotActionType) {
        self.menu
            .lock()
            .unwrap()
            .on_slot_click(slot, button, action, self.fixture.player.as_ref());
        // Deliberately no send_content_updates here: repeated extraction and the next result
        // must already be correct when the click returns.
    }

    fn click(&self, button: i32, action: SlotActionType) {
        self.click_slot(0, button, action);
    }

    fn assert_grid(&self, occupied: &[(usize, ItemStack)]) {
        let mut expected = vec![ItemStack::EMPTY.clone(); self.width * self.width];
        for (index, value) in occupied {
            expected[*index] = value.clone();
        }
        let handler = self.menu.lock().unwrap();
        let actual: Vec<_> = handler.get_behaviour().slots[1..=self.width * self.width]
            .iter()
            .map(|slot| slot.get_stack())
            .collect();
        assert_stacks(&actual, &expected);
    }

    fn assert_inventory(&self, expected: &[ItemStack]) {
        let actual: Vec<_> = (0..41)
            .map(|i| self.fixture.player.inventory.get_slot(i))
            .collect();
        assert_stacks(&actual, expected);
    }

    fn assert_cursor(&self, expected: &ItemStack) {
        assert_stack(
            &self
                .menu
                .lock()
                .unwrap()
                .get_behaviour()
                .cursor_stack
                .lock()
                .unwrap(),
            expected,
        );
    }

    fn assert_result(&self, expected: &ItemStack) {
        assert_stack(
            &self.menu.lock().unwrap().get_behaviour().slots[0].get_stack(),
            expected,
        );
    }

    fn assert_drops(&self, expected: &[ItemStack]) {
        assert_stacks(
            &dropped_stacks(self.fixture)[self.dropped_before..],
            expected,
        );
    }

    fn assert_crafted(&self, amount: i32) {
        assert_eq!(
            self.fixture
                .player
                .get_stat(StatisticCategory::Crafted, i32::from(self.output.id))
                - self.crafted_before,
            amount,
            "existing crafted-stat callback must run once per successful take"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_pickup_returns_whole_output_and_fresh_positioned_remainder() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for count in [1, 2] {
            for button in [0, 1] {
                let case = CraftCase::new(&fixture, width, &Item::SUGAR);
                let index = width * width - 1;
                case.input(index, tagged(&Item::HONEY_BOTTLE, count));
                case.sync();
                case.assert_result(&stack(&Item::SUGAR, 3));
                case.click(button, SlotActionType::Pickup);
                case.assert_cursor(&stack(&Item::SUGAR, 3));
                let mut inventory = empty_inventory();
                if count == 1 {
                    case.assert_grid(&[(index, stack(&Item::GLASS_BOTTLE, 1))]);
                    case.assert_result(ItemStack::EMPTY);
                } else {
                    case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 1))]);
                    inventory[0] = stack(&Item::GLASS_BOTTLE, 1);
                    case.assert_result(&stack(&Item::SUGAR, 3));
                }
                case.assert_inventory(&inventory);
                case.assert_drops(&[]);
                case.assert_crafted(3);
            }
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_cursor_capacity_rejects_partial_output_without_consumption() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for button in [0, 1] {
            for cursor in [stack(&Item::SUGAR, 62), stack(&Item::STONE, 1)] {
                let case = CraftCase::new(&fixture, width, &Item::SUGAR);
                let index = width * width - 1;
                case.input(index, tagged(&Item::HONEY_BOTTLE, 2));
                case.sync();
                case.cursor(cursor.clone());
                case.click(button, SlotActionType::Pickup);
                case.assert_cursor(&cursor);
                case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 2))]);
                case.assert_result(&stack(&Item::SUGAR, 3));
                case.assert_inventory(&empty_inventory());
                case.assert_drops(&[]);
                case.assert_crafted(0);

                case.cursor(stack(&Item::SUGAR, 61));
                case.click(button, SlotActionType::Pickup);
                case.assert_cursor(&stack(&Item::SUGAR, 64));
                case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 1))]);
                case.assert_result(&stack(&Item::SUGAR, 3));
                let mut inventory = empty_inventory();
                inventory[0] = stack(&Item::GLASS_BOTTLE, 1);
                case.assert_inventory(&inventory);
                case.assert_drops(&[]);
                case.assert_crafted(3);
            }
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn four_honey_returns_one_bottle_per_ingredient_at_original_grid_slots() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for count in [1, 2] {
            let case = CraftCase::new(&fixture, width, &Item::HONEY_BLOCK);
            let indices = if width == 2 {
                [0, 1, 2, 3]
            } else {
                [4, 5, 7, 8]
            };
            for index in indices {
                case.input(index, tagged(&Item::HONEY_BOTTLE, count));
            }
            case.sync();
            case.assert_result(&stack(&Item::HONEY_BLOCK, 1));
            case.click(0, SlotActionType::Pickup);
            case.assert_cursor(&stack(&Item::HONEY_BLOCK, 1));
            let mut inventory = empty_inventory();
            let remaining = if count == 1 {
                case.assert_result(ItemStack::EMPTY);
                stack(&Item::GLASS_BOTTLE, 1)
            } else {
                inventory[0] = stack(&Item::GLASS_BOTTLE, 4);
                case.assert_result(&stack(&Item::HONEY_BLOCK, 1));
                tagged(&Item::HONEY_BOTTLE, 1)
            };
            case.assert_grid(&indices.map(|index| (index, remaining.clone())));
            case.assert_inventory(&inventory);
            case.assert_drops(&[]);
            case.assert_crafted(1);
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cake_returns_three_fresh_buckets_from_one_craft() {
    let fixture = player_fixture().await;
    for button in [0, 1] {
        let case = CraftCase::new(&fixture, 3, &Item::CAKE);
        for index in 0..3 {
            case.input(index, tagged(&Item::MILK_BUCKET, 1));
            case.input(index + 6, stack(&Item::WHEAT, 1));
        }
        case.input(3, stack(&Item::SUGAR, 1));
        case.input(4, stack(&Item::EGG, 1));
        case.input(5, stack(&Item::SUGAR, 1));
        case.sync();
        case.assert_result(&stack(&Item::CAKE, 1));
        case.click(button, SlotActionType::Pickup);
        case.assert_cursor(&stack(&Item::CAKE, 1));
        case.assert_grid(&[0, 1, 2].map(|index| (index, stack(&Item::BUCKET, 1))));
        case.assert_result(ItemStack::EMPTY);
        case.assert_inventory(&empty_inventory());
        case.assert_drops(&[]);
        case.assert_crafted(1);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_shift_click_repeats_until_ingredients_are_depleted() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        let case = CraftCase::new(&fixture, width, &Item::SUGAR);
        let index = width * width - 1;
        case.input(index, stack(&Item::HONEY_BOTTLE, 4));
        case.sync();
        case.click(0, SlotActionType::QuickMove);
        case.assert_cursor(ItemStack::EMPTY);
        case.assert_grid(&[(index, stack(&Item::GLASS_BOTTLE, 1))]);
        case.assert_result(ItemStack::EMPTY);
        let mut inventory = empty_inventory();
        inventory[8] = stack(&Item::SUGAR, 12);
        inventory[0] = stack(&Item::GLASS_BOTTLE, 3);
        case.assert_inventory(&inventory);
        case.assert_drops(&[]);
        case.assert_crafted(12);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_shift_click_full_and_partial_inventory_accounts_for_every_item() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for (room, consumed) in [(0, 0), (1, 1), (4, 2)] {
            let case = CraftCase::new(&fixture, width, &Item::SUGAR);
            let index = width * width - 1;
            let mut inventory = full_inventory();
            inventory[8] = stack(&Item::SUGAR, 64 - room);
            case.inventory(&inventory);
            case.input(index, stack(&Item::HONEY_BOTTLE, 3));
            case.sync();
            case.click(0, SlotActionType::QuickMove);
            case.assert_cursor(ItemStack::EMPTY);
            case.assert_result(&stack(&Item::SUGAR, 3));
            case.assert_grid(&[(index, stack(&Item::HONEY_BOTTLE, 3 - consumed))]);
            inventory[8] = stack(&Item::SUGAR, 64);
            case.assert_inventory(&inventory);
            let mut dropped = vec![stack(&Item::GLASS_BOTTLE, 1); usize::from(consumed)];
            if room != 0 {
                dropped.push(stack(&Item::SUGAR, 2));
            }
            case.assert_drops(&dropped);
            // Preserve the existing callback's moved-output count on partial insertion.
            // Java's removeCount/achievement accounting is a separate, unimplemented contract.
            case.assert_crafted(i32::from(room));
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_number_key_moves_output_to_empty_hotbar_once() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        let case = CraftCase::new(&fixture, width, &Item::SUGAR);
        let index = width * width - 1;
        case.input(index, stack(&Item::HONEY_BOTTLE, 2));
        case.sync();
        case.click(3, SlotActionType::Swap);
        case.assert_cursor(ItemStack::EMPTY);
        case.assert_grid(&[(index, stack(&Item::HONEY_BOTTLE, 1))]);
        case.assert_result(&stack(&Item::SUGAR, 3));
        let mut inventory = empty_inventory();
        inventory[3] = stack(&Item::SUGAR, 3);
        inventory[0] = stack(&Item::GLASS_BOTTLE, 1);
        case.assert_inventory(&inventory);
        case.assert_drops(&[]);
        case.assert_crafted(3);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn honey_throw_zero_consumes_once_and_throw_one_finishes() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for button in [0, 1] {
            let case = CraftCase::new(&fixture, width, &Item::SUGAR);
            let index = width * width - 1;
            case.input(index, stack(&Item::HONEY_BOTTLE, 3));
            case.sync();
            case.click(button, SlotActionType::Throw);
            case.assert_cursor(ItemStack::EMPTY);
            let mut inventory = empty_inventory();
            if button == 0 {
                case.assert_grid(&[(index, stack(&Item::HONEY_BOTTLE, 2))]);
                case.assert_result(&stack(&Item::SUGAR, 3));
                inventory[0] = stack(&Item::GLASS_BOTTLE, 1);
                case.assert_drops(&[stack(&Item::SUGAR, 3)]);
                case.assert_crafted(3);
            } else {
                case.assert_grid(&[(index, stack(&Item::GLASS_BOTTLE, 1))]);
                case.assert_result(ItemStack::EMPTY);
                inventory[0] = stack(&Item::GLASS_BOTTLE, 2);
                case.click(1, SlotActionType::Throw);
                case.assert_drops(&vec![stack(&Item::SUGAR, 3); 3]);
                case.assert_crafted(9);
            }
            case.assert_inventory(&inventory);
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_result_throw_one_stops_on_empty_take_without_consuming_inputs() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        let case = CraftCase::new(&fixture, width, &Item::SUGAR);
        let index = width * width - 1;
        case.input(index, stack(&Item::HONEY_BOTTLE, 2));
        case.sync();
        case.assert_result(&stack(&Item::SUGAR, 3));
        // Invalidate the grid without a listener update to retain a nonempty stale result.
        case.input(0, stack(&Item::STONE, 1));
        case.click(1, SlotActionType::Throw);
        case.assert_grid(&[
            (0, stack(&Item::STONE, 1)),
            (index, stack(&Item::HONEY_BOTTLE, 2)),
        ]);
        case.assert_cursor(ItemStack::EMPTY);
        case.assert_inventory(&empty_inventory());
        case.assert_drops(&[]);
        case.assert_crafted(0);
        case.sync();
        case.assert_result(ItemStack::EMPTY);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crafting_spill_fills_selected_then_offhand_then_main_then_empty_main() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        let case = CraftCase::new(&fixture, width, &Item::SUGAR);
        let index = width * width - 1;
        let mut inventory = full_inventory();
        inventory[0] = ItemStack::EMPTY.clone();
        for index in [5, 9, 40] {
            inventory[index] = stack(&Item::GLASS_BOTTLE, 63);
        }
        case.inventory(&inventory);
        fixture.player.inventory.set_selected_slot(5);
        case.input(index, tagged(&Item::HONEY_BOTTLE, 5));
        case.sync();
        // Check each intermediate state: filling every destination in one craft would not
        // distinguish an incorrect preference order.
        for (crafted, destination) in [(1, 5), (2, 40), (3, 9), (4, 0)] {
            case.click(0, SlotActionType::Pickup);
            inventory[destination] =
                stack(&Item::GLASS_BOTTLE, if destination == 0 { 1 } else { 64 });
            case.assert_cursor(&stack(&Item::SUGAR, crafted * 3));
            case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 5 - crafted))]);
            case.assert_result(&stack(&Item::SUGAR, 3));
            case.assert_inventory(&inventory);
            case.assert_drops(&[]);
            case.assert_crafted(i32::from(crafted) * 3);
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crafting_spill_preserves_mismatched_components_and_drops_exact_leftover() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        let case = CraftCase::new(&fixture, width, &Item::SUGAR);
        let index = width * width - 1;
        let mut inventory = full_inventory();
        for slot in [0, 9, 40] {
            inventory[slot] = tagged(&Item::GLASS_BOTTLE, 63);
        }
        case.inventory(&inventory);
        case.input(index, tagged(&Item::HONEY_BOTTLE, 2));
        case.sync();
        case.click(0, SlotActionType::Pickup);
        case.assert_cursor(&stack(&Item::SUGAR, 3));
        case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 1))]);
        case.assert_result(&stack(&Item::SUGAR, 3));
        case.assert_inventory(&inventory);
        case.assert_drops(&[stack(&Item::GLASS_BOTTLE, 1)]);
        case.assert_crafted(3);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_no_remainder_and_consumption_only_bowl_do_not_create_remainders() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for use_bowl in [false, true] {
            let case = CraftCase::new(&fixture, width, &Item::SUGAR);
            let mut input = stack(&Item::SUGAR_CANE, 1);
            if use_bowl {
                // An actual builtin sugar recipe with a consumption component on its input.
                // This component is not the item's crafting-remainder property.
                input.set_data_component(UseRemainderImpl {
                    remainder: Some("minecraft:bowl".into()),
                });
            }
            case.input(width * width - 1, input);
            case.sync();
            case.assert_result(&stack(&Item::SUGAR, 1));
            case.click(0, SlotActionType::Pickup);
            case.assert_cursor(&stack(&Item::SUGAR, 1));
            case.assert_grid(&[]);
            case.assert_result(ItemStack::EMPTY);
            case.assert_inventory(&empty_inventory());
            case.assert_drops(&[]);
            case.assert_crafted(1);
        }
        let case = CraftCase::new(&fixture, width, &Item::MUSHROOM_STEW);
        case.input(0, stack(&Item::BROWN_MUSHROOM, 1));
        case.input(1, stack(&Item::RED_MUSHROOM, 1));
        case.input(width, stack(&Item::BOWL, 1));
        case.sync();
        case.assert_result(&stack(&Item::MUSHROOM_STEW, 1));
        case.click(0, SlotActionType::Pickup);
        case.assert_cursor(&stack(&Item::MUSHROOM_STEW, 1));
        case.assert_grid(&[]);
        case.assert_result(ItemStack::EMPTY);
        case.assert_inventory(&empty_inventory());
        case.assert_drops(&[]);
        case.assert_crafted(1);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_add_retains_partial_survival_leftover_and_discards_creative_leftover() {
    let fixture = player_fixture().await;
    let case = CraftCase::new(&fixture, 2, &Item::SUGAR);
    let mut inventory = full_inventory();
    inventory[40] = stack(&Item::GLASS_BOTTLE, 63);
    case.inventory(&inventory);
    let mut offered = stack(&Item::GLASS_BOTTLE, 3);
    assert!(!fixture.player.inventory.insert_stack_for_player(
        &mut offered,
        true,
        fixture.player.as_ref(),
    ));
    assert_stack(&offered, &stack(&Item::GLASS_BOTTLE, 2));
    inventory[40] = stack(&Item::GLASS_BOTTLE, 64);
    case.assert_inventory(&inventory);
    case.assert_drops(&[]);
    assert!(!fixture.player.inventory.insert_stack_for_player(
        &mut offered,
        true,
        fixture.player.as_ref(),
    ));
    assert_stack(&offered, &stack(&Item::GLASS_BOTTLE, 2));
    fixture
        .player
        .inventory
        .offer_or_drop_stack(offered, fixture.player.as_ref());
    case.assert_drops(&[stack(&Item::GLASS_BOTTLE, 2)]);

    fixture.player.gamemode.store(GameMode::Creative);
    let mut creative = stack(&Item::GLASS_BOTTLE, 4);
    assert!(fixture.player.inventory.insert_stack_for_player(
        &mut creative,
        true,
        fixture.player.as_ref(),
    ));
    assert!(creative.is_empty());
    case.assert_inventory(&inventory);
    case.assert_drops(&[stack(&Item::GLASS_BOTTLE, 2)]);
    case.assert_grid(&[]);
    case.assert_result(ItemStack::EMPTY);
    case.assert_cursor(ItemStack::EMPTY);
    case.assert_crafted(0);
    fixture.player.gamemode.store(GameMode::Survival);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_inventory_throw_zero_removes_only_one_item() {
    let fixture = player_fixture().await;
    let case = CraftCase::new(&fixture, 2, &Item::SUGAR);
    let mut inventory = empty_inventory();
    inventory[0] = stack(&Item::STONE, 5);
    case.inventory(&inventory);
    case.click_slot(36, 0, SlotActionType::Throw);
    inventory[0] = stack(&Item::STONE, 4);
    case.assert_inventory(&inventory);
    case.assert_drops(&[stack(&Item::STONE, 1)]);
    case.assert_grid(&[]);
    case.assert_cursor(ItemStack::EMPTY);
    case.assert_result(ItemStack::EMPTY);
    case.assert_crafted(0);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn menu_close_returns_cursor_and_grid_without_creative_item_loss() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for gamemode in [GameMode::Survival, GameMode::Creative] {
            for offhand_room in [0, 1] {
                let case = CraftCase::new(&fixture, width, &Item::SUGAR);
                fixture.player.gamemode.store(gamemode);
                let mut inventory = full_inventory();
                inventory[40] = tagged(&Item::GLASS_BOTTLE, 64 - offhand_room);
                case.inventory(&inventory);
                case.cursor(tagged(&Item::GLASS_BOTTLE, 3));
                case.input(width * width - 1, tagged(&Item::HONEY_BOTTLE, 2));
                case.sync();
                case.assert_result(&stack(&Item::SUGAR, 3));

                case.menu.lock().unwrap().on_closed(fixture.player.as_ref());

                inventory[40] = tagged(&Item::GLASS_BOTTLE, 64);
                case.assert_inventory(&inventory);
                case.assert_cursor(ItemStack::EMPTY);
                case.assert_grid(&[]);
                // Closing returns original items, including their components. It must not
                // consume inputs, drop the recipe preview, or discard leftovers in creative.
                case.assert_drops(&[
                    tagged(&Item::GLASS_BOTTLE, 3 - offhand_room),
                    tagged(&Item::HONEY_BOTTLE, 2),
                ]);
                case.assert_crafted(0);
                case.sync();
                case.assert_result(ItemStack::EMPTY);
            }
        }
    }
    fixture.player.gamemode.store(GameMode::Survival);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crafting_add_discards_only_creative_remainder_when_inventory_is_full() {
    let fixture = player_fixture().await;
    for width in [2, 3] {
        for gamemode in [GameMode::Survival, GameMode::Creative] {
            let case = CraftCase::new(&fixture, width, &Item::SUGAR);
            fixture.player.gamemode.store(gamemode);
            let mut inventory = full_inventory();
            inventory[40] = stack(&Item::STONE, 64);
            case.inventory(&inventory);
            let index = width * width - 1;
            case.input(index, tagged(&Item::HONEY_BOTTLE, 2));
            case.sync();
            case.click(0, SlotActionType::Pickup);

            case.assert_cursor(&stack(&Item::SUGAR, 3));
            case.assert_grid(&[(index, tagged(&Item::HONEY_BOTTLE, 1))]);
            case.assert_result(&stack(&Item::SUGAR, 3));
            case.assert_inventory(&inventory);
            if gamemode == GameMode::Creative {
                case.assert_drops(&[]);
            } else {
                case.assert_drops(&[stack(&Item::GLASS_BOTTLE, 1)]);
            }
            case.assert_crafted(3);
        }
    }
    fixture.player.gamemode.store(GameMode::Survival);
    close_fixture(&fixture).await;
}
