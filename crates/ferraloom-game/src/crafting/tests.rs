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
#[test]
fn vanilla_empty_stack_effective_accessors_and_restore() {
    use crate::entity::player::inventory::{
        component, copy_with_count, count, is_damaged, is_stackable, max_stack_size, set_count,
    };
    use crate::inventory::{ContainerId, Slot, SlotKind, UnsupportedMenuHooks};
    use ferraloom_protocol::{
        Writer,
        chat::Component,
        codec::VarInt,
        component::{ComponentTypeId, ItemStackTemplate},
    };

    fn check(stack: &ItemStack, expected: &serde_json::Value) {
        let components = stack.components().unwrap();
        let mut keys: Vec<_> = components
            .entries
            .iter()
            .map(|v| v.component_type().name())
            .collect();
        keys.sort_unstable();
        let mut prototype: Vec<_> = stack
            .prototype()
            .unwrap()
            .entries
            .iter()
            .map(|v| v.component_type().name())
            .collect();
        prototype.sort_unstable();
        let mut wire = Writer::new();
        wire.write(stack.components_patch()).unwrap();
        assert_eq!(stack.item().name(), expected["item"].as_str().unwrap());
        assert_eq!(count(stack), expected["count"].as_i64().unwrap() as i32);
        assert_eq!(stack.is_empty(), expected["empty"].as_bool().unwrap());
        assert_eq!(json!(keys), expected["components"]);
        assert_eq!(json!(prototype), expected["prototype"]);
        assert_eq!(wire.as_slice(), unhex(expected["patch"].as_str().unwrap()));
        assert_eq!(
            max_stack_size(stack),
            expected["max_stack_size"].as_i64().unwrap() as i32
        );
        assert_eq!(is_damaged(stack), expected["damaged"].as_bool().unwrap());
        assert_eq!(
            is_stackable(stack),
            expected["stackable"].as_bool().unwrap()
        );
        for key in ferraloom_data::registries::data_component_type::ALL {
            assert_eq!(stack.component(*key).unwrap(), components.get(*key));
            assert_eq!(component(stack, *key), components.get(*key));
        }
    }
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("stack-count-fixtures.json")).unwrap();
    for row in fixtures.as_array().unwrap() {
        let item = Item::from_name(row["source_item"].as_str().unwrap()).unwrap();
        let mut patch = DataComponentPatch::default();
        patch.set(DataComponent::CustomName(Box::new(Component::literal(
            "retained",
        ))));
        patch.set(DataComponent::MaxStackSize(VarInt(23)));
        patch.set(DataComponent::Damage(VarInt(7)));
        patch.remove(ComponentTypeId::ITEM_NAME);
        let mut source = ItemStack::new(item, 1, patch);
        let retained_patch = source.0.as_ref().unwrap().components.clone();
        set_count(&mut source, row["source_count"].as_i64().unwrap() as i32);
        check(&source, &row["effective"]);
        check(&copy_with_count(&source, 3), &row["copy"]);
        check(&stack::copy_with_count(&source, 3), &row["copy"]);
        let mut zero_copy = source.copy_with_count(0);
        check(&zero_copy, &row["zero_copy"]);
        let grown = count(&zero_copy).wrapping_add(2);
        set_count(&mut zero_copy, grown);
        check(&zero_copy, &row["zero_copy_grown"]);
        let mut negative_copy = source.copy_with_count(-3);
        let grown = count(&negative_copy).wrapping_add(2);
        set_count(&mut negative_copy, grown);
        check(&negative_copy, &row["negative_copy_grown"]);
        let template = ItemStackTemplate::new(Item::DIRT, 1, Default::default()).unwrap();
        check(&stack::transmute(&template, &source, 0), &row["transmuted"]);
        for (kind, accepts) in [
            SlotKind::Banner,
            SlotKind::ShulkerBox,
            SlotKind::Lapis,
            SlotKind::FurnaceFuel,
            SlotKind::MapAddition,
        ]
        .into_iter()
        .zip(row["effective"]["slots"].as_array().unwrap())
        {
            assert_eq!(
                Slot::new(ContainerId::Local(0), 0, 0, 0)
                    .with_kind(kind)
                    .may_place(&source, &UnsupportedMenuHooks)
                    .unwrap(),
                accepts.as_bool().unwrap()
            );
        }
        // Reading effective state must not discard the mutable stack's stored item or patch.
        assert_eq!(source.0.as_ref().unwrap().item, item);
        assert_eq!(source.0.as_ref().unwrap().components, retained_patch);
        let grown = count(&source).wrapping_add(2);
        set_count(&mut source, grown);
        check(&source, &row["grown"]);
        assert_eq!(source.0.as_ref().unwrap().components, retained_patch);
    }
}
fn decode(json: serde_json::Value) -> Recipe {
    Recipe::decode(&json, &tags()).unwrap()
}

