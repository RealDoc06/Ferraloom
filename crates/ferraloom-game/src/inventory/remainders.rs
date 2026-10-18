//! ResultSlot's remainder loop holds the native list of receiver identities,
//! including aliases from copyAllInputItems when no recipe matches.
use super::*;

pub struct CraftingRemainders<S> {
    pub grid_width: usize,
    pub width: usize,
    pub height: usize,
    pub left: usize,
    pub top: usize,
    /// Native order; every receiver is rooted by the surrounding operation.
    pub items: Vec<S>,
}

/// Port of ResultSlot#onTake after checkTakeAchievements/getRemainingItems.
/// Removing an ingredient finishes slotsChanged before the current-cell read;
/// fallback remainders keep the original input object if a callback replaces it.
pub fn apply_crafting_remainders<C: MenuClickContext>(
    context: &mut C,
    remaining: &CraftingRemainders<C::Stack>,
    hooks: &mut dyn MenuHooks,
) -> Result<()> {
    for y in 0..remaining.height {
        for x in 0..remaining.width {
            let grid_slot = x + remaining.left + (y + remaining.top) * remaining.grid_width;
            let index = grid_slot + 1;
            let mut current = context.slot_stack(index)?;
            let replacement = remaining.items[x + y * remaining.width];
            if !context.stack(current)?.is_empty() {
                context.remove_slot_stack(index, 1, hooks)?;
                current = context.slot_stack(index)?;
            }
            if context.stack(replacement)?.is_empty() {
                continue;
            }
            if context.stack(current)?.is_empty() {
                context.set_slot_stack(index, replacement, None, hooks)?;
            } else if is_same_item_same_components(
                &context.stack(current)?,
                &context.stack(replacement)?,
            ) {
                let total = count(&context.stack(replacement)?)
                    .wrapping_add(count(&context.stack(current)?));
                context.set_stack_count(replacement, total)?;
                context.set_slot_stack(index, replacement, None, hooks)?;
            } else if !context.add_to_inventory(replacement)? {
                context.drop_stack(replacement, false)?;
            }
        }
    }
    Ok(())
}
