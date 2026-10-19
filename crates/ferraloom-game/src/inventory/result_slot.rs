//! ResultSlot bookkeeping is shared; each adapter delivers its native effects
//! after ending the short storage borrow and keeps the captured receivers rooted.
use super::*;
use ferraloom_data::datapack::Identifier;

pub trait CraftingResultContext: MenuClickContext {
    fn validate_crafting_take(&self, hooks: &dyn MenuHooks) -> Result<()>;
    fn crafted_item_only(&mut self, stack: Self::Stack, hooks: &mut dyn MenuHooks) -> Result<()>;
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
    fn validate_crafting_take(&self, hooks: &dyn MenuHooks) -> Result<()> {
        hooks
            .crafting_ref()
            .ok_or(MenuError::Unsupported("crafting callbacks"))?
            .validate_take_result()
    }
    fn crafted_item_only(&mut self, stack: Self::Stack, hooks: &mut dyn MenuHooks) -> Result<()> {
        let stack = self.read(stack)?.clone();
        hooks
            .crafting()
            .ok_or(MenuError::Unsupported("crafting callbacks"))?
            .item_crafted(self.menu, self.context, &stack, None)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crafting::{CraftingInputAccess, RecipeHolder},
        entity::player::{equipment::EntityEquipment, inventory::Inventory},
    };
    use ferraloom_data::Item;

    fn describe(stack: &ItemStack) -> String {
        if stack.is_empty() {
            "-".into()
        } else {
            format!(
                "{},{},{}",
                stack.item().protocol_id(),
                count(stack),
                max_stack_size(stack)
            )
        }
    }
    #[derive(Default)]
    struct ReentrantCrafting(Vec<String>);
    impl MenuHooks for ReentrantCrafting {
        fn has_crafting(&self) -> bool {
            true
        }
        fn crafting(&mut self) -> Option<&mut dyn CraftingMenuHooks> {
            Some(self)
        }
    }
    impl CraftingMenuHooks for ReentrantCrafting {
        fn validate_take_result(&self) -> Result<()> {
            Ok(())
        }
        fn result(
            &self,
            _: &dyn CraftingInputAccess,
            _: Option<&Identifier>,
        ) -> Result<CraftingResult> {
            Err(MenuError::Unsupported("unused result query"))
        }
        fn remaining(&self, _: &dyn CraftingInputAccess) -> Result<Option<Vec<ItemStack>>> {
            Err(MenuError::Unsupported("unused remainder query"))
        }
        fn matches(&self, _: &RecipeHolder, _: &dyn CraftingInputAccess) -> Result<bool> {
            Err(MenuError::Unsupported("unused recipe query"))
        }
        fn item_crafted(
            &mut self,
            menu: &mut AbstractContainerMenu,
            context: &mut MenuContext<'_>,
            _: &ItemStack,
            amount: Option<i32>,
        ) -> Result<()> {
            self.0.push(format!("crafted:{}", amount.unwrap()));
            menu.set_item(
                context,
                1,
                ItemStack::new(Item::DIRT, 3, Default::default()),
            );
            menu.crafting_remove_count = 19;
            Ok(())
        }
        fn award(
            &mut self,
            menu: &mut AbstractContainerMenu,
            context: &mut MenuContext<'_>,
            _: &Identifier,
            input: &[ItemStack],
        ) -> Result<bool> {
            self.0.push(format!(
                "award:{}:{}:{}",
                menu.crafting_remove_count,
                input.len(),
                describe(&input[0])
            ));
            menu.set_item(
                context,
                1,
                ItemStack::new(Item::DIAMOND, 7, Default::default()),
            );
            self.0.push(format!("captured:{}", describe(&input[0])));
            Ok(true)
        }
    }

