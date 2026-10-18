use super::{Recipe, RecipeHolder, RecipeKind, Result, StackedItemContents, stack};
use ferraloom_data::Item;
use ferraloom_protocol::component::ItemStack;

/// Vanilla ServerPlaceRecipe's live menu/inventory adapter. Implementations own
/// ordinary Inventory/Slot callbacks; the placer never keeps a state borrow
/// across one of those calls. Inventory items are the 36 non-equipment slots.
pub trait RecipePlacementAccess {
    type Stack;
    fn inventory_items(&self) -> Result<&[ItemStack]>;
    fn input_grid(&self) -> Result<&[ItemStack]>;
    /// Capture getItem's receiver before inventory lookup/removal callbacks.
    fn capture_grid_item(&mut self, index: usize) -> Result<Self::Stack>;
    fn stack<'a>(&'a self, receiver: &'a Self::Stack) -> Result<&'a ItemStack>;
    fn fill_craft_slots(&self, contents: &mut StackedItemContents) -> Result<()>;
    fn recipe_matches(&self, recipe: &RecipeHolder) -> Result<bool>;
    /// Inventory#getSlotWithRemainingSpace, including selected/offhand precedence.
    fn has_inventory_space_for(&self, stack: &ItemStack) -> Result<bool>;
    /// For each slotsToClear: copy, placeItemBackInInventory(send=false,
    /// SERVER_ONLY), set residual; then menu.clearCraftingContent. The adapter
    /// performs drop/callback effects synchronously in that order.
    fn clear_grid(&mut self) -> Result<()>;
    /// Inventory#removeItem when count < stack count, else removeItemNoUpdate.
    fn take_inventory_item(&mut self, index: usize, count: i32) -> Result<Self::Stack>;
    fn set_grid_item(&mut self, index: usize, stack: Self::Stack) -> Result<()>;
    /// ItemStack#grow on the existing target, without Slot#set callbacks.
    fn grow_grid_item(&mut self, index: usize, receiver: &Self::Stack, count: i32) -> Result<()>;
    fn inventory_changed(&mut self) -> Result<()>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostPlaceAction {
    Nothing,
    PlaceGhostRecipe,
}
/// Port of ServerPlaceRecipe#placeRecipe, through the same live Slot callbacks
/// menus use for ordinary clicks. `allow_dropping` controls only the clear test.
pub fn place_recipe(
    access: &mut impl RecipePlacementAccess,
    grid_width: usize,
    grid_height: usize,
    recipe: &RecipeHolder,
    use_max_items: bool,
    allow_dropping: bool,
) -> Result<PostPlaceAction> {
    if !allow_dropping && !test_clear_grid(access)? {
        return Ok(PostPlaceAction::Nothing);
    }
    let mut available = StackedItemContents::default();
    for stack in access.inventory_items()? {
        available.account_simple_stack(stack)?;
    }
    access.fill_craft_slots(&mut available)?;
    let placement = recipe.value.placement_info();
    if !available.can_craft(&placement, 1) {
        access.clear_grid()?;
        access.inventory_changed()?;
        return Ok(PostPlaceAction::PlaceGhostRecipe);
    }
    let matches = access.recipe_matches(recipe)?;
    let biggest = available.biggest_craftable_stack(&placement.ingredients, i32::MAX);
    if matches {
        for stack in access.input_grid()? {
            if !stack.is_empty()
                && let Some(contents) = &stack.0
                && max_stack_size(stack)?.min(biggest) < contents.count.wrapping_add(1)
            {
                access.inventory_changed()?;
                return Ok(PostPlaceAction::Nothing);
            }
        }
    }
    let amount = if use_max_items {
        biggest
    } else if matches {
        access
            .input_grid()?
            .iter()
            .filter(|stack| !stack.is_empty())
            .filter_map(|stack| stack.0.as_ref().map(|stack| stack.count))
            .min()
            .map_or(i32::MAX, |count| count.wrapping_add(1))
    } else {
        1
    };
    if let Some(mut picked) = available.pick(&placement.ingredients, amount) {
        let adjusted = picked.iter().fold(amount, |amount, item| {
            amount.min(item.data().max_stack_size as i32)
        });
        if adjusted != amount {
            let Some(repicked) = available.pick(&placement.ingredients, adjusted) else {
                access.inventory_changed()?;
                return Ok(PostPlaceAction::Nothing);
            };
            picked = repicked;
        }
        access.clear_grid()?;
        for (grid_index, ingredient) in place_recipe_slots(
            grid_width,
            grid_height,
            &recipe.value,
            &placement.slots_to_ingredient_index,
        ) {
            if let Some(ingredient) = ingredient {
                let item = picked[ingredient];
                let mut remaining = adjusted;
                while remaining > 0 {
                    let target = access.capture_grid_item(grid_index)?;
                    let index = find_inventory_item(
                        access.inventory_items()?,
                        item,
                        access.stack(&target)?,
                    )?;
                    let Some(index) = index else {
                        break;
                    };
                    let taken = access.take_inventory_item(index, remaining)?;
                    let count = access
                        .stack(&taken)?
                        .0
                        .as_ref()
                        .map_or(0, |stack| stack.count);
                    if count <= 0 {
                        break;
                    }
                    if access.stack(&target)?.is_empty() {
                        access.set_grid_item(grid_index, taken)?;
                    } else {
                        access.grow_grid_item(grid_index, &target, count)?;
                    }
                    remaining -= count;
                }
            }
        }
    }
    access.inventory_changed()?;
    Ok(PostPlaceAction::Nothing)
}
/// Port of PlaceRecipeHelper#placeRecipe, including vanilla's strict half-grid
/// centering threshold. Entries for empty shaped slots still advance the grid.
pub fn place_recipe_slots<T: Clone>(
    grid_width: usize,
    grid_height: usize,
    recipe: &Recipe,
    entries: &[T],
) -> Vec<(usize, T)> {
    let (width, height) = match &recipe.kind {
        RecipeKind::Shaped { pattern, .. } => (pattern.width, pattern.height),
        _ => (grid_width, grid_height),
    };
    let mut output = Vec::new();
    let mut iterator = entries.iter();
    let mut grid_index = 0;
    let mut y = 0;
    while y < grid_height {
        let center_height = (height as f32) < (grid_height as f32 / 2.);
        let start = ((grid_height as f32 - height as f32) / 2.).floor() as usize;
        if center_height && start > y {
            grid_index += grid_width;
            y += 1;
        }
        for x in 0..grid_width {
            if iterator.len() == 0 {
                return output;
            }
            let center_width = (width as f32) < (grid_width as f32 / 2.);
            let start = ((grid_width as f32 - width as f32) / 2.).floor() as usize;
            let end = if center_width { start + width } else { width };
            let add = if center_width {
                start <= x && x < end
            } else {
                x < width
            };
            if add {
                output.push((grid_index, iterator.next().unwrap().clone()));
            } else if end == x {
                grid_index += grid_width - x;
                break;
            }
            grid_index += 1;
        }
        y += 1;
    }
    output
}
fn max_stack_size(stack: &ItemStack) -> Result<i32> {
    use ferraloom_protocol::component::{ComponentTypeId, DataComponent};
    Ok(
        match stack::component(stack, ComponentTypeId::MAX_STACK_SIZE)? {
            Some(DataComponent::MaxStackSize(value)) => value.0,
            _ => 1,
        },
    )
}
fn find_inventory_item(
    items: &[ItemStack],
    item: Item,
    target: &ItemStack,
) -> Result<Option<usize>> {
    for (index, stack) in items.iter().enumerate() {
        if !stack.is_empty()
            && stack::item(stack) == item
            && super::stacked_contents::is_usable_for_crafting(stack)?
            && (target.is_empty() || stack::same_item_components(target, stack)?)
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}
fn test_clear_grid(access: &impl RecipePlacementAccess) -> Result<bool> {
    let free_slots = access
        .inventory_items()?
        .iter()
        .filter(|stack| stack.is_empty())
        .count();
    let mut free: Vec<ItemStack> = Vec::new();
    for stack in access
        .input_grid()?
        .iter()
        .filter(|stack| !stack.is_empty())
    {
        if !access.has_inventory_space_for(stack)? {
            if free.len() > free_slots {
                return Ok(false);
            }
            let mut merged = false;
            for existing in &mut free {
                let count = existing.0.as_ref().unwrap().count;
                let maximum = max_stack_size(existing)?;
                let incoming = stack.0.as_ref().unwrap().count;
                if stack::item(existing) == stack::item(stack)
                    && count != maximum
                    && count.wrapping_add(incoming) <= maximum
                {
                    *existing = stack::copy_with_count(existing, count.wrapping_add(incoming));
                    merged = true;
                    break;
                }
            }
            if !merged {
                if free.len() >= free_slots {
                    return Ok(false);
                }
                free.push(stack.clone());
            }
        }
    }
    Ok(true)
}
