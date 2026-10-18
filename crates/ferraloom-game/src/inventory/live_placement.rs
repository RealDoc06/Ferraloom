//! Handle adapter for ServerPlaceRecipe. Read-only matching borrows canonical
//! grids; mutable delivery uses the exact held target and removed stack objects.
use super::crafting::placement_error;
use super::*;
use crate::{
    crafting::{
        CraftingInputView, RecipeHolder, RecipeInput, RecipePlacementAccess, StackedItemContents,
    },
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};

struct LiveRecipePlacement<'a, 'b> {
    transfer: super::transfer::LiveMenuTransfer<'a>,
    hooks: &'b mut dyn MenuHooks,
}
impl LiveRecipePlacement<'_, '_> {
    fn menu(&self) -> Result<&AbstractContainerMenu> {
        self.transfer.menu()
    }
    fn items(&self) -> Result<&[ItemStack]> {
        let menu = self.menu()?;
        if let Some(&handle) = menu.block_entities.first() {
            Ok(&self
                .transfer
                .operation
                .level
                .block_entity_state(handle)?
                .container()
                .ok_or(MenuError::Unsupported("recipe placement container"))?
                .items)
        } else {
            match &menu.local[0] {
                MenuContainer::Simple(container) => Ok(container.items()),
                MenuContainer::Crafting(container) => Ok(container.items()),
                MenuContainer::Result(_) => Err(MenuError::Unsupported("recipe placement grid")),
            }
        }
    }
    fn grid_offset(&self) -> Result<usize> {
        Ok(usize::from(self.menu()?.crafting_grid_size().is_some()))
    }
}
impl RecipePlacementAccess for LiveRecipePlacement<'_, '_> {
    type Stack = StackRef;
    fn inventory_items(&self) -> crate::crafting::Result<&[ItemStack]> {
        Ok(&self
            .transfer
            .operation
            .level
            .entity::<ServerPlayer>(self.transfer.player)
            .map_err(MenuError::from)
            .map_err(placement_error)?
            .inventory
            .items)
    }
    fn input_grid(&self) -> crate::crafting::Result<&[ItemStack]> {
        let items = self.items().map_err(placement_error)?;
        if self
            .menu()
            .map_err(placement_error)?
            .crafting_grid_size()
            .is_some()
        {
            Ok(items)
        } else {
            Ok(&items[..1])
        }
    }
    fn capture_grid_item(&mut self, index: usize) -> crate::crafting::Result<StackRef> {
        let offset = self.grid_offset().map_err(placement_error)?;
        self.transfer
            .slot_stack(index + offset)
            .map_err(placement_error)
    }
    fn stack<'a>(&'a self, reference: &'a StackRef) -> crate::crafting::Result<&'a ItemStack> {
        self.transfer
            .operation
            .level
            .stack(*reference)
            .map_err(placement_error)
    }
    fn fill_craft_slots(&self, contents: &mut StackedItemContents) -> crate::crafting::Result<()> {
        let crafting = self
            .menu()
            .map_err(placement_error)?
            .crafting_grid_size()
            .is_some();
        for stack in self.items().map_err(placement_error)? {
            if crafting {
                contents.account_simple_stack(stack)?;
            } else {
                contents.account_stack(stack, max_stack_size(stack));
            }
        }
        Ok(())
    }
    fn recipe_matches(&self, recipe: &RecipeHolder) -> crate::crafting::Result<bool> {
        let resources = self
            .transfer
            .operation
            .level
            .container_resources()
            .map_err(MenuError::from)
            .map_err(placement_error)?;
        let input = self.input_grid()?;
        if let Some(width) = self.menu().map_err(placement_error)?.crafting_grid_size() {
            let input = CraftingInputView::of_positioned(width, width, input)?.input;
            recipe
                .value
                .matches(RecipeInput::Crafting(&input), resources.crafting.as_ref())
        } else {
            recipe
                .value
                .matches(RecipeInput::Single(&input[0]), resources.crafting.as_ref())
        }
    }
    fn has_inventory_space_for(&self, stack: &ItemStack) -> crate::crafting::Result<bool> {
        let player = self
            .transfer
            .operation
            .level
            .entity::<ServerPlayer>(self.transfer.player)
            .map_err(MenuError::from)
            .map_err(placement_error)?;
        Ok(player
            .inventory
            .get_slot_with_remaining_space(stack, &player.parent.equipment)
            .is_some())
    }
    fn clear_grid(&mut self) -> crate::crafting::Result<()> {
        super::clear_grid::clear_grid(&mut self.transfer, self.hooks).map_err(placement_error)
    }
    fn take_inventory_item(
        &mut self,
        index: usize,
        amount: i32,
    ) -> crate::crafting::Result<StackRef> {
        let source = self
            .transfer
            .inventory_stack(index)
            .map_err(placement_error)?;
        if amount
            < count(
                self.transfer
                    .operation
                    .level
                    .stack(source)
                    .map_err(placement_error)?,
            )
        {
            let split = self
                .transfer
                .split_stack(source, amount)
                .map_err(placement_error)?;
            self.transfer.own_stack(split).map_err(placement_error)
        } else {
            // removeItemNoUpdate returns the original object. Move it to the
            // operation arena, preserving the old inventory ref through forwarding.
            let temporary = self
                .transfer
                .own_stack(ItemStack::default())
                .map_err(placement_error)?;
            self.transfer
                .operation
                .level
                .replace_stack_object(temporary.address(), StackValue::Existing(source))
                .map_err(placement_error)?;
            Ok(source)
        }
    }
    fn set_grid_item(&mut self, index: usize, stack: StackRef) -> crate::crafting::Result<()> {
        let offset = self.grid_offset().map_err(placement_error)?;
        self.transfer
            .set_slot_stack(index + offset, stack, None, self.hooks)
            .map_err(placement_error)
    }
    fn grow_grid_item(
        &mut self,
        _index: usize,
        target: &StackRef,
        amount: i32,
    ) -> crate::crafting::Result<()> {
        let total = count(
            self.transfer
                .operation
                .level
                .stack(*target)
                .map_err(placement_error)?,
        )
        .wrapping_add(amount);
        self.transfer
            .set_stack_count(*target, total)
            .map_err(placement_error)
    }
    fn inventory_changed(&mut self) -> crate::crafting::Result<()> {
        self.transfer
            .operation
            .level
            .entity_mut::<ServerPlayer>(self.transfer.player)
            .map_err(MenuError::from)
            .map_err(placement_error)?
            .inventory
            .set_changed();
        Ok(())
    }
}