    #[test]
    fn crafting_achievements_capture_inputs_after_item_callbacks_and_reset_after_awards() {
        let mut cases = 0;
        for line in include_str!("../../tests/java/inventory/listeners.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let Some(amount) = fields[1].strip_prefix("result-achievements-") else {
                continue;
            };
            let mut inventory = Inventory::default();
            let mut equipment = EntityEquipment::default();
            let mut context = MenuContext::player(&mut inventory, &mut equipment, false);
            let mut menu = AbstractContainerMenu::new(MenuKind::Inventory, 0).unwrap();
            menu.set_item(
                &mut context,
                1,
                ItemStack::new(Item::STONE, 5, Default::default()),
            );
            menu.crafting_remove_count = amount.parse().unwrap();
            menu.crafting_recipe_used = Some("test:used".parse().unwrap());
            let mut hooks = ReentrantCrafting::default();
            {
                let mut operation =
                    super::super::in_memory::InMemoryMenuOperation::new(&mut menu, &mut context);
                let output = operation.own(ItemStack::new(Item::STONE, 4, Default::default()));
                check_crafting_take_achievements(&mut operation, output, &mut hooks).unwrap();
            }
            assert!(menu.crafting_recipe_used.is_none());
            hooks.0.push(format!(
                "after:{}:{}",
                menu.crafting_remove_count,
                describe(&menu.get_item(&context, 1))
            ));
            assert_eq!(hooks.0.join(";"), fields[2], "{line}");
            cases += 1;
        }
        assert_eq!(cases, 2);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::{
        entity::{
            EntityState,
            player::{GameType, ServerPlayer},
        },
        level::Level,
    };
    use ferraloom_data::Item;
    use ferraloom_util::{Uuid, Vec3};

    #[derive(Default)]
    struct Reentrant(Vec<String>);
    fn describe(stack: &ItemStack) -> String {
        format!(
            "{},{},{}",
            stack.item().protocol_id(),
            count(stack),
            max_stack_size(stack)
        )
    }
    impl MenuHooks for Reentrant {
        fn live_item_crafted(
            &mut self,
            player: crate::entity::EntityRef,
            menu: MenuRef,
            _stack: StackRef,
            amount: i32,
            level: &mut Level,
        ) -> Result<()> {
            self.0.push(format!("crafted:{amount}"));
            level.replace_stack_object(
                ContainerSlot {
                    container: ContainerHandle::MenuLocal {
                        player,
                        menu,
                        container: 0,
                    },
                    slot: 0,
                },
                StackValue::New(ItemStack::new(Item::DIRT, 3, Default::default())),
            )?;
            level
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .menu_mut(menu)?
                .crafting_remove_count = 19;
            Ok(())
        }
        fn live_award_recipe(
            &mut self,
            player: crate::entity::EntityRef,
            menu: MenuRef,
            _recipe: &Identifier,
            input: &[StackRef],
            level: &mut Level,
        ) -> Result<bool> {
            self.0.push(format!(
                "award:{}:{}:{}",
                level
                    .entity::<ServerPlayer>(player)?
                    .menus
                    .menu(menu)?
                    .crafting_remove_count,
                input.len(),
                describe(level.stack(input[0])?)
            ));
            level.replace_stack_object(
                ContainerSlot {
                    container: ContainerHandle::MenuLocal {
                        player,
                        menu,
                        container: 0,
                    },
                    slot: 0,
                },
                StackValue::New(ItemStack::new(Item::DIAMOND, 7, Default::default())),
            )?;
            self.0
                .push(format!("captured:{}", describe(level.stack(input[0])?)));
            Ok(true)
        }
    }
    #[test]
    fn live_result_achievements_keep_native_input_receivers_across_reentry() {
        let mut checked = 0;
        for line in include_str!("../../tests/java/inventory/listeners.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let Some(amount) = fields[1].strip_prefix("result-achievements-") else {
                continue;
            };
            let mut level = crate::entity::tests::level(false);
            let player = level.next_entity_ref();
            let mut state = ServerPlayer::new(
                Uuid::new(1, 2),
                "result".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            let menu = state.menus.current_ref();
            let receiver = state.menus.menu_mut(menu).unwrap();
            receiver.local[0].set_item(0, ItemStack::new(Item::STONE, 5, Default::default()));
            receiver.crafting_remove_count = amount.parse().unwrap();
            receiver.crafting_recipe_used = Some("test:used".parse().unwrap());
            level
                .add_entity(player, EntityState::Player(Box::new(state)))
                .unwrap();
            let mut hooks = Reentrant::default();
            with_menu_callback(player, menu, &mut level, |context| {
                let mut context = super::super::transfer::LiveMenuTransfer {
                    operation: StackOperation::new(context.level),
                    player,
                    menu,
                };
                let stack =
                    context.own_stack(ItemStack::new(Item::STONE, 4, Default::default()))?;
                check_crafting_take_achievements(&mut context, stack, &mut hooks)?;
                assert!(context.menu()?.crafting_recipe_used.is_none());
                let input = context.slot_stack(1)?;
                hooks.0.push(format!(
                    "after:{}:{}",
                    context.menu()?.crafting_remove_count,
                    describe(context.operation.level.stack(input)?)
                ));
                Ok(())
            })
            .unwrap();
            assert_eq!(hooks.0.join(";"), fields[2]);
            checked += 1;
        }
        assert_eq!(checked, 2);
    }
}