#[test]
fn borrowed_positioned_input_keeps_live_receivers_and_shares_recipe_evaluation() {
    let mut grid = vec![ItemStack::default(); 9];
    grid[4] = stack(Item::MILK_BUCKET, 1);
    grid[5] = stack(Item::WHEAT, 3);
    let recipe = decode(json!({
        "type": "minecraft:crafting_shaped",
        "pattern": ["AB"],
        "key": { "A": "minecraft:milk_bucket", "B": "minecraft:wheat" },
        "result": { "id": "minecraft:bread" }
    }));
    let owned = CraftingInput::of_positioned(3, 3, grid.clone()).unwrap();
    {
        let borrowed = CraftingInputView::of_positioned(3, 3, &grid).unwrap();
        assert_eq!((borrowed.left, borrowed.top), (owned.left, owned.top));
        assert_eq!((borrowed.input.width(), borrowed.input.height()), (2, 1));
        assert!(std::ptr::eq(borrowed.input.get_item(0, 0), &grid[4]));
        assert!(std::ptr::eq(borrowed.input.get_item(1, 0), &grid[5]));
        assert!(std::ptr::eq(
            borrowed.input.iter().next_back().unwrap(),
            &grid[5]
        ));
        let input = RecipeInput::Crafting(&borrowed.input);
        assert!(recipe.matches(input, &()).unwrap());
        assert_eq!(
            recipe.assemble(input, &()).unwrap(),
            recipe
                .assemble(RecipeInput::Crafting(&owned.input), &())
                .unwrap()
        );
        assert_eq!(
            recipe.remaining_items(&borrowed.input).unwrap(),
            recipe.remaining_items(&owned.input).unwrap()
        );
    }
    // The next native recipe query reborrows the changed canonical grid.
    grid[5] = ItemStack::default();
    let borrowed = CraftingInputView::of_positioned(3, 3, &grid).unwrap();
    assert_eq!(borrowed.input.width(), 1);
    assert!(
        !recipe
            .matches(RecipeInput::Crafting(&borrowed.input), &())
            .unwrap()
    );
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
        4
    );
    assert!(contents.can_craft(&recipe.placement_info(), 4));
    assert!(!contents.can_craft(&recipe.placement_info(), 5));
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