/// Port of AbstractCraftingMenu/AbstractFurnaceMenu#handlePlacement, with the
/// CraftingMenu begin/finish bracket and its recompute on success or error.
pub fn place_live_menu_recipe(
    player: EntityRef,
    menu: MenuRef,
    recipe: &RecipeHolder,
    use_max_items: bool,
    allow_dropping: bool,
    hooks: &mut dyn MenuHooks,
    level: &mut Level,
) -> Result<crate::crafting::PostPlaceAction> {
    with_menu_callback(player, menu, level, |context| {
        let kind = context.menu()?.kind;
        let width = match kind {
            MenuKind::Inventory => 2,
            MenuKind::Crafting => 3,
            MenuKind::Furnace | MenuKind::Smoker | MenuKind::BlastFurnace => 1,
            _ => return Err(MenuError::Unsupported("non-recipe-book menu placement")),
        };
        let bracket = kind == MenuKind::Crafting;
        if bracket {
            context.menu_mut()?.placing_recipe = true;
        }
        let placed = crate::crafting::place_recipe(
            &mut LiveRecipePlacement {
                transfer: super::transfer::LiveMenuTransfer {
                    player,
                    menu,
                    operation: StackOperation::new(context.level),
                },
                hooks,
            },
            width,
            width,
            recipe,
            use_max_items,
            allow_dropping,
        );
        if bracket {
            context.menu_mut()?.placing_recipe = false;
            // Preserve the hint in the same result evaluator. The hook reads
            // actual book/rule facts, without holding the menu across callbacks.
            hooks.live_grid_changed_with_hint(player, menu, &recipe.id, context.level)?;
        }
        Ok(placed?)
    })
}
