//! ResultSlot bookkeeping is shared; each adapter delivers its native effects
//! after ending the short storage borrow and keeps the captured receivers rooted.
use super::*;
use ferraloom_data::datapack::Identifier;

pub trait CraftingResultContext: MenuClickContext {
    fn crafted_item(
        &mut self,
        stack: Self::Stack,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()>;
    fn award_crafting_recipe(
        &mut self,
        recipe: &Identifier,
        input: &[Self::Stack],
        hooks: &mut dyn MenuHooks,
    ) -> Result<bool>;
    fn crafting_remainders(
        &mut self,
        hooks: &mut dyn MenuHooks,
    ) -> Result<CraftingRemainders<Self::Stack>>;
}

/// Port of ResultSlot#checkTakeAchievements. The recipe/input receiver list is
/// captured after item callbacks, and removeCount resets after all recipe work.
pub fn check_crafting_take_achievements<C: CraftingResultContext>(
    context: &mut C,
    stack: C::Stack,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    let amount = context.menu()?.crafting_remove_count;
    if amount > 0 {
        context.crafted_item(stack, amount, hooks)?;
    }
    let width = context
        .menu()?
        .crafting_grid_size()
        .ok_or(MenuError::Unsupported("non-crafting result slot"))?;
    // CraftingContainer#getItems captures every original grid receiver, with
    // empty entries and repeated identities preserved for recipe predicates.
    let mut input = Vec::with_capacity(width * width);
    for index in 0..width * width {
        input.push(context.slot_stack(index + 1)?);
    }
    let recipe = context.menu()?.crafting_recipe_used.clone();
    if let Some(recipe) = recipe
        && context.award_crafting_recipe(&recipe, &input, hooks)?
    {
        context.menu_mut()?.crafting_recipe_used = None;
    }
    context.menu_mut()?.crafting_remove_count = 0;
    Ok(())
}

/// Port of ResultSlot#onTake; callers already applied onSwapCraft, if needed.
pub fn take_crafting_result_stack<C: CraftingResultContext>(
    context: &mut C,
    stack: C::Stack,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    check_crafting_take_achievements(context, stack, hooks)?;
    let remaining = context.crafting_remainders(hooks)?;
    apply_crafting_remainders(context, &remaining, hooks)
}

impl CraftingResultContext for super::in_memory::InMemoryMenuOperation<'_, '_, '_> {
    fn crafted_item(
        &mut self,
        stack: super::in_memory::MemoryStackRef,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let observed = self.read(stack)?.clone();
        hooks.crafting().expect("crafting bridge").item_crafted(
            self.menu,
            self.context,
            &observed,
            Some(amount),
        )
    }
    fn award_crafting_recipe(
        &mut self,
        recipe: &Identifier,
        input: &[super::in_memory::MemoryStackRef],
        hooks: &mut dyn MenuHooks,
    ) -> Result<bool> {
        // The legacy callback signature observes standalone values. It never
        // writes them back; the live adapter passes these retained handles.
        let observed = input
            .iter()
            .map(|&reference| self.read(reference).cloned())
            .collect::<Result<Vec<_>>>()?;
        hooks
            .crafting()
            .expect("crafting bridge")
            .award(self.menu, self.context, recipe, &observed)
    }
    fn crafting_remainders(
        &mut self,
        hooks: &mut dyn MenuHooks,
    ) -> Result<CraftingRemainders<super::in_memory::MemoryStackRef>> {
        use crate::crafting::{CraftingInputAccess, CraftingInputView};
        let grid_width = self.menu.crafting_grid_size().expect("crafting menu");
        let (width, height, left, top, remaining) = {
            let positioned = CraftingInputView::of_positioned(
                grid_width,
                grid_width,
                self.menu.crafting_grid(),
            )?;
            let input = &positioned.input;
            let remaining = hooks
                .crafting()
                .expect("crafting bridge")
                .remaining(input)?;
            (
                input.width(),
                input.height(),
                positioned.left,
                positioned.top,
                remaining,
            )
        };
        let items = match remaining {
            Some(items) => items.into_iter().map(|item| self.own(item)).collect(),
            None => {
                let mut items = Vec::with_capacity(width * height);
                for y in 0..height {
                    for x in 0..width {
                        items.push(self.slot_stack(x + left + (y + top) * grid_width + 1)?);
                    }
                }
                items
            }
        };
        Ok(CraftingRemainders {
            grid_width,
            width,
            height,
            left,
            top,
            items,
        })
    }
}