struct OracleContext(serde_json::Value);
impl ferraloom_protocol::codec::RegistryContext for OracleContext {
    fn entry_id(
        &self,
        registry: &ferraloom_protocol::Identifier,
        key: &ferraloom_protocol::Identifier,
    ) -> Option<i32> {
        self.0[registry.as_str()]
            .as_array()?
            .iter()
            .position(|value| value.as_str() == Some(key.as_str()))
            .map(|index| index as i32)
    }
    fn entry_key(
        &self,
        registry: &ferraloom_protocol::Identifier,
        id: i32,
    ) -> Option<ferraloom_protocol::Identifier> {
        self.0[registry.as_str()]
            .as_array()?
            .get(usize::try_from(id).ok()?)?
            .as_str()?
            .parse()
            .ok()
    }
    fn has_tag(
        &self,
        _registry: &ferraloom_protocol::Identifier,
        _tag: &ferraloom_protocol::Identifier,
    ) -> bool {
        true
    }
}
fn oracle() -> serde_json::Value {
    serde_json::from_reader(flate2::read::GzDecoder::new(
        &include_bytes!("vanilla.json.gz")[..],
    ))
    .unwrap()
}
fn builtins() -> RecipeManager {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/home/doc/code/Ferraloom/.cache/vanilla/26.3/server.jar".into());
    let resources = ResourceManager::new(vec![
        Pack::vanilla_jar(&path)
            .expect("fetch the pinned vanilla jar, or set FERRALOOM_VANILLA_JAR"),
    ]);
    let tags = TagSet::load(&resources).unwrap();
    RecipeManager::load(&resources, &tags).unwrap()
}
fn oracle_stack(value: &serde_json::Value) -> ItemStack {
    use ferraloom_protocol::component::Persistent;
    if value.as_object().is_some_and(|v| v.is_empty()) {
        ItemStack::default()
    } else {
        ItemStack::from_json(value).unwrap()
    }
}
struct GameplayOracle<'a>(&'a serde_json::Value);
impl CraftingContext for GameplayOracle<'_> {
    fn map_scale(&self, id: i32) -> Result<Option<u8>> {
        Ok(match id {
            0 => Some(0),
            3 => Some(3),
            4 => Some(4),
            _ => None,
        })
    }
    fn is_curse(&self, enchantment: &ferraloom_protocol::Identifier) -> Result<bool> {
        Ok(self.0["curses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(enchantment.as_str())))
    }
}
fn check_case(manager: &RecipeManager, test: &serde_json::Value, context: &impl CraftingContext) {
    let items: Vec<_> = test["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(oracle_stack)
        .collect();
    let width = test["width"].as_u64().unwrap() as usize;
    let height = test["height"].as_u64().unwrap() as usize;
    let positioned;
    let smithing;
    let (kind, input) = match test["type"].as_str().unwrap() {
        "crafting" => {
            positioned = CraftingInput::of_positioned(width, height, items.clone()).unwrap();
            assert_eq!(
                (positioned.left, positioned.top),
                (
                    test["left"].as_u64().unwrap() as usize,
                    test["top"].as_u64().unwrap() as usize
                ),
                "{}",
                test["label"]
            );
            (
                RecipeType::Crafting,
                RecipeInput::Crafting(&positioned.input),
            )
        }
        "smithing" => {
            smithing = SmithingRecipeInput {
                template: items[0].clone(),
                base: items[1].clone(),
                addition: items[2].clone(),
            };
            (RecipeType::Smithing, RecipeInput::Smithing(&smithing))
        }
        "brewing" => (
            RecipeType::Brewing,
            RecipeInput::Brewing {
                input: &items[0],
                reagent: &items[1],
            },
        ),
        kind => (
            match kind {
                "smelting" => RecipeType::Smelting,
                "blasting" => RecipeType::Blasting,
                "smoking" => RecipeType::Smoking,
                "campfire_cooking" => RecipeType::CampfireCooking,
                "stonecutting" => RecipeType::Stonecutting,
                _ => panic!("unknown type"),
            },
            RecipeInput::Single(&items[0]),
        ),
    };
    let recipe = manager
        .get_recipe_for(kind, input, context, None)
        .unwrap_or_else(|error| panic!("{}: {error}", test["label"]));
    assert_eq!(
        recipe.map(|recipe| recipe.id.to_string()).as_deref(),
        test["recipe"].as_str(),
        "{}",
        test["label"]
    );
    if let Some(recipe) = recipe {
        let assembled = recipe
            .value
            .assemble(input, context)
            .unwrap_or_else(|error| panic!("{}: {error}", test["label"]));
        let mut expected = oracle_stack(&test["result"]);
        if test["map_post_processing"].as_str() == Some("SCALE") {
            stack::set(
                &mut expected,
                DataComponent::MapPostProcessing(
                    ferraloom_protocol::component::MapPostProcessing::Scale,
                ),
            );
        }
        assert!(
            stack::same_item_components(&assembled, &expected).unwrap(),
            "{} result: {assembled:?} expected {expected:?}",
            test["label"]
        );
        if let (Some(actual), Some(expected)) = (&assembled.0, &expected.0) {
            assert_eq!(
                actual.components.added.len(),
                expected.components.added.len(),
                "{} added patch size",
                test["label"]
            );
            assert_eq!(
                actual.components.removed.len(),
                expected.components.removed.len(),
                "{} removed patch size",
                test["label"]
            );
            assert!(
                actual
                    .components
                    .added
                    .iter()
                    .all(|value| expected.components.added.contains(value)),
                "{} added patch",
                test["label"]
            );
            assert!(
                actual
                    .components
                    .removed
                    .iter()
                    .all(|id| expected.components.removed.contains(id)),
                "{} removed patch",
                test["label"]
            );
        }
        assert_eq!(
            assembled.0.as_ref().map(|s| s.count),
            expected.0.as_ref().map(|s| s.count),
            "{} count",
            test["label"]
        );
        if let RecipeInput::Crafting(input) = input {
            let actual = recipe.value.remaining_items(input).unwrap();
            let expected: Vec<_> = test["remaining"]
                .as_array()
                .unwrap()
                .iter()
                .map(oracle_stack)
                .collect();
            for (a, b) in actual.iter().zip(&expected) {
                assert!(
                    stack::same_item_components(a, b).unwrap(),
                    "{} remainder",
                    test["label"]
                );
                assert_eq!(a.0.as_ref().map(|s| s.count), b.0.as_ref().map(|s| s.count));
            }
            assert_eq!(actual.len(), expected.len());
        }
    }
}
#[test]
fn vanilla_all_recipes_and_random_grids() {
    let oracle = oracle();
    let manager = builtins();
    assert_eq!(
        manager
            .recipes()
            .iter()
            .map(|recipe| recipe.id.to_string())
            .collect::<Vec<_>>(),
        oracle["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    let context = GameplayOracle(&oracle);
    for test in oracle["cases"].as_array().unwrap() {
        check_case(&manager, test, &context);
    }
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn vanilla_all_recipe_display_bytes_and_update_packet() {
    use ferraloom_protocol::{Reader, Wire, Writer, play::clientbound::UpdateRecipes};
    let oracle = oracle();
    let manager = builtins();
    let context = OracleContext(oracle["registries"].clone());
    for test in oracle["displays"].as_array().unwrap() {
        let id = test["id"].as_str().unwrap().parse().unwrap();
        let actual = manager.by_key(&id).unwrap().value.display();
        let expected = test["displays"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{id}");
        for (actual, expected) in actual.iter().zip(expected) {
            let mut writer = Writer::with_context(&context);
            actual.encode(&mut writer).unwrap();
            assert_eq!(writer.as_slice(), unhex(expected.as_str().unwrap()), "{id}");
        }
    }
    for test in oracle["displays"].as_array().unwrap() {
        let id = test["id"].as_str().unwrap().parse().unwrap();
        let recipe = &manager.by_key(&id).unwrap().value;
        let slots: Vec<i32> = recipe
            .placement_info()
            .slots_to_ingredient_index
            .iter()
            .map(|slot| slot.map_or(-1, |index| index as i32))
            .collect();
        for size in 1..=3 {
            let actual = place_recipe_slots(size, size, recipe, &slots);
            let expected: Vec<_> = test["placements"][size - 1]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_u64().unwrap() as usize,
                        pair[1].as_i64().unwrap() as i32,
                    )
                })
                .collect();
            assert_eq!(actual, expected, "{id}, {size}x{size}");
        }
    }
    let bytes = unhex(oracle["update_packet"].as_str().unwrap());
    let mut reader = Reader::with_context(&bytes, &context);
    let packet = UpdateRecipes::decode(&mut reader).unwrap();
    assert!(reader.remaining().is_empty());
    let mut writer = Writer::with_context(&context);
    packet.encode(&mut writer).unwrap();
    assert_eq!(writer.as_slice(), bytes);
    let sync = RecipeSynchronization::new(&manager, |_| true);
    for test in oracle["displays"].as_array().unwrap() {
        let id = test["id"].as_str().unwrap().parse().unwrap();
        let actual: Vec<_> = sync.displays_for_recipe(&id).collect();
        let expected = test["entries"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{id}");
        for (actual, expected) in actual.iter().zip(expected) {
            let mut writer = Writer::with_context(&context);
            actual.encode(&mut writer).unwrap();
            assert_eq!(
                writer.as_slice(),
                unhex(expected.as_str().unwrap()),
                "{id} display entry"
            );
        }
    }
    assert_eq!(sync.update.stonecutter_recipes, packet.stonecutter_recipes);
    for property in &sync.update.item_sets {
        let expected = packet
            .item_sets
            .iter()
            .find(|set| set.key == property.key)
            .unwrap();
        assert_eq!(property.items.len(), expected.items.len());
        for item in &property.items {
            assert!(
                expected.items.contains(item),
                "{} missing {}",
                property.key,
                item.name()
            );
        }
    }
}

#[test]
fn book_add_remove_highlight_and_persistence_follow_displays() {
    let recipes = vec![
        RecipeHolder {
            id: "first".parse().unwrap(),
            value: decode(
                json!({"type":"crafting_transmute","input":"stone","material":"dirt","material_count":{"min":1,"max":3},"result":"diamond","show_notification":false}),
            ),
        },
        RecipeHolder {
            id: "special".parse().unwrap(),
            value: decode(json!({"type":"crafting_special_repairitem"})),
        },
    ];
    let manager = RecipeManager::new(recipes).unwrap();
    let sync = RecipeSynchronization::new(&manager, |_| true);
    let mut book = ServerRecipeBook::default();
    let addition = book.add_recipes(manager.recipes(), &sync);
    assert_eq!(addition.unlocked.len(), 1);
    let packet = addition.packet.unwrap();
    assert_eq!(packet.entries.len(), 3);
    assert!(packet.entries.iter().all(|entry| entry.flags == 2));
    assert!(!packet.replace);
    assert!(book.add_recipes(manager.recipes(), &sync).packet.is_none());
    book.change_settings(
        ferraloom_protocol::play::RecipeBookType::Furnace,
        true,
        true,
    );
    let mut restored = ServerRecipeBook::default();
    restored.load_untrusted(&book.to_nbt(), &manager).unwrap();
    assert_eq!(restored.to_nbt(), book.to_nbt());
    restored.mark_display_seen(&sync, packet.entries[0].contents.id);
    assert!(!restored.highlighted(&manager.recipes()[0].id));
    let (_, initial) = restored.initial_packets(&sync);
    assert!(initial.replace);
    assert!(initial.entries.iter().all(|entry| entry.flags == 0));
    assert_eq!(
        restored
            .remove_recipes(manager.recipes(), &sync)
            .unwrap()
            .recipes
            .len(),
        3
    );
    assert!(restored.known().is_empty());
}
#[test]
fn map_scaling_requires_saved_data_and_caps_at_four() {
    struct Map(u8);
    impl CraftingContext for Map {
        fn map_scale(&self, _: i32) -> Result<Option<u8>> {
            Ok(Some(self.0))
        }
    }
    let recipe = decode(
        json!({"type":"crafting_special_mapextending","map":"filled_map","material":"paper","result":{}}),
    );
    let mut map = stack(Item::FILLED_MAP, 1);
    stack::set(
        &mut map,
        DataComponent::MapId(ferraloom_protocol::codec::VarInt(17)),
    );
    let mut grid = vec![stack(Item::PAPER, 1); 9];
    grid[4] = map;
    let input = CraftingInput::of(3, 3, grid).unwrap();
    assert!(matches!(
        recipe.matches(RecipeInput::Crafting(&input), &()),
        Err(Error::ContextRequired(_))
    ));
    assert!(
        recipe
            .matches(RecipeInput::Crafting(&input), &Map(3))
            .unwrap()
    );
    assert!(
        !recipe
            .matches(RecipeInput::Crafting(&input), &Map(4))
            .unwrap()
    );
    let output = recipe
        .assemble(RecipeInput::Crafting(&input), &Map(3))
        .unwrap();
    assert!(matches!(
        stack::component(
            &output,
            ferraloom_protocol::component::ComponentTypeId::MAP_POST_PROCESSING
        )
        .unwrap(),
        Some(DataComponent::MapPostProcessing(
            ferraloom_protocol::component::MapPostProcessing::Scale
        ))
    ));
}
#[test]
fn cache_keys_ignore_counts_and_invalidate_on_manager_change() {
    let recipe =
        decode(json!({"type":"crafting_shapeless","ingredients":["stone"],"result":"diamond"}));
    let id = "test".parse().unwrap();
    let manager = RecipeManager::new(vec![RecipeHolder { id, value: recipe }]).unwrap();
    let input = CraftingInput::of(1, 1, vec![stack(Item::STONE, 1)]).unwrap();
    let mut cache = RecipeCache::new(2);
    assert!(cache.get(&manager, &input, &()).unwrap().is_some());
    let input = CraftingInput::of(1, 1, vec![stack(Item::STONE, 64)]).unwrap();
    assert!(cache.get(&manager, &input, &()).unwrap().is_some());
    let empty = RecipeManager::new(Vec::new()).unwrap();
    assert!(cache.get(&empty, &input, &()).unwrap().is_none());
}

#[test]
fn placement_centers_only_one_wide_patterns_in_three_wide_grid() {
    let narrow = decode(
        json!({"type":"crafting_shaped","pattern":["S","S"],"key":{"S":"stick"},"result":"diamond"}),
    );
    assert_eq!(
        place_recipe_slots(3, 3, &narrow, &[0, 1]),
        vec![(1, 0), (4, 1)]
    );
    let square = decode(
        json!({"type":"crafting_shaped","pattern":["SS","SS"],"key":{"S":"stick"},"result":"diamond"}),
    );
    assert_eq!(
        place_recipe_slots(3, 3, &square, &[0, 1, 2, 3]),
        vec![(0, 0), (1, 1), (3, 2), (4, 3)]
    );
}
#[test]
fn vanilla_recipe_codec_edges() {
    let oracle = oracle();
    for case in oracle["codec_cases"].as_array().unwrap() {
        let decoded = Recipe::decode(&case["input"], &tags());
        assert_eq!(
            decoded.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{}: {decoded:?}",
            case["input"]
        );
        if let Ok(recipe) = decoded
            && case.get("cookingtime").is_some()
        {
            assert_eq!(
                recipe.cooking_time(),
                Some(case["cookingtime"].as_i64().unwrap() as i32),
                "{}",
                case["input"]
            );
            assert_eq!(
                recipe.experience(),
                Some(case["experience"].as_f64().unwrap() as f32)
            );
            let actual = recipe
                .assemble(RecipeInput::Single(&stack(Item::STONE, 1)), &())
                .unwrap();
            assert_eq!(actual, oracle_stack(&case["assembled"]));
        }
    }
}

struct PlacementState {
    inventory: Vec<ItemStack>,
    grid: Vec<ItemStack>,
    width: usize,
    height: usize,
}
impl RecipePlacementAccess for PlacementState {
    type Stack = ItemStack;
    fn inventory_items(&self) -> Result<&[ItemStack]> {
        Ok(&self.inventory)
    }
    fn input_grid(&self) -> Result<&[ItemStack]> {
        Ok(&self.grid)
    }
    fn capture_grid_item(&mut self, index: usize) -> Result<ItemStack> {
        Ok(self.grid[index].clone())
    }
    fn stack<'a>(&'a self, receiver: &'a ItemStack) -> Result<&'a ItemStack> {
        Ok(receiver)
    }
    fn fill_craft_slots(&self, contents: &mut StackedItemContents) -> Result<()> {
        for stack in &self.grid {
            contents.account_stack(stack, 64);
        }
        Ok(())
    }
    fn recipe_matches(&self, recipe: &RecipeHolder) -> Result<bool> {
        if self.width == 1 {
            recipe
                .value
                .matches(RecipeInput::Single(&self.grid[0]), &())
        } else {
            recipe.value.matches(
                RecipeInput::Crafting(&CraftingInput::of(
                    self.width,
                    self.height,
                    self.grid.clone(),
                )?),
                &(),
            )
        }
    }
    fn has_inventory_space_for(&self, stack: &ItemStack) -> Result<bool> {
        for existing in &self.inventory {
            if !existing.is_empty()
                && existing.0.as_ref().unwrap().count < 64
                && stack::same_item_components(existing, stack)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn clear_grid(&mut self) -> Result<()> {
        for source in &mut self.grid {
            if source.is_empty() {
                continue;
            }
            let count = source.0.as_ref().unwrap().count;
            if let Some(existing) = self.inventory.iter_mut().find(|existing| {
                !existing.is_empty() && stack::item(existing) == stack::item(source)
            }) {
                let n = existing.0.as_ref().unwrap().count;
                *existing = stack::copy_with_count(existing, n + count);
            } else {
                let index = self.inventory.iter().position(ItemStack::is_empty).unwrap();
                self.inventory[index] = source.clone();
            }
            *source = ItemStack::default();
        }
        Ok(())
    }
    fn take_inventory_item(&mut self, index: usize, count: i32) -> Result<ItemStack> {
        let stack = &mut self.inventory[index];
        if count >= stack.0.as_ref().unwrap().count {
            return Ok(std::mem::take(stack));
        }
        let n = stack.0.as_ref().unwrap().count.min(count);
        let taken = stack::copy_with_count(stack, n);
        *stack = stack::copy_with_count(stack, stack.0.as_ref().unwrap().count - n);
        Ok(taken)
    }
    fn set_grid_item(&mut self, index: usize, stack: ItemStack) -> Result<()> {
        self.grid[index] = stack;
        Ok(())
    }
    fn grow_grid_item(&mut self, index: usize, _receiver: &ItemStack, count: i32) -> Result<()> {
        self.grid[index].0.as_mut().unwrap().count += count;
        Ok(())
    }
    fn inventory_changed(&mut self) -> Result<()> {
        Ok(())
    }
}
#[test]
fn vanilla_recipe_book_placement_inventory_and_grid() {
    let oracle = oracle();
    let manager = builtins();
    for case in oracle["placement_cases"].as_array().unwrap() {
        let parse = |name| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(oracle_stack)
                .collect()
        };
        let mut state = PlacementState {
            inventory: parse("inventory"),
            grid: parse("grid"),
            width: case["width"].as_u64().unwrap() as usize,
            height: case["height"].as_u64().unwrap() as usize,
        };
        let id = case["recipe"].as_str().unwrap().parse().unwrap();
        let recipe = manager.by_key(&id).unwrap();
        let (width, height) = (state.width, state.height);
        let action = place_recipe(
            &mut state,
            width,
            height,
            recipe,
            case["max"].as_bool().unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(
            action,
            if case["action"] == "NOTHING" {
                PostPlaceAction::Nothing
            } else {
                PostPlaceAction::PlaceGhostRecipe
            }
        );
        assert_eq!(state.inventory, parse("after_inventory"), "{id} inventory");
        assert_eq!(state.grid, parse("after_grid"), "{id} grid");
    }
}

#[test]
fn manager_preserves_resource_filename_order_and_hint_precedence() {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    let oracle = oracle();
    let mut pack = Pack::new("recipe order");
    for file in oracle["resource_order"].as_array().unwrap().iter().rev() {
        let file = file.as_str().unwrap();
        pack.insert(
            file.parse().unwrap(),
            serde_json::to_vec(
                &json!({"type":"crafting_shapeless","ingredients":["stone"],"result":"diamond"}),
            )
            .unwrap(),
        );
    }
    let manager = RecipeManager::load(&ResourceManager::new(vec![pack]), &tags()).unwrap();
    assert_eq!(
        manager
            .recipes()
            .iter()
            .map(|recipe| format!("recipe/{}.json", recipe.id.path()))
            .collect::<Vec<_>>(),
        oracle["resource_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    let input = CraftingInput::of(1, 1, vec![stack(Item::STONE, 1)]).unwrap();
    assert_eq!(
        manager
            .get_crafting_recipe(&input, &())
            .unwrap()
            .unwrap()
            .id
            .path(),
        "a-b"
    );
    let hint = "a".parse().unwrap();
    assert_eq!(
        manager
            .get_recipe_for(
                RecipeType::Crafting,
                RecipeInput::Crafting(&input),
                &(),
                Some(&hint)
            )
            .unwrap()
            .unwrap()
            .id
            .path(),
        "a"
    );
}

#[test]
fn vanilla_simple_stacked_contents_use_effective_damageability() {
    let oracle = oracle();
    let placement = PlacementInfo::create(vec![Ingredient::of(vec![Item::STONE]).unwrap()]);
    for case in oracle["simple_stacks"].as_array().unwrap() {
        let input = oracle_stack(&case["input"]);
        let mut contents = StackedItemContents::default();
        contents.account_simple_stack(&input).unwrap();
        assert_eq!(
            contents.can_craft(&placement, 1),
            case["can_craft"].as_bool().unwrap(),
            "{input:?}"
        );
    }
}

#[test]
fn vanilla_packed_recipe_book_required_lists_and_boolean_defaults() {
    let manager = builtins();
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("book-fixtures.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let input = ferraloom_nbt::parse(case["input"].as_str().unwrap()).unwrap();
        let mut book = ServerRecipeBook::default();
        let before = book.to_nbt();
        let result = book.load_untrusted(&input, &manager);
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{input:?}"
        );
        if let Some(expected) = case["expected"].as_str() {
            assert_eq!(
                book.to_nbt(),
                ferraloom_nbt::parse(expected).unwrap(),
                "{input:?}"
            );
        } else {
            assert_eq!(
                book.to_nbt(),
                before,
                "malformed Packed must not partially mutate the book"
            );
        }
    }
}
