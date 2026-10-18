use super::{Ingredient, Recipe, RecipeKind, RecipeType};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::VarInt,
    component::ComponentTypeId,
    play::{RecipeDisplay, SlotDisplay},
};
fn item(item: Item) -> Box<SlotDisplay> {
    Box::new(SlotDisplay::Item { item })
}
fn template(stack: &ferraloom_protocol::component::ItemStackTemplate) -> Box<SlotDisplay> {
    Box::new(SlotDisplay::ItemStack {
        stack: stack.clone(),
    })
}
fn optional(ingredient: Option<&Ingredient>) -> Box<SlotDisplay> {
    Box::new(ingredient.map_or(SlotDisplay::Empty {}, Ingredient::display))
}
fn shapeless(ingredients: Vec<SlotDisplay>, result: Box<SlotDisplay>) -> RecipeDisplay {
    RecipeDisplay::CraftingShapeless {
        ingredients,
        result,
        crafting_station: item(Item::CRAFTING_TABLE),
    }
}
impl Recipe {
    /// Port of each Recipe#display override. Specials and brewing have no displays.
    pub fn display(&self) -> Vec<RecipeDisplay> {
        use RecipeKind::*;
        let display = match &self.kind {
            Shaped { pattern, result } => RecipeDisplay::CraftingShaped {
                width: VarInt(pattern.width as i32),
                height: VarInt(pattern.height as i32),
                ingredients: pattern
                    .ingredients
                    .iter()
                    .map(|ingredient| *optional(ingredient.as_ref()))
                    .collect(),
                result: template(result),
                crafting_station: item(Item::CRAFTING_TABLE),
            },
            Shapeless {
                ingredients,
                result,
            } => shapeless(
                ingredients.iter().map(Ingredient::display).collect(),
                template(result),
            ),
            Dye { target, dye, .. } => {
                let dye = SlotDisplay::OnlyWithComponent {
                    source: Box::new(dye.display()),
                    component: ComponentTypeId::DYE,
                };
                let target = target.display();
                shapeless(
                    vec![target.clone(), dye.clone()],
                    Box::new(SlotDisplay::Dyed {
                        dye: Box::new(dye),
                        target: Box::new(target),
                    }),
                )
            }
            Imbue {
                source,
                material,
                result,
            } => {
                let source = SlotDisplay::WithAnyPotion {
                    display: Box::new(source.display()),
                };
                let material = material.display();
                RecipeDisplay::CraftingShaped {
                    width: VarInt(3),
                    height: VarInt(3),
                    ingredients: (0..9)
                        .map(|i| {
                            if i == 4 {
                                source.clone()
                            } else {
                                material.clone()
                            }
                        })
                        .collect(),
                    result: Box::new(SlotDisplay::WithAnyPotion {
                        display: template(result),
                    }),
                    crafting_station: item(Item::CRAFTING_TABLE),
                }
            }
            Transmute {
                input,
                material,
                material_count,
                result,
                add_material_count,
            } => {
                let mut displays = Vec::new();
                let mut ingredients = vec![input.display()];
                // Vanilla starts the list with one input and adds one material per
                // display, even when a custom minimum is larger than one.
                for count in material_count.min.unwrap_or(1)..=material_count.max.unwrap_or(8) {
                    ingredients.push(material.display());
                    let count = result.count + if *add_material_count { count } else { 0 };
                    let display = if let Some(item) = result.item {
                        *template(&result.resolve(item, count))
                    } else {
                        SlotDisplay::Composite {
                            contents: input
                                .items()
                                .iter()
                                .map(|item| *template(&result.resolve(*item, count)))
                                .collect(),
                        }
                    };
                    displays.push(shapeless(ingredients.clone(), Box::new(display)));
                }
                return displays;
            }
            Cooking {
                recipe_type,
                ingredient,
                result,
                cooking_time,
                experience,
            } => RecipeDisplay::Furnace {
                ingredient: Box::new(ingredient.display()),
                fuel: Box::new(SlotDisplay::AnyFuel {}),
                result: template(result),
                crafting_station: item(match recipe_type {
                    RecipeType::Smelting => Item::FURNACE,
                    RecipeType::Blasting => Item::BLAST_FURNACE,
                    RecipeType::Smoking => Item::SMOKER,
                    _ => Item::CAMPFIRE,
                }),
                duration: VarInt(*cooking_time),
                experience: *experience,
            },
            Stonecutting { ingredient, result } => RecipeDisplay::Stonecutter {
                input: Box::new(ingredient.display()),
                result: template(result),
                crafting_station: item(Item::STONECUTTER),
            },
            SmithingTransform {
                template: source,
                base,
                addition,
                result,
            } => RecipeDisplay::Smithing {
                template: optional(source.as_ref()),
                base: Box::new(base.display()),
                addition: optional(addition.as_ref()),
                result: template(result),
                crafting_station: item(Item::SMITHING_TABLE),
            },
            SmithingTrim {
                template: source,
                base,
                addition,
                pattern,
            } => RecipeDisplay::Smithing {
                template: Box::new(source.display()),
                base: Box::new(base.display()),
                addition: Box::new(addition.display()),
                result: Box::new(SlotDisplay::SmithingTrim {
                    base: Box::new(base.display()),
                    material: Box::new(addition.display()),
                    pattern: Box::new(pattern.clone()),
                }),
                crafting_station: item(Item::SMITHING_TABLE),
            },
            _ => return Vec::new(),
        };
        vec![display]
    }
    pub fn recipe_book_category(&self) -> i32 {
        match self.recipe_type() {
            RecipeType::Crafting => match self.category.as_str() {
                "building" => 0,
                "redstone" => 1,
                "equipment" => 2,
                _ => 3,
            },
            RecipeType::Smelting => match self.category.as_str() {
                "food" => 4,
                "blocks" => 5,
                _ => 6,
            },
            RecipeType::Blasting => {
                if self.category == "blocks" {
                    7
                } else {
                    8
                }
            }
            RecipeType::Smoking => 9,
            RecipeType::Stonecutting => 10,
            RecipeType::Smithing => 11,
            RecipeType::CampfireCooking => 12,
            RecipeType::Brewing => 3,
        }
    }
}
