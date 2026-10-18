use super::{Ingredient, Result, stack};
use ferraloom_data::Item;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, ItemStack};

/// Port of PlacementInfo. Empty ingredients invalidate the whole placement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacementInfo {
    pub ingredients: Vec<Ingredient>,
    pub slots_to_ingredient_index: Vec<Option<usize>>,
}
impl PlacementInfo {
    pub fn create(ingredients: Vec<Ingredient>) -> Self {
        Self::from_optionals(ingredients.into_iter().map(Some).collect())
    }
    pub fn from_optionals(slots: Vec<Option<Ingredient>>) -> Self {
        let mut result = Self::default();
        for ingredient in slots {
            if ingredient.as_ref().is_some_and(Ingredient::is_empty) {
                return Self::default();
            }
            result
                .slots_to_ingredient_index
                .push(ingredient.as_ref().map(|_| result.ingredients.len()));
            if let Some(ingredient) = ingredient {
                result.ingredients.push(ingredient);
            }
        }
        result
    }
    pub fn is_impossible_to_place(&self) -> bool {
        self.slots_to_ingredient_index.is_empty()
    }
}

/// Port of StackedItemContents, accounting by item type, independently of components.
#[derive(Clone, Debug, Default)]
pub struct StackedItemContents {
    amounts: Vec<(Item, i32)>,
}
impl StackedItemContents {
    pub fn clear(&mut self) {
        self.amounts.clear();
    }
    pub fn account_stack(&mut self, input: &ItemStack, max_count: i32) {
        if let Some(input) = &input.0
            && input.count > 0
        {
            let count = max_count.min(input.count);
            if let Some((_, amount)) = self
                .amounts
                .iter_mut()
                .find(|(item, _)| *item == input.item)
            {
                *amount = amount.wrapping_add(count);
            } else {
                self.amounts.push((input.item, count));
            }
        }
    }
    pub fn account_simple_stack(&mut self, input: &ItemStack) -> Result<()> {
        if is_usable_for_crafting(input)? {
            let components = input.components()?;
            let max = match components.get(ComponentTypeId::MAX_STACK_SIZE) {
                Some(DataComponent::MaxStackSize(v)) => v.0,
                _ => 1,
            };
            self.account_stack(input, max);
        }
        Ok(())
    }
    pub fn can_craft(&self, placement: &PlacementInfo, amount: i32) -> bool {
        !placement.is_impossible_to_place() && self.pick(&placement.ingredients, amount).is_some()
    }
    pub fn from_grid(input: &super::CraftingInput) -> Self {
        let mut contents = Self::default();
        for stack in input.items() {
            contents.account_stack(stack, 1);
        }
        contents
    }
    /// Capacity-constrained augmenting paths preserve ingredient precedence without
    /// factorial permutation searches when custom ingredients overlap.
    pub fn pick(&self, ingredients: &[Ingredient], amount: i32) -> Option<Vec<Item>> {
        if amount <= 0 {
            return Some(Vec::new());
        }
        let items: Vec<_> = self
            .amounts
            .iter()
            .filter(|(_, n)| *n > 0)
            .copied()
            .collect();
        let mut counts: Vec<_> = items.iter().map(|(_, n)| *n / amount).collect();
        let mut assignment = vec![None; ingredients.len()];
        for (start, count) in counts.iter_mut().enumerate() {
            while *count > 0 {
                let mut visited_items = vec![false; items.len()];
                let mut visited_ingredients = vec![false; ingredients.len()];
                if !assign(
                    start,
                    ingredients,
                    &items,
                    &mut assignment,
                    &mut visited_items,
                    &mut visited_ingredients,
                ) {
                    break;
                }
                *count -= 1;
            }
        }
        assignment
            .into_iter()
            .map(|index| index.map(|index| items[index].0))
            .collect()
    }
    /// Port of StackedContents.RecipePicker#tryPickAll, including Java's signed
    /// overflow at the exclusive upper bound (observable for custom huge counts).
    pub fn biggest_craftable_stack(&self, ingredients: &[Ingredient], max_size: i32) -> i32 {
        let upper = ingredients
            .iter()
            .map(|ingredient| {
                self.amounts
                    .iter()
                    .filter(|(item, _)| ingredient.accepts_item(*item))
                    .map(|(_, count)| *count)
                    .max()
                    .unwrap_or(0)
                    .max(0)
            })
            .min()
            .unwrap_or(i32::MAX);
        let mut low = 0i32;
        let mut high = upper.min(max_size).wrapping_add(1);
        loop {
            let mid = low.wrapping_add(high) / 2;
            if self.pick(ingredients, mid).is_some() {
                if high.wrapping_sub(low) <= 1 {
                    return mid;
                }
                low = mid;
            } else {
                high = mid;
            }
        }
    }
}
fn assign(
    item: usize,
    ingredients: &[Ingredient],
    items: &[(Item, i32)],
    assignment: &mut [Option<usize>],
    visited_items: &mut [bool],
    visited_ingredients: &mut [bool],
) -> bool {
    visited_items[item] = true;
    for (index, ingredient) in ingredients.iter().enumerate() {
        if visited_ingredients[index]
            || !ingredient.accepts_item(items[item].0)
            || assignment[index] == Some(item)
        {
            continue;
        }
        visited_ingredients[index] = true;
        let available = match assignment[index] {
            None => true,
            Some(previous) => {
                !visited_items[previous]
                    && assign(
                        previous,
                        ingredients,
                        items,
                        assignment,
                        visited_items,
                        visited_ingredients,
                    )
            }
        };
        if available {
            assignment[index] = Some(item);
            return true;
        }
    }
    false
}

/// Port of CraftingRecipe#defaultCraftingReminder.
pub fn default_crafting_remainders(input: &super::CraftingInput) -> Result<Vec<ItemStack>> {
    input
        .items()
        .iter()
        .map(|input| crafting_remainder(stack::item(input)))
        .collect()
}
pub(super) fn crafting_remainder(item: Item) -> Result<ItemStack> {
    use ferraloom_protocol::component::{DataComponentPatch, Persistent};
    let Some(remainder) = item.data().crafting_remainder else {
        return Ok(ItemStack::default());
    };
    let json =
        serde_json::from_str(remainder.components_json).expect("extracted component patch JSON");
    Ok(ItemStack::new(
        remainder.item,
        remainder.count,
        DataComponentPatch::from_json(&json)?,
    ))
}

pub(super) fn is_usable_for_crafting(input: &ItemStack) -> Result<bool> {
    let components = input.components()?;
    let damaged = matches!(components.get(ComponentTypeId::DAMAGE), Some(DataComponent::Damage(value)) if value.0 > 0);
    let enchanted = matches!(components.get(ComponentTypeId::ENCHANTMENTS), Some(DataComponent::Enchantments(value)) if !value.0.is_empty());
    Ok(!damaged && !enchanted && components.get(ComponentTypeId::CUSTOM_NAME).is_none())
}
