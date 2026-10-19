//! Synchronous crafting callbacks at the menu's canonical mutation sites.
use super::*;
use crate::crafting::{
    CraftingContext, CraftingInputAccess, CraftingInputView, RecipeHolder, RecipeInput,
    RecipeManager, RecipeSynchronization, RecipeType, ServerRecipeBook,
};
use ferraloom_data::datapack::Identifier;

/// Region-owned gameplay callbacks. Called synchronously with the live menu and
/// inventory; implementations may reenter those values before crafting resumes.
pub trait CraftingCallbacks {
    fn validate_take_result(&self) -> Result<()>;
    fn item_enabled(&self, stack: &ItemStack) -> Result<bool>;
    /// Some(count) is ItemStack#onCraftedBy (stat followed by item callback);
    /// None is Item#onCraftedBy alone (quick move and creative clone).
    fn item_crafted(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        count: Option<i32>,
    ) -> Result<()>;
    fn recipe_crafted(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        recipe: &RecipeHolder,
        input: &[ItemStack],
    ) -> Result<()>;
    fn recipe_unlocked(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        recipe: &RecipeHolder,
        book: &ServerRecipeBook,
    ) -> Result<()>;
}

pub struct CraftingResult {
    pub stack: ItemStack,
    /// None leaves ResultContainer's previous recipe-used reference intact.
    pub recipe: Option<Identifier>,
}

/// Menu-facing object-safe bridge. The concrete adapter below retains the
/// recipe module's typed runtime CraftingContext rather than substituting ().
pub trait CraftingMenuHooks {
    fn validate_take_result(&self) -> Result<()>;
    fn result(
        &self,
        input: &dyn CraftingInputAccess,
        hint: Option<&Identifier>,
    ) -> Result<CraftingResult>;
    /// None is vanilla's copyAllInputItems fallback, retaining live slot aliases.
    fn remaining(&self, input: &dyn CraftingInputAccess) -> Result<Option<Vec<ItemStack>>>;
    fn matches(&self, recipe: &RecipeHolder, input: &dyn CraftingInputAccess) -> Result<bool>;
    fn item_crafted(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        count: Option<i32>,
    ) -> Result<()>;
    /// Returns whether the used recipe is cleared (ordinary recipes, not special).
    fn award(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        recipe: &Identifier,
        input: &[ItemStack],
    ) -> Result<bool>;
}

