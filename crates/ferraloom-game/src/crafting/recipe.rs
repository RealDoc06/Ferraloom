use super::{CraftingInput, Error, Ingredient, PlacementInfo, RecipeInput, Result, stack};
use ferraloom_data::{Item, datapack::TagSet};
use ferraloom_protocol::component::{
    ComponentTypeId, DataComponentPatch, FireworkShape, ItemStack, ItemStackTemplate, Persistent,
    PotionsPredicate, TrimPatternHolder,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Port of RecipeType. A cooking recipe's type is distinct from its serializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipeType {
    Crafting,
    Smelting,
    Blasting,
    Smoking,
    CampfireCooking,
    Stonecutting,
    Smithing,
    Brewing,
}

/// Read-only world/registry facts consulted by vanilla's context-dependent recipes.
/// Missing facts are explicit errors, never failed matches or guessed values.
pub trait CraftingContext {
    fn map_scale(&self, _map_id: i32) -> Result<Option<u8>> {
        Err(Error::ContextRequired("saved map data"))
    }
    fn is_curse(&self, _enchantment: &ferraloom_protocol::Identifier) -> Result<bool> {
        Err(Error::ContextRequired("enchantment curse tag"))
    }
    fn potion_in_tag(
        &self,
        _tag: &ferraloom_protocol::Identifier,
        _potion: &ferraloom_protocol::Identifier,
    ) -> Result<bool> {
        Err(Error::ContextRequired("potion tags"))
    }
    fn potion_effects(
        &self,
        _potion: &ferraloom_protocol::Identifier,
    ) -> Result<Vec<ferraloom_protocol::component::MobEffectInstance>> {
        Err(Error::ContextRequired("potion effects registry"))
    }
    fn test_potions(&self, predicate: &PotionsPredicate, stack: &ItemStack) -> Result<bool> {
        let Some(ferraloom_protocol::component::DataComponent::PotionContents(contents)) =
            stack::component(stack, ComponentTypeId::POTION_CONTENTS)?
        else {
            return Ok(false);
        };
        if let Some(potions) = &predicate.potions {
            let Some(potion) = &contents.potion else {
                return Ok(false);
            };
            match potions {
                ferraloom_protocol::codec::HolderSet::Direct(items) => {
                    if !items.contains(&potion.0) {
                        return Ok(false);
                    }
                }
                ferraloom_protocol::codec::HolderSet::Named(tag) => {
                    if !self.potion_in_tag(tag, &potion.0)? {
                        return Ok(false);
                    }
                }
            }
        }
        if let Some(predicate) = &predicate.effects {
            let mut effects = if let Some(potion) = &contents.potion {
                self.potion_effects(&potion.0)?
            } else {
                Vec::new()
            };
            effects.extend(contents.custom_effects);
            if !super::potion::test_effects(predicate, &effects) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
impl CraftingContext for () {}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub kind: RecipeKind,
    pub group: String,
    pub category: String,
    pub show_notification: bool,
}

/// Ported serializer data, retaining vanilla names for source cross-reference.
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeKind {
    Shaped {
        pattern: ShapedRecipePattern,
        result: ItemStackTemplate,
    },
    Shapeless {
        ingredients: Vec<Ingredient>,
        result: ItemStackTemplate,
    },
    Dye {
        target: Ingredient,
        dye: Ingredient,
        result: ItemStackTemplate,
    },
    Imbue {
        source: Ingredient,
        material: Ingredient,
        result: ItemStackTemplate,
    },
    Transmute {
        input: Ingredient,
        material: Ingredient,
        material_count: IntRange,
        result: TransmuteResult,
        add_material_count: bool,
    },
    DecoratedPot {
        walls: [Ingredient; 4],
        result: ItemStackTemplate,
    },
    BookCloning {
        source: Ingredient,
        material: Ingredient,
        allowed_generations: IntRange,
        result: ItemStackTemplate,
    },
    MapExtending {
        map: Ingredient,
        material: Ingredient,
        result: TransmuteResult,
    },
    FireworkRocket {
        shell: Ingredient,
        fuel: Ingredient,
        star: Ingredient,
        result: ItemStackTemplate,
    },
    FireworkStar {
        shapes: Vec<(FireworkShape, Ingredient)>,
        trail: Ingredient,
        twinkle: Ingredient,
        fuel: Ingredient,
        dye: Ingredient,
        result: ItemStackTemplate,
    },
    FireworkStarFade {
        target: Ingredient,
        dye: Ingredient,
        result: ItemStackTemplate,
    },
    BannerDuplicate {
        banner: Ingredient,
        result: ItemStackTemplate,
    },
    ShieldDecoration {
        banner: Ingredient,
        target: Ingredient,
        result: ItemStackTemplate,
    },
    RepairItem,
    Cooking {
        recipe_type: RecipeType,
        ingredient: Ingredient,
        result: ItemStackTemplate,
        cooking_time: i32,
        experience: f32,
    },
    Stonecutting {
        ingredient: Ingredient,
        result: ItemStackTemplate,
    },
    SmithingTransform {
        template: Option<Ingredient>,
        base: Ingredient,
        addition: Option<Ingredient>,
        result: ItemStackTemplate,
    },
    SmithingTrim {
        template: Ingredient,
        base: Ingredient,
        addition: Ingredient,
        pattern: TrimPatternHolder,
    },
    Brewing {
        input: PotionIngredient,
        reagent: PotionIngredient,
        output: ItemStackTemplate,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct PotionIngredient {
    pub ingredient: Ingredient,
    pub potions: Option<PotionsPredicate>,
}
impl PotionIngredient {
    fn decode(v: &Value, tags: &TagSet<Item>) -> Result<Self> {
        Ok(Self {
            ingredient: Ingredient::decode(required(v, "item")?, tags)?,
            potions: v
                .get("potion_contents")
                .map(PotionsPredicate::from_json)
                .transpose()?,
        })
    }
    pub fn test(&self, stack: &ItemStack, context: &impl CraftingContext) -> Result<bool> {
        if !self.ingredient.test(stack) {
            return Ok(false);
        }
        self.potions
            .as_ref()
            .map_or(Ok(true), |predicate| context.test_potions(predicate, stack))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntRange {
    pub min: Option<i32>,
    pub max: Option<i32>,
}
impl IntRange {
    pub fn matches(self, value: i32) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }
    fn decode(v: &Value, lower: i32, upper: i32) -> Result<Self> {
        let result = if v.is_number() {
            let n = integer(v)?;
            Self {
                min: Some(n),
                max: Some(n),
            }
        } else {
            if !v.is_object() {
                return Err(Error::Invalid("expected integer bounds".into()));
            }
            Self {
                min: v.get("min").map(integer).transpose()?,
                max: v.get("max").map(integer).transpose()?,
            }
        };
        if result.min.is_none()
            || result.max.is_none()
            || result.min.is_some_and(|n| n < lower || n > upper)
            || result.max.is_some_and(|n| n < lower || n > upper)
            || matches!((result.min, result.max), (Some(min), Some(max)) if min > max)
        {
            return Err(Error::Invalid(
                "integer bounds outside allowed range".into(),
            ));
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TransmuteResult {
    pub item: Option<Item>,
    pub count: i32,
    pub components: DataComponentPatch,
}
impl TransmuteResult {
    fn decode(v: &Value) -> Result<Self> {
        if v.is_string() {
            let template = ItemStackTemplate::from_json(v)?;
            return Ok(Self {
                item: Some(template.item),
                count: 1,
                components: template.components,
            });
        }
        if !v.is_object() {
            return Err(Error::Invalid("expected transmute result".into()));
        }
        let item = v.get("id").map(Item::from_json).transpose()?;
        let count = v.get("count").map(integer).transpose()?.unwrap_or(1);
        if !(1..=99).contains(&count) {
            return Err(Error::Invalid("result count outside 1..99".into()));
        }
        Ok(Self {
            item,
            count,
            components: v
                .get("components")
                .map(DataComponentPatch::from_json)
                .transpose()?
                .unwrap_or_default(),
        })
    }
    pub fn resolve(&self, item: Item, count: i32) -> ItemStackTemplate {
        ItemStackTemplate {
            item: self.item.unwrap_or(item),
            count,
            components: self.components.clone(),
        }
    }
}

/// Port of ShapedRecipePattern#unpack, #shrink and #matches, using Java UTF-16 symbols.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedRecipePattern {
    pub width: usize,
    pub height: usize,
    pub ingredients: Vec<Option<Ingredient>>,
    pub symmetrical: bool,
}
impl ShapedRecipePattern {
    fn decode(v: &Value, tags: &TagSet<Item>) -> Result<Self> {
        let key = required(v, "key")?
            .as_object()
            .ok_or_else(|| Error::Invalid("expected pattern key".into()))?;
        let mut symbols = BTreeMap::new();
        for (name, value) in key {
            let chars: Vec<_> = name.encode_utf16().collect();
            if chars.len() != 1 || chars[0] == 32 {
                return Err(Error::Invalid(
                    "pattern key must be one non-space UTF-16 character".into(),
                ));
            }
            symbols.insert(chars[0], Ingredient::decode(value, tags)?);
        }
        let rows = required(v, "pattern")?
            .as_array()
            .ok_or_else(|| Error::Invalid("expected pattern rows".into()))?;
        if rows.is_empty() || rows.len() > 3 {
            return Err(Error::Invalid("pattern requires 1..3 rows".into()));
        }
        let rows: Vec<Vec<u16>> = rows
            .iter()
            .map(|row| {
                row.as_str()
                    .map(|row| row.encode_utf16().collect())
                    .ok_or_else(|| Error::Invalid("expected pattern string".into()))
            })
            .collect::<Result<_>>()?;
        let width = rows[0].len();
        if width > 3 || rows.iter().any(|row| row.len() != width) {
            return Err(Error::Invalid(
                "pattern rows must have equal width <= 3".into(),
            ));
        }
        let occupied: Vec<_> = rows
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .filter_map(move |(x, c)| (*c != 32).then_some((x, y)))
            })
            .collect();
        let left = occupied
            .iter()
            .map(|(x, _)| *x)
            .min()
            .ok_or_else(|| Error::Invalid("empty shaped pattern".into()))?;
        let right = occupied.iter().map(|(x, _)| *x).max().unwrap();
        let top = occupied.iter().map(|(_, y)| *y).min().unwrap();
        let bottom = occupied.iter().map(|(_, y)| *y).max().unwrap();
        let mut used = std::collections::BTreeSet::new();
        let mut ingredients = Vec::new();
        for row in &rows[top..=bottom] {
            for symbol in &row[left..=right] {
                ingredients.push(if *symbol == 32 {
                    None
                } else {
                    used.insert(*symbol);
                    Some(
                        symbols
                            .get(symbol)
                            .ok_or_else(|| {
                                Error::Invalid("pattern references undefined symbol".into())
                            })?
                            .clone(),
                    )
                });
            }
        }
        if used.len() != symbols.len() {
            return Err(Error::Invalid("unused pattern key".into()));
        }
        Ok(Self::new(right - left + 1, bottom - top + 1, ingredients))
    }
    pub fn new(width: usize, height: usize, ingredients: Vec<Option<Ingredient>>) -> Self {
        assert_eq!(width * height, ingredients.len());
        let symmetrical = (0..height).all(|y| {
            (0..width / 2)
                .all(|x| ingredients[x + y * width] == ingredients[width - x - 1 + y * width])
        });
        Self {
            width,
            height,
            ingredients,
            symmetrical,
        }
    }
    pub fn matches(&self, input: &CraftingInput) -> bool {
        if input.width() != self.width
            || input.height() != self.height
            || input.ingredient_count() != self.ingredients.iter().flatten().count()
        {
            return false;
        }
        let matches = |flip| {
            (0..self.height).all(|y| {
                (0..self.width).all(|x| {
                    Ingredient::test_optional(
                        self.ingredients
                            [(if flip { self.width - x - 1 } else { x }) + y * self.width]
                            .as_ref(),
                        input.get_item(x, y),
                    )
                })
            })
        };
        (!self.symmetrical && matches(true)) || matches(false)
    }
}
pub(super) fn required<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| Error::Invalid(format!("missing recipe field {key}")))
}
fn integer(v: &Value) -> Result<i32> {
    if !v.is_number() {
        return Err(Error::Invalid("expected integer".into()));
    }
    // StrictJsonParser leaves Gson numbers lazy. Number.intValue falls through
    // to BigDecimal for decimals/large integers, truncating and keeping low bits.
    let text = v.to_string();
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text.as_str(), 0), |(m, e)| {
            (
                m,
                e.parse::<i64>().unwrap_or(if e.starts_with('-') {
                    i64::MIN
                } else {
                    i64::MAX
                }),
            )
        });
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.trim_start_matches('-');
    let decimals = mantissa.split_once('.').map_or(0, |(_, tail)| tail.len()) as i64;
    let mut digits: Vec<u8> = mantissa.bytes().filter(|byte| *byte != b'.').collect();
    let scale = exponent.saturating_sub(decimals);
    if scale < 0 {
        digits.truncate(
            digits
                .len()
                .saturating_sub(scale.unsigned_abs().min(usize::MAX as u64) as usize),
        );
    }
    let mut number = digits.into_iter().fold(0u32, |number, byte| {
        number.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'))
    });
    if scale >= 32 {
        number = 0;
    } else {
        for _ in 0..scale.max(0) {
            number = number.wrapping_mul(10);
        }
    }
    Ok(if negative {
        number.wrapping_neg() as i32
    } else {
        number as i32
    })
}

fn decode_template(v: &Value) -> Result<ItemStackTemplate> {
    let mut value = v.clone();
    if let Some(count) = v.get("count") {
        value["count"] = Value::from(integer(count)?);
    }
    Ok(ItemStackTemplate::from_json(&value)?)
}

fn boolean(v: &Value, key: &str, default: bool) -> Result<bool> {
    v.get(key).map_or(Ok(default), |v| {
        v.as_bool()
            .ok_or_else(|| Error::Invalid(format!("expected boolean {key}")))
    })
}
fn string(v: &Value, key: &str, default: &str) -> Result<String> {
    v.get(key).map_or_else(
        || Ok(default.to_owned()),
        |v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::Invalid(format!("expected string {key}")))
        },
    )
}
impl Recipe {
    /// Port of Recipe.DIRECT_CODEC and the 26.3 RecipeSerializers registry.
    pub fn decode(v: &Value, tags: &TagSet<Item>) -> Result<Self> {
        let name = required(v, "type")?
            .as_str()
            .ok_or_else(|| Error::Invalid("expected recipe type".into()))?;
        let name: ferraloom_data::datapack::Identifier = name.parse()?;
        if name.namespace() != "minecraft" {
            return Err(Error::Invalid(format!("unknown recipe serializer {name}")));
        }
        let ingredient = |field| Ingredient::decode(required(v, field)?, tags);
        let optional = |field| {
            v.get(field)
                .map(|v| Ingredient::decode(v, tags))
                .transpose()
        };
        let result = || decode_template(required(v, "result")?);
        let kind = match name.path() {
            "crafting_shaped" => RecipeKind::Shaped {
                pattern: ShapedRecipePattern::decode(v, tags)?,
                result: result()?,
            },
            "crafting_shapeless" => {
                let values = required(v, "ingredients")?
                    .as_array()
                    .ok_or_else(|| Error::Invalid("expected ingredients list".into()))?;
                if !(1..=9).contains(&values.len()) {
                    return Err(Error::Invalid("shapeless requires 1..9 ingredients".into()));
                }
                RecipeKind::Shapeless {
                    ingredients: values
                        .iter()
                        .map(|v| Ingredient::decode(v, tags))
                        .collect::<Result<_>>()?,
                    result: result()?,
                }
            }
            "crafting_dye" => RecipeKind::Dye {
                target: ingredient("target")?,
                dye: ingredient("dye")?,
                result: result()?,
            },
            "crafting_imbue" => RecipeKind::Imbue {
                source: ingredient("source")?,
                material: ingredient("material")?,
                result: result()?,
            },
            "crafting_transmute" => RecipeKind::Transmute {
                input: ingredient("input")?,
                material: ingredient("material")?,
                material_count: v
                    .get("material_count")
                    .map(|v| IntRange::decode(v, 1, 8))
                    .transpose()?
                    .unwrap_or(IntRange {
                        min: Some(1),
                        max: Some(1),
                    }),
                result: TransmuteResult::decode(required(v, "result")?)?,
                add_material_count: boolean(v, "add_material_count_to_result", false)?,
            },
            "crafting_decorated_pot" => RecipeKind::DecoratedPot {
                walls: [
                    ingredient("back")?,
                    ingredient("left")?,
                    ingredient("right")?,
                    ingredient("front")?,
                ],
                result: result()?,
            },
            "crafting_special_bookcloning" => RecipeKind::BookCloning {
                source: ingredient("source")?,
                material: ingredient("material")?,
                allowed_generations: v
                    .get("allowed_generations")
                    .map(|v| IntRange::decode(v, 0, 2))
                    .transpose()?
                    .unwrap_or(IntRange {
                        min: Some(0),
                        max: Some(1),
                    }),
                result: result()?,
            },
            "crafting_special_mapextending" => RecipeKind::MapExtending {
                map: ingredient("map")?,
                material: ingredient("material")?,
                result: TransmuteResult::decode(required(v, "result")?)?,
            },
            "crafting_special_firework_rocket" => RecipeKind::FireworkRocket {
                shell: ingredient("shell")?,
                fuel: ingredient("fuel")?,
                star: ingredient("star")?,
                result: result()?,
            },
            "crafting_special_firework_star" => {
                let shape_values = required(v, "shapes")?
                    .as_object()
                    .ok_or_else(|| Error::Invalid("expected firework shapes".into()))?;
                let mut shapes = Vec::new();
                for (key, value) in shape_values {
                    let shape = FireworkShape::from_json(&Value::String(key.clone()))?;
                    shapes.push((shape, Ingredient::decode(value, tags)?));
                }
                RecipeKind::FireworkStar {
                    shapes,
                    trail: ingredient("trail")?,
                    twinkle: ingredient("twinkle")?,
                    fuel: ingredient("fuel")?,
                    dye: ingredient("dye")?,
                    result: result()?,
                }
            }
            "crafting_special_firework_star_fade" => RecipeKind::FireworkStarFade {
                target: ingredient("target")?,
                dye: ingredient("dye")?,
                result: result()?,
            },
            "crafting_special_bannerduplicate" => RecipeKind::BannerDuplicate {
                banner: ingredient("banner")?,
                result: result()?,
            },
            "crafting_special_shielddecoration" => RecipeKind::ShieldDecoration {
                banner: ingredient("banner")?,
                target: ingredient("target")?,
                result: result()?,
            },
            "crafting_special_repairitem" => RecipeKind::RepairItem,
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => RecipeKind::Cooking {
                recipe_type: match name.path() {
                    "smelting" => RecipeType::Smelting,
                    "blasting" => RecipeType::Blasting,
                    "smoking" => RecipeType::Smoking,
                    _ => RecipeType::CampfireCooking,
                },
                ingredient: ingredient("ingredient")?,
                result: result()?,
                cooking_time: integer(required(v, "cookingtime")?)?,
                experience: v.get("experience").map_or(Ok(0.), |v| {
                    v.as_f64()
                        .map(|n| n as f32)
                        .ok_or_else(|| Error::Invalid("expected experience float".into()))
                })?,
            },
            "stonecutting" => RecipeKind::Stonecutting {
                ingredient: ingredient("ingredient")?,
                result: result()?,
            },
            "smithing_transform" => RecipeKind::SmithingTransform {
                template: optional("template")?,
                base: ingredient("base")?,
                addition: optional("addition")?,
                result: result()?,
            },
            "smithing_trim" => RecipeKind::SmithingTrim {
                template: ingredient("template")?,
                base: ingredient("base")?,
                addition: ingredient("addition")?,
                pattern: TrimPatternHolder::from_json(required(v, "pattern")?)?,
            },
            "brewing" => RecipeKind::Brewing {
                input: PotionIngredient::decode(required(v, "input")?, tags)?,
                reagent: PotionIngredient::decode(required(v, "reagent")?, tags)?,
                output: decode_template(required(v, "output")?)?,
            },
            _ => return Err(Error::Invalid(format!("unknown recipe serializer {name}"))),
        };
        let special = kind.is_special();
        let normal_crafting = kind.recipe_type() == RecipeType::Crafting && !special;
        let cooking = matches!(kind, RecipeKind::Cooking { .. });
        let category = if normal_crafting || cooking {
            string(v, "category", "misc")?
        } else {
            "misc".into()
        };
        if normal_crafting
            && !["building", "redstone", "equipment", "misc"].contains(&category.as_str())
            || cooking && !["food", "blocks", "misc"].contains(&category.as_str())
        {
            return Err(Error::Invalid("unknown recipe book category".into()));
        }
        Ok(Self {
            kind,
            group: if normal_crafting || cooking {
                string(v, "group", "")?
            } else {
                String::new()
            },
            category,
            show_notification: if special {
                false
            } else {
                boolean(v, "show_notification", true)?
            },
        })
    }
    pub fn recipe_type(&self) -> RecipeType {
        self.kind.recipe_type()
    }
    pub fn is_special(&self) -> bool {
        self.kind.is_special()
    }
    pub fn placement_info(&self) -> PlacementInfo {
        use RecipeKind::*;
        let ingredients = match &self.kind {
            Shaped { pattern, .. } => {
                return PlacementInfo::from_optionals(pattern.ingredients.clone());
            }
            Shapeless { ingredients, .. } => ingredients.clone(),
            Dye { target, dye, .. } => vec![target.clone(), dye.clone()],
            Imbue {
                source, material, ..
            } => (0..9)
                .map(|i| {
                    if i == 4 {
                        source.clone()
                    } else {
                        material.clone()
                    }
                })
                .collect(),
            Transmute {
                input,
                material,
                material_count,
                ..
            } => std::iter::once(input.clone())
                .chain(std::iter::repeat_n(
                    material.clone(),
                    material_count.max.unwrap_or(8) as usize,
                ))
                .collect(),
            Cooking { ingredient, .. } | Stonecutting { ingredient, .. } => {
                vec![ingredient.clone()]
            }
            SmithingTransform {
                template,
                base,
                addition,
                ..
            } => {
                return PlacementInfo::from_optionals(vec![
                    template.clone(),
                    Some(base.clone()),
                    addition.clone(),
                ]);
            }
            SmithingTrim {
                template,
                base,
                addition,
                ..
            } => vec![template.clone(), base.clone(), addition.clone()],
            _ => return PlacementInfo::default(),
        };
        PlacementInfo::create(ingredients)
    }
    pub fn cooking_time(&self) -> Option<i32> {
        if let RecipeKind::Cooking { cooking_time, .. } = self.kind {
            Some(cooking_time)
        } else {
            None
        }
    }
    pub fn experience(&self) -> Option<f32> {
        if let RecipeKind::Cooking { experience, .. } = self.kind {
            Some(experience)
        } else {
            None
        }
    }
    pub fn matches(&self, input: RecipeInput<'_>, context: &impl CraftingContext) -> Result<bool> {
        use RecipeKind::*;
        match (&self.kind, input) {
            (
                Cooking { ingredient, .. } | Stonecutting { ingredient, .. },
                RecipeInput::Single(input),
            ) => Ok(ingredient.test(input)),
            (
                SmithingTransform {
                    template,
                    base,
                    addition,
                    ..
                },
                RecipeInput::Smithing(input),
            ) => Ok(
                Ingredient::test_optional(template.as_ref(), &input.template)
                    && base.test(&input.base)
                    && Ingredient::test_optional(addition.as_ref(), &input.addition),
            ),
            (
                SmithingTrim {
                    template,
                    base,
                    addition,
                    ..
                },
                RecipeInput::Smithing(input),
            ) => Ok(template.test(&input.template)
                && base.test(&input.base)
                && addition.test(&input.addition)),
            (
                Brewing {
                    input: ingredient,
                    reagent,
                    ..
                },
                RecipeInput::Brewing {
                    input,
                    reagent: actual,
                },
            ) => Ok(ingredient.test(input, context)? && reagent.test(actual, context)?),
            (_, RecipeInput::Crafting(input)) if self.recipe_type() == RecipeType::Crafting => {
                self.matches_crafting(input, context)
            }
            _ => Ok(false),
        }
    }
    pub fn assemble(
        &self,
        input: RecipeInput<'_>,
        context: &impl CraftingContext,
    ) -> Result<ItemStack> {
        use RecipeKind::*;
        match (&self.kind, input) {
            (Cooking { result, .. } | Stonecutting { result, .. }, RecipeInput::Single(_)) => {
                Ok(stack::create(result))
            }
            (SmithingTransform { result, .. }, RecipeInput::Smithing(input)) => {
                Ok(stack::transmute(result, &input.base, 0))
            }
            (SmithingTrim { pattern, .. }, RecipeInput::Smithing(input)) => {
                use ferraloom_protocol::component::{ArmorTrim, DataComponent};
                let Some(DataComponent::ProvidesTrimMaterial(material)) =
                    stack::component(&input.addition, ComponentTypeId::PROVIDES_TRIM_MATERIAL)?
                else {
                    return Ok(ItemStack::default());
                };
                let trim = ArmorTrim {
                    material,
                    pattern: pattern.clone(),
                };
                if stack::component(&input.base, ComponentTypeId::TRIM)?
                    == Some(DataComponent::Trim(trim.clone()))
                {
                    return Ok(ItemStack::default());
                }
                let mut result = stack::copy_with_count(&input.base, 1);
                stack::set(&mut result, DataComponent::Trim(trim));
                Ok(result)
            }
            (Brewing { output, .. }, RecipeInput::Brewing { .. }) => Ok(stack::create(output)),
            (_, RecipeInput::Crafting(input)) if self.recipe_type() == RecipeType::Crafting => {
                self.assemble_crafting(input, context)
            }
            _ => Err(Error::Invalid("recipe input type mismatch".into())),
        }
    }
}
impl RecipeKind {
    fn recipe_type(&self) -> RecipeType {
        match self {
            Self::Cooking { recipe_type, .. } => *recipe_type,
            Self::Stonecutting { .. } => RecipeType::Stonecutting,
            Self::SmithingTransform { .. } | Self::SmithingTrim { .. } => RecipeType::Smithing,
            Self::Brewing { .. } => RecipeType::Brewing,
            _ => RecipeType::Crafting,
        }
    }
    fn is_special(&self) -> bool {
        !matches!(
            self,
            Self::Shaped { .. }
                | Self::Shapeless { .. }
                | Self::Dye { .. }
                | Self::Imbue { .. }
                | Self::Transmute { .. }
                | Self::Cooking { .. }
                | Self::Stonecutting { .. }
                | Self::SmithingTransform { .. }
                | Self::SmithingTrim { .. }
        )
    }
}
