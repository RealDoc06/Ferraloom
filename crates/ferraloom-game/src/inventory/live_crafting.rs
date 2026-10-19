//! Live recipe queries borrow the canonical grid only while the shared read-only
//! evaluator runs. ResultSlot's later callbacks keep generation handles instead.
use super::*;
use crate::{
    block::containers::runtime::ContainerResources,
    crafting::{CraftingInputView, RecipeInput, RecipeType},
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};
use std::sync::Arc;

/// Immutable recipe services and the actual rule captured by the packet owner.
/// Achievement producers remain explicit until the advancements slice lands.
pub struct LiveRecipeMenuHooks {
    pub resources: Arc<ContainerResources>,
    pub kind: MenuKind,
    pub limited_crafting: bool,
}
impl MenuHooks for LiveRecipeMenuHooks {
    fn has_crafting(&self) -> bool {
        matches!(self.kind, MenuKind::Inventory | MenuKind::Crafting)
    }
    fn prepare_recipe_click(&mut self, kind: MenuKind) -> Result<()> {
        if matches!(kind, MenuKind::Inventory | MenuKind::Crafting) {
            Ok(())
        } else {
            Err(MenuError::Unsupported("non-crafting result calculation"))
        }
    }
    fn live_grid_changed(
        &mut self,
        player: EntityRef,
        menu: MenuRef,
        level: &mut Level,
    ) -> Result<()> {
        refresh_live_crafting_result(player, menu, self.limited_crafting, level)
    }
    fn result_available(&self, kind: ResultKind, _stack: &ItemStack) -> Result<bool> {
        if matches!(kind, ResultKind::Crafting | ResultKind::Furnace) {
            Ok(true)
        } else {
            Err(MenuError::Unsupported("non-crafting result pickup"))
        }
    }
    fn live_grid_changed_with_hint(
        &mut self,
        player: EntityRef,
        menu: MenuRef,
        hint: &ferraloom_data::datapack::Identifier,
        level: &mut Level,
    ) -> Result<()> {
        refresh_live_crafting_result_with_hint(
            player,
            menu,
            self.limited_crafting,
            Some(hint),
            level,
        )
    }
    fn can_smelt(&self, stack: &ItemStack) -> Result<bool> {
        let recipe_type = match self.kind {
            MenuKind::BlastFurnace => RecipeType::Blasting,
            MenuKind::Smoker => RecipeType::Smoking,
            MenuKind::Furnace => RecipeType::Smelting,
            _ => return Err(MenuError::Unsupported("non-furnace cooking property set")),
        };
        Ok(self
            .resources
            .recipes
            .get_recipe_for(
                recipe_type,
                RecipeInput::Single(stack),
                self.resources.crafting.as_ref(),
                None,
            )?
            .is_some())
    }
}

/// Port of CraftingMenu#slotChangedCraftingGrid / InventoryMenu#slotsChanged.
/// Recipe matching and assembly use borrowed live cells, with no grid snapshot.
pub fn refresh_live_crafting_result(
    player: EntityRef,
    menu: MenuRef,
    limited_crafting: bool,
    level: &mut Level,
) -> Result<()> {
    refresh_live_crafting_result_with_hint(player, menu, limited_crafting, None, level)
}
pub(super) fn refresh_live_crafting_result_with_hint(
    player: EntityRef,
    menu: MenuRef,
    limited_crafting: bool,
    hint: Option<&ferraloom_data::datapack::Identifier>,
    level: &mut Level,
) -> Result<()> {
    let resources = level.container_resources()?;
    let (stack, recipe) = {
        let owner = level.entity::<ServerPlayer>(player)?;
        let menu = owner.menus.menu(menu)?;
        if menu.placing_recipe {
            return Ok(());
        }
        let width = menu
            .crafting_grid_size()
            .ok_or(MenuError::Unsupported("non-crafting grid"))?;
        let input = CraftingInputView::of_positioned(width, width, menu.crafting_grid())?.input;
        let recipe = resources.recipes.get_recipe_for(
            RecipeType::Crafting,
            RecipeInput::Crafting(&input),
            resources.crafting.as_ref(),
            hint,
        )?;
        if let Some(recipe) = recipe
            && (recipe.value.is_special()
                || !limited_crafting
                || owner.recipe_book.contains(&recipe.id))
        {
            let mut result = recipe
                .value
                .assemble(RecipeInput::Crafting(&input), resources.crafting.as_ref())?;
            if !result.item().is_enabled_in_vanilla() {
                result = ItemStack::default();
            }
            (result, Some(recipe.id.clone()))
        } else {
            (ItemStack::default(), None)
        }
    };
    let address = ContainerSlot {
        container: ContainerHandle::MenuLocal {
            player,
            menu,
            container: 1,
        },
        slot: 0,
    };
    level.replace_stack_object(address, StackValue::New(stack))?;
    let stack = level.stack(level.stack_ref(address)?)?.clone();
    let receiver = level
        .entity_mut::<ServerPlayer>(player)?
        .menus
        .menu_mut(menu)?;
    if let Some(recipe) = recipe {
        receiver.crafting_recipe_used = Some(recipe);
    }
    receiver.set_remote_slot(0, &stack);
    let state_id = receiver.increment_state_id();
    let update = ContainerUpdate::Slot {
        container_id: receiver.container_id,
        state_id,
        slot: 0,
        stack,
    };
    // Native connection.send happens even without a synchronizer and while
    // suppressRemoteUpdates is active, before the next grid mutation.
    send_container_packets(player, update.into_packets(), level)
}

