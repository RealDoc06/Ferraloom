use super::*;
use ferraloom_data::{
    Item,
    datapack::{TagSet, Tags},
};
use ferraloom_protocol::component::{DataComponent, DataComponentPatch, ItemStack};
use serde_json::json;
fn tags() -> TagSet<Item> {
    TagSet::from_tags(
        Tags {
            values: Default::default(),
            diagnostics: Vec::new(),
        },
        ferraloom_data::ITEMS.len(),
    )
    .unwrap()
}
fn stack(item: Item, count: i32) -> ItemStack {
    ItemStack::new(item, count, Default::default())
}
fn decode(json: serde_json::Value) -> Recipe {
    Recipe::decode(&json, &tags()).unwrap()
}
#[test]
fn positioned_mirror_and_empty_cells() {
    let recipe = decode(
        json!({"type":"crafting_shaped","pattern":[" A "," BB"],"key":{"A":"stick","B":"stone"},"result":{"id":"diamond"}}),
    );
    let grid = CraftingInput::of_positioned(
        3,
        3,
        vec![
            ItemStack::default(),
            ItemStack::default(),
            ItemStack::default(),
            ItemStack::default(),
            ItemStack::default(),
            stack(Item::STICK, 9),
            ItemStack::default(),
            stack(Item::STONE, 2),
            stack(Item::STONE, 1),
        ],
    )
    .unwrap();
    assert_eq!(
        (grid.left, grid.top, grid.input.width(), grid.input.height()),
        (1, 1, 2, 2)
    );
    assert!(
        recipe
            .matches(RecipeInput::Crafting(&grid.input), &())
            .unwrap()
    );
    let invalid = CraftingInput::of(
        2,
        2,
        vec![
            stack(Item::STICK, 1),
            stack(Item::STONE, 1),
            stack(Item::STONE, 1),
            stack(Item::STONE, 1),
        ],
    )
    .unwrap();
    assert!(
        !recipe
            .matches(RecipeInput::Crafting(&invalid), &())
            .unwrap()
    );
}
#[test]
fn overlapping_shapeless_ingredients_need_augmenting_paths() {
    let recipe = decode(
        json!({"type":"crafting_shapeless","ingredients":[["stone","dirt"],"stone"],"result":"diamond"}),
    );
    let input =
        CraftingInput::of(2, 1, vec![stack(Item::STONE, 64), stack(Item::DIRT, 1)]).unwrap();
    assert!(recipe.matches(RecipeInput::Crafting(&input), &()).unwrap());
    let mut contents = StackedItemContents::default();
    contents.account_stack(&stack(Item::STONE, 7), 64);
    contents.account_stack(&stack(Item::DIRT, 4), 64);
    assert_eq!(
        contents.biggest_craftable_stack(&recipe.placement_info().ingredients, 64),
        5
    );
    assert!(contents.can_craft(&recipe.placement_info(), 5));
    assert!(!contents.can_craft(&recipe.placement_info(), 6));
}
#[test]
fn transmute_template_wins_and_noop_is_rejected() {
    let recipe = decode(
        json!({"type":"crafting_transmute","input":"stone","material":"dirt","result":{"components":{"minecraft:custom_name":"result"}}}),
    );
    let mut patch = DataComponentPatch::default();
    patch.set(DataComponent::CustomName(Box::new(
        ferraloom_protocol::chat::Component::literal("input"),
    )));
    let source = ItemStack::new(Item::STONE, 1, patch);
    let input = CraftingInput::of(2, 1, vec![source, stack(Item::DIRT, 1)]).unwrap();
    assert!(recipe.matches(RecipeInput::Crafting(&input), &()).unwrap());
    let output = recipe.assemble(RecipeInput::Crafting(&input), &()).unwrap();
    let input = CraftingInput::of(2, 1, vec![output, stack(Item::DIRT, 1)]).unwrap();
    assert!(!recipe.matches(RecipeInput::Crafting(&input), &()).unwrap());
}
#[test]
fn item_remainders_and_cooking_codecs_are_distinct() {
    let recipe =
        decode(json!({"type":"crafting_shapeless","ingredients":["milk_bucket"],"result":"cake"}));
    let input = CraftingInput::of(1, 1, vec![stack(Item::MILK_BUCKET, 1)]).unwrap();
    assert_eq!(
        recipe.remaining_items(&input).unwrap(),
        vec![stack(Item::BUCKET, 1)]
    );
    assert!(
        Recipe::decode(
            &json!({"type":"smelting","ingredient":"stone","result":"diamond"}),
            &tags()
        )
        .is_err()
    );
    let cooking = decode(
        json!({"type":"smelting","ingredient":"stone","result":"diamond","cookingtime":-3,"experience":-1.5}),
    );
    assert_eq!(cooking.cooking_time(), Some(-3));
    assert_eq!(cooking.experience(), Some(-1.5));
}