/// Port of CraftingMenu#slotChangedCraftingGrid and RecipeCraftingHolder using
/// one loaded recipe/registry snapshot. The caller supplies actual world facts.
pub struct RecipeMenuHooks<'a, C> {
    pub manager: &'a RecipeManager,
    pub context: &'a C,
    pub synchronization: &'a RecipeSynchronization,
    pub book: &'a mut ServerRecipeBook,
    pub limited_crafting: Option<bool>,
    pub callbacks: &'a mut dyn CraftingCallbacks,
}
impl<C: CraftingContext> MenuHooks for RecipeMenuHooks<'_, C> {
    fn crafting_ref(&self) -> Option<&dyn CraftingMenuHooks> {
        Some(self)
    }
    fn crafting(&mut self) -> Option<&mut dyn CraftingMenuHooks> {
        Some(self)
    }
    fn has_crafting(&self) -> bool {
        true
    }
    fn prepare_recipe_click(&mut self, kind: MenuKind) -> Result<()> {
        if matches!(kind, MenuKind::Inventory | MenuKind::Crafting) {
            self.limited_crafting.ok_or(MenuError::Unsupported(
                "canonical LIMITED_CRAFTING game rule",
            ))?;
            Ok(())
        } else {
            Err(MenuError::Unsupported("non-crafting result calculation"))
        }
    }
    fn result_available(&self, kind: ResultKind, _stack: &ItemStack) -> Result<bool> {
        if kind == ResultKind::Crafting {
            Ok(true)
        } else {
            Err(MenuError::Unsupported("non-crafting result pickup"))
        }
    }
}
impl<C: CraftingContext> CraftingMenuHooks for RecipeMenuHooks<'_, C> {
    fn validate_take_result(&self) -> Result<()> {
        self.callbacks.validate_take_result()
    }
    fn result(
        &self,
        input: &dyn CraftingInputAccess,
        hint: Option<&Identifier>,
    ) -> Result<CraftingResult> {
        let recipe = self.manager.get_recipe_for(
            RecipeType::Crafting,
            RecipeInput::Crafting(input),
            self.context,
            hint,
        )?;
        if let Some(recipe) = recipe
            && (recipe.value.is_special()
                || !self
                    .limited_crafting
                    .ok_or(crate::crafting::Error::ContextRequired(
                        "canonical LIMITED_CRAFTING game rule",
                    ))?
                || self.book.contains(&recipe.id))
        {
            let mut stack = recipe
                .value
                .assemble(RecipeInput::Crafting(input), self.context)?;
            if !self.callbacks.item_enabled(&stack)? {
                stack = ItemStack::default();
            }
            Ok(CraftingResult {
                stack,
                recipe: Some(recipe.id.clone()),
            })
        } else {
            Ok(CraftingResult {
                stack: ItemStack::default(),
                recipe: None,
            })
        }
    }
    fn remaining(&self, input: &dyn CraftingInputAccess) -> Result<Option<Vec<ItemStack>>> {
        self.manager
            .get_crafting_recipe(input, self.context)?
            .map(|recipe| recipe.value.remaining_items(input).map_err(MenuError::from))
            .transpose()
    }
    fn matches(&self, recipe: &RecipeHolder, input: &dyn CraftingInputAccess) -> Result<bool> {
        Ok(recipe
            .value
            .matches(RecipeInput::Crafting(input), self.context)?)
    }
    fn item_crafted(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        count: Option<i32>,
    ) -> Result<()> {
        self.callbacks.item_crafted(menu, ctx, stack, count)
    }
    fn award(
        &mut self,
        menu: &mut AbstractContainerMenu,
        ctx: &mut MenuContext<'_>,
        id: &Identifier,
        input: &[ItemStack],
    ) -> Result<bool> {
        let recipe = self.manager.by_key(id).ok_or(MenuError::Unsupported(
            "recipe used from a retired recipe snapshot",
        ))?;
        self.callbacks.recipe_crafted(menu, ctx, recipe, input)?;
        if recipe.value.is_special() {
            return Ok(false);
        }
        if let Some(entries) = self.book.unlock_recipe(recipe, self.synchronization) {
            self.callbacks
                .recipe_unlocked(menu, ctx, recipe, self.book)?;
            if !entries.is_empty() {
                menu.updates.push(ContainerUpdate::RecipeBookAdd(
                    ferraloom_protocol::play::clientbound::RecipeBookAdd {
                        entries,
                        replace: false,
                    },
                ));
            }
        }
        Ok(true)
    }
}

