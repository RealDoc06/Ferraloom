use super::{Ingredient, RecipeHolder, RecipeKind, RecipeManager};
use ferraloom_data::{Item, datapack::Identifier};
use ferraloom_protocol::{
    codec::VarInt,
    common::OptionalVarInt,
    play::{
        RecipeDisplay, RecipeDisplayEntry, RecipeDisplayId, RecipePropertySet, SelectableRecipe,
        SlotDisplay, clientbound::UpdateRecipes,
    },
};

/// Port of RecipeManager.ServerDisplayInfo; parent index belongs to one snapshot.
#[derive(Clone, Debug)]
pub struct ServerDisplayInfo {
    pub display: RecipeDisplayEntry,
    pub parent: Identifier,
}
/// Port of RecipeManager#finalizeRecipeLoading. This snapshot is replaced on reload.
#[derive(Clone, Debug)]
pub struct RecipeSynchronization {
    pub update: UpdateRecipes,
    pub displays: Vec<ServerDisplayInfo>,
}
const PROPERTY_KEYS: [&str; 9] = [
    "smithing_addition",
    "smithing_base",
    "smithing_template",
    "furnace_input",
    "blast_furnace_input",
    "smoker_input",
    "campfire_input",
    "brewing_input",
    "brewing_reagent",
];
impl RecipeSynchronization {
    /// Item feature predicates come from the enabled registry snapshot. Vanilla
    /// identity/hash-salted immutable sets use the util crate's insertion policy.
    pub fn new(manager: &RecipeManager, is_item_enabled: impl Fn(Item) -> bool) -> Self {
        let mut properties: [Vec<Item>; 9] = std::array::from_fn(|_| Vec::new());
        let mut stonecutter_recipes = Vec::new();
        for holder in manager.recipes() {
            let recipe = &holder.value;
            if !recipe.is_special() && recipe.placement_info().is_impossible_to_place() {
                continue;
            }
            let mut collect = |index: usize, ingredient: &Ingredient| {
                if ingredient.items().iter().all(|item| is_item_enabled(*item)) {
                    for item in ingredient.items() {
                        if !properties[index].contains(item) {
                            properties[index].push(*item);
                        }
                    }
                }
            };
            match &recipe.kind {
                RecipeKind::SmithingTransform {
                    template,
                    base,
                    addition,
                    ..
                } => {
                    if let Some(i) = addition {
                        collect(0, i);
                    }
                    collect(1, base);
                    if let Some(i) = template {
                        collect(2, i);
                    }
                }
                RecipeKind::SmithingTrim {
                    template,
                    base,
                    addition,
                    ..
                } => {
                    collect(0, addition);
                    collect(1, base);
                    collect(2, template);
                }
                RecipeKind::Cooking {
                    recipe_type,
                    ingredient,
                    ..
                } => collect(
                    match recipe_type {
                        super::RecipeType::Smelting => 3,
                        super::RecipeType::Blasting => 4,
                        super::RecipeType::Smoking => 5,
                        _ => 6,
                    },
                    ingredient,
                ),
                RecipeKind::Brewing { input, reagent, .. } => {
                    collect(7, &input.ingredient);
                    collect(8, &reagent.ingredient);
                }
                RecipeKind::Stonecutting { ingredient, result } => {
                    let display = SlotDisplay::ItemStack {
                        stack: result.clone(),
                    };
                    if ingredient.items().iter().all(|item| is_item_enabled(*item))
                        && slot_enabled(&display, &is_item_enabled)
                    {
                        stonecutter_recipes.push(SelectableRecipe {
                            input: ingredient.item_set(),
                            option_display: Box::new(display),
                        });
                    }
                }
                _ => {}
            }
        }
        let item_sets = PROPERTY_KEYS
            .into_iter()
            .zip(properties)
            .map(|(key, items)| RecipePropertySet {
                key: ferraloom_protocol::Identifier::parse(&format!("minecraft:{key}"))
                    .expect("property key"),
                items,
            })
            .collect();
        let mut groups = Vec::new();
        let mut displays = Vec::new();
        for holder in manager.recipes() {
            let recipe = &holder.value;
            let group = if recipe.group.is_empty() {
                None
            } else {
                Some(
                    match groups.iter().position(|group| group == &recipe.group) {
                        Some(index) => index as i32,
                        None => {
                            let index = groups.len() as i32;
                            groups.push(recipe.group.clone());
                            index
                        }
                    },
                )
            };
            let requirements = if recipe.is_special() {
                None
            } else {
                Some(
                    recipe
                        .placement_info()
                        .ingredients
                        .iter()
                        .map(Ingredient::item_set)
                        .collect(),
                )
            };
            for display in recipe.display() {
                if display_enabled(&display, &is_item_enabled) {
                    let entry = RecipeDisplayEntry {
                        id: RecipeDisplayId(displays.len() as i32),
                        display,
                        group: OptionalVarInt(group),
                        category: VarInt(recipe.recipe_book_category()),
                        crafting_requirements: requirements.clone(),
                    };
                    displays.push(ServerDisplayInfo {
                        display: entry,
                        parent: holder.id.clone(),
                    });
                }
            }
        }
        Self {
            update: UpdateRecipes {
                item_sets,
                stonecutter_recipes,
            },
            displays,
        }
    }
    pub fn get_recipe_from_display(&self, id: RecipeDisplayId) -> Option<&ServerDisplayInfo> {
        usize::try_from(id.0)
            .ok()
            .and_then(|index| self.displays.get(index))
    }
    pub fn displays_for_recipe<'a>(
        &'a self,
        id: &'a Identifier,
    ) -> impl Iterator<Item = &'a RecipeDisplayEntry> {
        self.displays
            .iter()
            .filter(move |entry| &entry.parent == id)
            .map(|entry| &entry.display)
    }
    pub fn parent_for_display<'a>(
        &self,
        manager: &'a RecipeManager,
        id: RecipeDisplayId,
    ) -> Option<&'a RecipeHolder> {
        manager.by_key(&self.get_recipe_from_display(id)?.parent)
    }
}
fn slot_enabled(slot: &SlotDisplay, enabled: &impl Fn(Item) -> bool) -> bool {
    match slot {
        SlotDisplay::Item { item } => enabled(*item),
        SlotDisplay::ItemStack { stack } => enabled(stack.item),
        SlotDisplay::Composite { contents } => {
            contents.iter().all(|slot| slot_enabled(slot, enabled))
        }
        SlotDisplay::WithRemainder { input, remainder } => {
            slot_enabled(input, enabled) && slot_enabled(remainder, enabled)
        }
        _ => true,
    }
}
fn display_enabled(display: &RecipeDisplay, enabled: &impl Fn(Item) -> bool) -> bool {
    let (result, station) = match display {
        RecipeDisplay::CraftingShaped {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::CraftingShapeless {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Furnace {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Stonecutter {
            result,
            crafting_station,
            ..
        }
        | RecipeDisplay::Smithing {
            result,
            crafting_station,
            ..
        } => (result, crafting_station),
    };
    let base = slot_enabled(result, enabled) && slot_enabled(station, enabled);
    base && match display {
        RecipeDisplay::CraftingShaped { ingredients, .. }
        | RecipeDisplay::CraftingShapeless { ingredients, .. } => {
            ingredients.iter().all(|slot| slot_enabled(slot, enabled))
        }
        _ => true,
    }
}