impl CraftingResultContext for super::transfer::LiveMenuTransfer<'_> {
    fn validate_crafting_take(&self, hooks: &dyn MenuHooks) -> Result<()> {
        hooks.validate_live_result(ResultKind::Crafting)
    }
    fn crafted_item_only(&mut self, stack: StackRef, _hooks: &mut dyn MenuHooks) -> Result<()> {
        crafted_item_post_process(self.operation.level, stack)
    }
    fn crafted_item(
        &mut self,
        stack: StackRef,
        amount: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        hooks.live_item_crafted(self.player, self.menu, stack, amount, self.operation.level)
    }
    fn award_crafting_recipe(
        &mut self,
        recipe: &ferraloom_data::datapack::Identifier,
        input: &[StackRef],
        hooks: &mut dyn MenuHooks,
    ) -> Result<bool> {
        hooks.live_award_recipe(self.player, self.menu, recipe, input, self.operation.level)
    }
    fn crafting_remainders(
        &mut self,
        _hooks: &mut dyn MenuHooks,
    ) -> Result<CraftingRemainders<StackRef>> {
        use crate::crafting::CraftingInputAccess;
        let resources = self.operation.level.container_resources()?;
        let (grid_width, width, height, left, top, remaining) = {
            let menu = self.menu()?;
            let grid_width = menu
                .crafting_grid_size()
                .ok_or(MenuError::Unsupported("non-crafting grid"))?;
            let positioned =
                CraftingInputView::of_positioned(grid_width, grid_width, menu.crafting_grid())?;
            let input = &positioned.input;
            let remaining = resources
                .recipes
                .get_crafting_recipe(input, resources.crafting.as_ref())?
                .map(|recipe| recipe.value.remaining_items(input))
                .transpose()?;
            (
                grid_width,
                input.width(),
                input.height(),
                positioned.left,
                positioned.top,
                remaining,
            )
        };
        let items = match remaining {
            Some(items) => items
                .into_iter()
                .map(|item| self.operation.own(self.player, item))
                .collect::<Result<Vec<_>>>()?,
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

impl super::transfer::LiveMenuTransfer<'_> {
    /// Port of FurnaceResultSlot#checkTakeAchievements. The XP/recipe producer
    /// runs before resetting removeCount, and onTake invokes Slot#setChanged later.
    pub(super) fn check_furnace_achievements(
        &mut self,
        index: usize,
        stack: StackRef,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let amount = self.menu()?.furnace_remove_count;
        hooks.live_item_crafted(self.player, self.menu, stack, amount, self.operation.level)?;
        let address = self.context().slot_address(index)?;
        if let ContainerHandle::BlockEntity(furnace) = address.container {
            hooks.live_award_furnace(self.player, furnace, self.operation.level)?;
        }
        self.menu_mut()?.furnace_remove_count = 0;
        Ok(())
    }
}