impl AbstractContainerMenu {
    pub fn crafting_grid_size(&self) -> Option<usize> {
        match self.kind {
            MenuKind::Inventory => Some(2),
            MenuKind::Crafting => Some(3),
            _ => None,
        }
    }
    pub(super) fn crafting_grid(&self) -> &[ItemStack] {
        match &self.local[0] {
            MenuContainer::Crafting(c) => c.items(),
            _ => unreachable!("crafting grid"),
        }
    }
    fn crafting_input(&self) -> Result<crate::crafting::PositionedCraftingInputView<'_>> {
        let width = self
            .crafting_grid_size()
            .ok_or(MenuError::Unsupported("non-crafting menu"))?;
        Ok(CraftingInputView::of_positioned(
            width,
            width,
            self.crafting_grid(),
        )?)
    }
    /// Port of TransientCraftingContainer#setItem followed immediately by
    /// slotsChanged. Receiver-side initialization continues using raw set_item.
    pub fn set_item_live(
        &mut self,
        ctx: &mut MenuContext<'_>,
        index: usize,
        stack: ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let slot = self.slots[index].clone();
        if self.crafting_grid_size().is_some()
            && slot.container == ContainerId::Local(0)
            && hooks.has_crafting()
        {
            hooks.prepare_recipe_click(self.kind)?;
        }
        self.set_item(ctx, index, stack);
        if self.crafting_grid_size().is_some() && slot.container == ContainerId::Local(0) {
            self.crafting_slots_changed(hooks, None)?;
        }
        Ok(())
    }
    pub fn crafting_slots_changed(
        &mut self,
        hooks: &mut dyn MenuHooks,
        hint: Option<&Identifier>,
    ) -> Result<()> {
        if self.placing_recipe {
            return Ok(());
        }
        let input = self.crafting_input()?.input;
        let Some(crafting) = hooks.crafting() else {
            return Ok(());
        };
        let result = crafting.result(&input, hint)?;
        if let Some(recipe) = result.recipe {
            self.crafting_recipe_used = Some(recipe);
        }
        self.local[1].set_item(0, result.stack.clone());
        self.set_remote_slot(0, &result.stack);
        // This is a direct ServerPlayer connection send, independent of both
        // ContainerSynchronizer installation and suppressRemoteUpdates.
        let state_id = self.increment_state_id();
        self.updates.push(ContainerUpdate::Slot {
            container_id: self.container_id,
            state_id,
            slot: 0,
            stack: result.stack,
        });
        Ok(())
    }
    pub(crate) fn crafting_check_achievements(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        let mut context = super::in_memory::InMemoryMenuOperation::new(self, ctx);
        let stack = context.own(stack.clone());
        check_crafting_take_achievements(&mut context, stack, hooks)
    }
    /// Port of ResultSlot#onTake. Remainders are matched after achievement
    /// callbacks, which may have changed the live crafting input.
    pub(crate) fn take_crafting_result(
        &mut self,
        ctx: &mut MenuContext<'_>,
        stack: &ItemStack,
        swap_count: i32,
        hooks: &mut dyn MenuHooks,
    ) -> Result<()> {
        self.crafting_remove_count = self.crafting_remove_count.wrapping_add(swap_count);
        let mut context = super::in_memory::InMemoryMenuOperation::new(self, ctx);
        let stack = context.own(stack.clone());
        take_crafting_result_stack(&mut context, stack, hooks)
    }
}

struct CraftingPlacement<'a, 'b, 'p> {
    operation: super::in_memory::InMemoryMenuOperation<'a, 'b, 'p>,
    hooks: &'b mut dyn MenuHooks,
}
pub(super) fn placement_error(error: MenuError) -> crate::crafting::Error {
    match error {
        MenuError::Crafting(error) => error,
        MenuError::Protocol(error) => crate::crafting::Error::Protocol(error),
        MenuError::Unsupported(operation) => crate::crafting::Error::ContextRequired(operation),
        error => crate::crafting::Error::Invalid(error.to_string()),
    }
}
impl crate::crafting::RecipePlacementAccess for CraftingPlacement<'_, '_, '_> {
    type Stack = super::in_memory::MemoryStackRef;
    fn inventory_items(&self) -> crate::crafting::Result<&[ItemStack]> {
        Ok(&self.operation.context.player.inventory.items)
    }
    fn input_grid(&self) -> crate::crafting::Result<&[ItemStack]> {
        Ok(self.operation.menu.crafting_grid())
    }
    fn capture_grid_item(&mut self, index: usize) -> crate::crafting::Result<Self::Stack> {
        self.operation
            .slot_stack(index + 1)
            .map_err(placement_error)
    }
    fn stack<'a>(&'a self, receiver: &'a Self::Stack) -> crate::crafting::Result<&'a ItemStack> {
        self.operation.read(*receiver).map_err(placement_error)
    }
    fn fill_craft_slots(
        &self,
        contents: &mut crate::crafting::StackedItemContents,
    ) -> crate::crafting::Result<()> {
        for stack in self.operation.menu.crafting_grid() {
            contents.account_simple_stack(stack)?;
        }
        Ok(())
    }
    fn recipe_matches(&self, recipe: &RecipeHolder) -> crate::crafting::Result<bool> {
        let input = self
            .operation
            .menu
            .crafting_input()
            .map_err(placement_error)?
            .input;
        self.hooks
            .crafting_ref()
            .expect("crafting hook")
            .matches(recipe, &input)
            .map_err(placement_error)
    }
    fn has_inventory_space_for(&self, stack: &ItemStack) -> crate::crafting::Result<bool> {
        Ok(self
            .operation
            .context
            .player
            .inventory
            .get_slot_with_remaining_space(stack, self.operation.context.player.equipment)
            .is_some())
    }
    fn clear_grid(&mut self) -> crate::crafting::Result<()> {
        super::clear_grid::clear_grid(&mut self.operation, self.hooks).map_err(placement_error)
    }
    fn take_inventory_item(
        &mut self,
        index: usize,
        amount: i32,
    ) -> crate::crafting::Result<Self::Stack> {
        let source = MenuClickContext::inventory_stack(&mut self.operation, index)
            .map_err(placement_error)?;
        if amount < count(self.operation.read(source).map_err(placement_error)?) {
            let taken = self
                .operation
                .split_stack(source, amount)
                .map_err(placement_error)?;
            Ok(self.operation.own(taken))
        } else {
            self.operation
                .detach(
                    super::in_memory::MemoryContainer::Container(ContainerId::Player),
                    index,
                )
                .map_err(placement_error)
        }
    }
    fn set_grid_item(&mut self, index: usize, stack: Self::Stack) -> crate::crafting::Result<()> {
        self.operation
            .set_slot_stack(index + 1, stack, None, self.hooks)
            .map_err(placement_error)
    }
    fn grow_grid_item(
        &mut self,
        _index: usize,
        receiver: &Self::Stack,
        amount: i32,
    ) -> crate::crafting::Result<()> {
        let amount =
            count(self.operation.read(*receiver).map_err(placement_error)?).wrapping_add(amount);
        MenuTransferContext::set_stack_count(&mut self.operation, *receiver, amount)
            .map_err(placement_error)
    }
    fn inventory_changed(&mut self) -> crate::crafting::Result<()> {
        self.operation.context.player.inventory.set_changed();
        Ok(())
    }
}

impl AbstractContainerMenu {
    /// Port of AbstractCraftingMenu#handlePlacement. Only CraftingMenu overrides
    /// the placement bracket; InventoryMenu's inherited begin/finish are no-ops.
    /// The finish callback runs on both success and failure, like Java finally.
    pub fn handle_crafting_placement(
        &mut self,
        ctx: &mut MenuContext<'_>,
        recipe: &RecipeHolder,
        use_max_items: bool,
        allow_dropping: bool,
        hooks: &mut dyn MenuHooks,
    ) -> Result<crate::crafting::PostPlaceAction> {
        let width = self
            .crafting_grid_size()
            .ok_or(MenuError::Unsupported("non-crafting recipe placement"))?;
        if !hooks.has_crafting() {
            return Err(MenuError::Unsupported("crafting recipe placement runtime"));
        }
        let bracket = self.kind == MenuKind::Crafting;
        self.placing_recipe = bracket;
        let placed = crate::crafting::place_recipe(
            &mut CraftingPlacement {
                operation: super::in_memory::InMemoryMenuOperation::new(self, ctx),
                hooks,
            },
            width,
            width,
            recipe,
            use_max_items,
            allow_dropping,
        );
        if bracket {
            self.placing_recipe = false;
            self.crafting_slots_changed(hooks, Some(&recipe.id))?;
        }
        Ok(placed?)
    }
}
