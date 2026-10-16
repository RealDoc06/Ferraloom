use pumpkin_data::{
    data_component_impl::{CookingFuelImpl, IntProvider},
    item_stack::ItemStack,
    recipes::CookingRecipe,
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

use crate::block::entities::{BlockEntity, PropertyDelegate};
pub use pumpkin_inventory::ExperienceContainer;
use pumpkin_inventory::{Clearable, Inventory};

/// `TagValueInput.getIntOr` accepts every `NumericTag`. `Float`/`Double.intValue`
/// floors before Java's saturating conversion; `Long.intValue` keeps its low bits.
pub(crate) fn read_cooking_int(nbt: &NbtCompound, key: &str) -> i32 {
    match nbt.get(key) {
        Some(NbtTag::Byte(value)) => i32::from(*value),
        Some(NbtTag::Short(value)) => i32::from(*value),
        Some(NbtTag::Int(value)) => *value,
        Some(NbtTag::Long(value)) => *value as i32,
        Some(NbtTag::Float(value)) => value.floor() as i32,
        Some(NbtTag::Double(value)) => value.floor() as i32,
        _ => 0,
    }
}

pub(crate) fn read_cooking_speed(nbt: &NbtCompound) -> f32 {
    match nbt.get("speed_multiplier") {
        Some(NbtTag::Byte(value)) => f32::from(*value),
        Some(NbtTag::Short(value)) => f32::from(*value),
        Some(NbtTag::Int(value)) => *value as f32,
        Some(NbtTag::Long(value)) => *value as f32,
        Some(NbtTag::Float(value)) => *value,
        Some(NbtTag::Double(value)) => *value as f32,
        _ => 1.0,
    }
}

pub(crate) fn cooking_time(recipe_time: i32, speed: f32) -> i32 {
    if speed > 0.0 {
        (recipe_time as f32 / speed).ceil() as i32
    } else {
        recipe_time
    }
}

pub(crate) fn rescale_cooking_progress(spent: i32, total: i32, new_total: i32) -> i32 {
    (spent as f32 / total as f32 * new_total as f32).ceil() as i32
}

pub(crate) fn cooking_fuel_duration(fuel: &CookingFuelImpl, is_fast: bool) -> i32 {
    match &fuel.burn_time {
        // A ResolvableInt.Constant is already resolved, including its sign/range.
        IntProvider::Inline(value) => *value,
        IntProvider::Id(_) => i32::from(fuel.get_burn_ticks(is_fast)),
    }
}

pub(crate) fn write_cooking_state(entity: &impl CookingBlockEntityBase, nbt: &mut NbtCompound) {
    nbt.put_int("cooking_total_time", entity.get_cooking_total_time());
    nbt.put_int("cooking_time_spent", entity.get_cooking_time_spent());
    nbt.put_int("lit_total_time", entity.get_lit_total_time());
    nbt.put_int("lit_time_remaining", entity.get_lit_time_remaining());
    nbt.put_float("speed_multiplier", entity.get_speed_multiplier());
}

/// Trait for extracting smelting experience from cooking block entities.
pub trait CookingBlockEntityBase:
    Sync + Send + Inventory + PropertyDelegate + BlockEntity + Clearable
{
    fn get_cooking_time_spent(&self) -> i32;
    fn get_cooking_total_time(&self) -> i32;
    fn get_lit_time_remaining(&self) -> i32;
    fn get_lit_total_time(&self) -> i32;
    fn get_speed_multiplier(&self) -> f32;

    /// Track that a recipe was used (for XP calculation on extraction)
    /// Uses the namespaced recipe ID as the recipe identifier.
    fn add_recipe_used(&self, recipe: &CookingRecipe);
    /// Extract and reset accumulated experience, returning the total as an integer
    /// Calculates XP from tracked recipes and clears the `recipes_used` map
    fn extract_experience_from_recipes(&self) -> i32;

    fn get_input_item(&self) -> ItemStack;
    fn get_fuel_item(&self) -> ItemStack;
    fn get_output_item(&self) -> ItemStack;

    fn set_cooking_time_spent(&self, spent_time: i32);
    fn set_cooking_total_time(&self, total_time: i32);
    fn set_lit_time_remaining(&self, remaining_time: i32);
    fn set_lit_total_time(&self, total_time: i32);

    fn is_burning(&self) -> bool;
    fn can_accept_recipe_output(&self, recipe: Option<&CookingRecipe>, max_count: u8) -> bool;
    fn craft_recipe(&self, recipe: Option<&CookingRecipe>) -> bool;
}

#[macro_export]
macro_rules! impl_cooking_block_entity_base {
    ($struct_name:ty) => {
        impl CookingBlockEntityBase for $struct_name {
            fn get_cooking_time_spent(&self) -> i32 {
                self.cooking_time_spent.load(Ordering::Relaxed)
            }

            fn get_cooking_total_time(&self) -> i32 {
                self.cooking_total_time.load(Ordering::Relaxed)
            }

            fn get_lit_time_remaining(&self) -> i32 {
                self.lit_time_remaining.load(Ordering::Relaxed)
            }

            fn get_lit_total_time(&self) -> i32 {
                self.lit_total_time.load(Ordering::Relaxed)
            }

            fn get_speed_multiplier(&self) -> f32 {
                self.speed_multiplier.load()
            }

            fn get_input_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[0].clone()
            }

            fn get_fuel_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[1].clone()
            }

            fn get_output_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[2].clone()
            }

            fn set_cooking_time_spent(&self, spent_time: i32) {
                self.cooking_time_spent.store(spent_time, Ordering::Relaxed);
            }

            fn set_cooking_total_time(&self, total_time: i32) {
                self.cooking_total_time.store(total_time, Ordering::Relaxed);
            }

            fn set_lit_time_remaining(&self, remaining_time: i32) {
                self.lit_time_remaining
                    .store(remaining_time, Ordering::Relaxed);
            }

            fn set_lit_total_time(&self, total_time: i32) {
                self.lit_total_time.store(total_time, Ordering::Relaxed);
            }

            fn is_burning(&self) -> bool {
                self.get_lit_time_remaining() > 0
            }

            fn add_recipe_used(&self, recipe: &pumpkin_data::recipes::CookingRecipe) {
                // Track recipe usage by recipe ID for XP calculation
                let recipe_id = recipe.recipe_id.to_string();
                let mut recipes = self
                    .recipes_used
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *recipes.entry(recipe_id).or_insert(0) += 1;
            }

            fn extract_experience_from_recipes(&self) -> i32 {
                // Calculate total XP from tracked recipes and clear the map (vanilla behavior)
                let mut recipes = self
                    .recipes_used
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut total_xp: f32 = 0.0;
                for (recipe_id, count) in recipes.iter() {
                    // Look up the recipe's XP value
                    if let Some(xp) = pumpkin_data::recipes::get_recipe_experience(recipe_id) {
                        total_xp += xp * (*count as f32);
                    }
                }
                recipes.clear();
                total_xp.floor() as i32
            }

            fn can_accept_recipe_output(
                &self,
                recipe: Option<&pumpkin_data::recipes::CookingRecipe>,
                max_count: u8,
            ) -> bool {
                let Some(recipe) = recipe else { return false };
                let Ok(items) = self.items.try_read() else {
                    return false;
                };

                let is_top_items_empty = items[0].is_empty();
                let side_item_stack = &items[2];

                if side_item_stack.is_empty() {
                    return !is_top_items_empty;
                }

                if let Some(recipe_output_item) = pumpkin_data::item::Item::from_registry_key(
                    recipe
                        .result
                        .id
                        .strip_prefix("minecraft:")
                        .unwrap_or(&recipe.result.id),
                ) && !is_top_items_empty
                    && recipe_output_item.id == side_item_stack.item.id
                    && side_item_stack.item_count < max_count
                    && side_item_stack.item_count < side_item_stack.get_max_stack_size()
                {
                    return true;
                }
                false
            }
            fn craft_recipe(&self, recipe: Option<&pumpkin_data::recipes::CookingRecipe>) -> bool {
                let can_accept_output =
                    self.can_accept_recipe_output(recipe, self.get_max_count_per_stack());
                if let Some(recipe) = recipe {
                    if can_accept_output {
                        let Ok(mut items) = self.items.try_write() else {
                            return false;
                        };
                        let Some(output_item) = pumpkin_data::item::Item::from_registry_key(
                            recipe
                                .result
                                .id
                                .strip_prefix("minecraft:")
                                .unwrap_or(&recipe.result.id),
                        ) else {
                            return false;
                        };
                        let output_item_stack = ItemStack::new(recipe.result.count, output_item);

                        if items[2].are_equal(ItemStack::EMPTY) {
                            items[2] = output_item_stack;
                        } else if items[2].are_items_and_components_equal(&output_item_stack) {
                            items[2].increment(1);
                        }

                        // Track recipe usage for XP calculation (vanilla RecipesUsed format)
                        self.add_recipe_used(recipe);

                        if items[0].item.id == pumpkin_data::item::Item::WET_SPONGE.id
                            && !items[1].is_empty()
                            && items[1].item.id == pumpkin_data::item::Item::BUCKET.id
                        {
                            items[1] = ItemStack::new(1, &pumpkin_data::item::Item::WATER_BUCKET);
                        }

                        items[0].decrement(1);
                        return true;
                    }
                }

                false
            }
        }
    };
}

#[macro_export]
macro_rules! impl_property_delegate_for_cooking {
    ($struct_name:ty) => {
        impl $crate::block::entities::PropertyDelegate for $struct_name {
            fn get_property(&self, index: i32) -> i32 {
                match index {
                    0 => self.get_lit_time_remaining(),
                    1 => self.get_lit_total_time(),
                    2 => self.get_cooking_time_spent(),
                    3 => self.get_cooking_total_time(),
                    _ => 0,
                }
            }

            fn set_property(&self, index: i32, value: i32) {
                match index {
                    0 => self.set_lit_time_remaining(value),
                    1 => self.set_lit_total_time(value),
                    2 => self.set_cooking_time_spent(value),
                    3 => self.set_cooking_total_time(value),
                    _ => {}
                }
            }

            fn get_properties_size(&self) -> i32 {
                4
            }
        }
    };
}

#[macro_export]
macro_rules! impl_clearable_for_cooking {
    ($struct_name:ty) => {
        impl pumpkin_inventory::Clearable for $struct_name {
            fn clear(&self) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.fill_with(|| ItemStack::EMPTY.clone());
                self.mark_dirty();
            }
        }
    };
}

#[macro_export]
macro_rules! impl_experience_container_for_cooking {
    ($struct_name:ty) => {
        impl $crate::block::entities::furnace_like_block_entity::ExperienceContainer
            for $struct_name
        {
            fn extract_experience(&self) -> i32 {
                // Delegate to the CookingBlockEntityBase method
                self.extract_experience_from_recipes()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_inventory_for_cooking {
    ($struct_name:ty) => {
        impl pumpkin_inventory::Inventory for $struct_name {
            fn size(&self) -> usize {
                Self::INVENTORY_SIZE
            }

            fn is_empty(&self) -> bool {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.iter().all(|s| s.is_empty())
            }

            fn get_stack(&self, slot: usize) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[slot].clone()
            }

            fn remove_stack(&self, slot: usize) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let removed = std::mem::replace(&mut items[slot], ItemStack::EMPTY.clone());
                self.mark_dirty();
                removed
            }

            fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let res = if !items[slot].is_empty() && amount > 0 {
                    items[slot].split(amount)
                } else {
                    ItemStack::EMPTY.clone()
                };
                self.mark_dirty();
                res
            }

            fn set_stack(&self, slot: usize, stack: ItemStack) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let is_same_item = !stack.is_empty()
                    && ItemStack::are_items_and_components_equal(&items[slot], &stack);

                items[slot] = stack.clone();

                if slot == 0 && !is_same_item {
                    if let Some(recipe) = pumpkin_data::recipes::get_cooking_recipe_with_ingredient(
                        stack.item,
                        Self::RECIPE_KIND,
                    ) {
                        self.set_cooking_total_time(
                            $crate::block::entities::furnace_like_block_entity::cooking_time(
                                recipe.cookingtime,
                                self.get_speed_multiplier(),
                            ),
                        );
                    } else {
                        self.set_cooking_total_time(200);
                    }
                    self.set_cooking_time_spent(0);
                }

                // Always consider the inventory changed when setting a stack
                self.mark_dirty();
            }

            fn mark_dirty(&self) {
                self.dirty.store(true, Ordering::Relaxed);
                self.comparator_dirty.store(true, Ordering::Relaxed);
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    };
}

#[macro_export]
macro_rules! impl_block_entity_for_cooking {
    ($struct_name:ty,$recipe_kind:expr) => {
        impl $crate::block::entities::BlockEntity for $struct_name {
            #[expect(clippy::too_many_lines)]
            fn tick(
                &self,
                world: &Arc<$crate::world::World>,
            ) {
                let is_burning = self.is_burning();
                let mut is_dirty = false;
                if self.is_burning() {
                    self.lit_time_remaining.fetch_sub(1, Ordering::Relaxed);
                }

                let (top_item, bottom_item) = if let Ok(items_guard) = self.items.try_read() {
                    (items_guard[0].clone(), items_guard[1].clone())
                } else {
                    return;
                };

                let is_top_items_empty = top_item.is_empty();

                let furnace_recipe = pumpkin_data::recipes::get_cooking_recipe_with_ingredient(
                    top_item.item,
                    $recipe_kind,
                );

                let can_accept_output = self
                    .can_accept_recipe_output(furnace_recipe, self.get_max_count_per_stack());

                let bottom_items_is_empty = bottom_item.is_empty();
                if self.is_burning() || !bottom_items_is_empty && !is_top_items_empty {
                    if !self.is_burning() && can_accept_output {
                        let is_fast = matches!(
                            $recipe_kind,
                            CookingRecipeKind::Blasting | CookingRecipeKind::Smoking
                        );
                        let fuel_component = bottom_item
                            .get_data_component::<pumpkin_data::data_component_impl::CookingFuelImpl>();
                        let adjusted_fuel_ticks = fuel_component
                            .map_or(0, |fuel| {
                                $crate::block::entities::furnace_like_block_entity::cooking_fuel_duration(fuel, is_fast)
                            });
                        let speed_multiplier = fuel_component
                            .map_or(1.0, |fuel| fuel.get_speed_multiplier(is_fast));

                        let mut burn_ticks = adjusted_fuel_ticks;
                        let mut burn_cancelled = false;
                        if let Some(server) = world.server.upgrade() {
                            let mut burn_event = $crate::plugin::api::events::inventory::furnace_burn::FurnaceBurnEvent::new(
                                self.position,
                                bottom_item.item.registry_key.to_string(),
                                adjusted_fuel_ticks as u32,
                            );
                            server.plugin_manager.fire_blocking(&server, &mut burn_event);
                            if burn_event.cancelled {
                                burn_cancelled = true;
                            } else {
                                burn_ticks = burn_event.burn_time as i32;
                            }
                        }
                        if !burn_cancelled {
                            self.set_lit_time_remaining(burn_ticks);
                            self.set_lit_total_time(burn_ticks);
                            self.speed_multiplier.store(speed_multiplier);
                            let old_total = self.get_cooking_total_time();
                            let spent = self.get_cooking_time_spent();
                            if old_total > 0 && spent < old_total {
                                if let Some(recipe) = furnace_recipe {
                                    let new_total = $crate::block::entities::furnace_like_block_entity::cooking_time(
                                        recipe.cookingtime,
                                        speed_multiplier,
                                    );
                                    self.set_cooking_total_time(new_total);
                                    self.set_cooking_time_spent(
                                        $crate::block::entities::furnace_like_block_entity::rescale_cooking_progress(
                                            spent, old_total, new_total,
                                        ),
                                    );
                                }
                            }

                            if self.is_burning() {
                                is_dirty = true;
                                if let Ok(mut items_guard) = self.items.try_write() {
                                    if !items_guard[1].is_empty() {
                                        // Vanilla consumes the item's crafting remainder,
                                        // not its separate consume/use remainder component.
                                        let remainder = (items_guard[1].item.id
                                            == pumpkin_data::item::Item::LAVA_BUCKET.id)
                                            .then_some(&pumpkin_data::item::Item::BUCKET);
                                        items_guard[1].decrement(1);
                                        if let Some(remainder_item) = remainder
                                        {
                                            let remainder_stack = ItemStack::new(1, remainder_item);
                                            if items_guard[1].is_empty() {
                                                items_guard[1] = remainder_stack;
                                            } else {
                                                let pos = self.position.to_centered_f64();
                                                let entity = $crate::entity::Entity::new(
                                                    world.clone(),
                                                    pos,
                                                    &pumpkin_data::entity::EntityType::ITEM,
                                                );
                                                world.spawn_entity(std::sync::Arc::new(
                                                    $crate::entity::item::ItemEntity::new(
                                                        entity,
                                                        remainder_stack,
                                                    ),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if self.is_burning() && can_accept_output {
                        let mut start_cancelled = false;
                        if self.get_cooking_time_spent() == 0 {
                            if let Some(server) = world.server.upgrade() {
                                let mut start_event = $crate::plugin::api::events::inventory::furnace_start_smelt::FurnaceStartSmeltEvent::new(
                                    self.position,
                                    top_item.item.registry_key.to_string(),
                                    self.get_cooking_total_time() as u32,
                                );
                                server.plugin_manager.fire_blocking(&server, &mut start_event);
                                if start_event.cancelled {
                                    start_cancelled = true;
                                } else {
                                    self.set_cooking_total_time(start_event.cooking_time as i32);
                                }
                            }
                        }
                        if !start_cancelled {
                            self.cooking_time_spent.fetch_add(1, Ordering::Relaxed);

                            if self.get_cooking_time_spent() >= self.get_cooking_total_time() {
                                self.set_cooking_time_spent(0);
                                if let Some(cooking_recipe) = furnace_recipe {
                                    self.set_cooking_total_time(
                                        $crate::block::entities::furnace_like_block_entity::cooking_time(
                                            cooking_recipe.cookingtime,
                                            self.get_speed_multiplier(),
                                        ),
                                    );

                                    let mut smelt_cancelled = false;
                                    if let Some(server) = world.server.upgrade() {
                                        let mut smelt_event = $crate::plugin::api::events::inventory::furnace_smelt::FurnaceSmeltEvent::new(
                                            self.position,
                                            top_item.item.registry_key.to_string(),
                                            cooking_recipe.result.id.to_string(),
                                        );
                                        server.plugin_manager.fire_blocking(&server, &mut smelt_event);
                                        if smelt_event.cancelled {
                                            smelt_cancelled = true;
                                        }
                                    }
                                    if !smelt_cancelled {
                                        self.craft_recipe(Some(cooking_recipe));
                                        is_dirty = true;
                                    }
                                }
                            }
                        }
                    } else {
                        self.set_cooking_time_spent(0);
                    }
                } else if !self.is_burning() && self.get_cooking_time_spent() > 0 {
                    let _ = self.cooking_time_spent.try_update(
                        Ordering::Acquire,
                        Ordering::Acquire,
                        |v| {
                            Some(
                                v.wrapping_sub(2)
                                    .max(0)
                                    .min(self.cooking_total_time.load(Ordering::Acquire)),
                            )
                        },
                    );
                }

                if is_burning != self.is_burning() {
                    is_dirty = true;
                    let (furnace_block, furnace_block_state) =
                        world.get_block_and_state(&self.position);
                    let mut props =
                        pumpkin_data::block_properties::FurnaceLikeProperties::from_state_id(furnace_block_state.id);

                    props.lit = self.is_burning();
                    world.set_block_state(
                        &self.position,
                        props.to_state_id(furnace_block),
                        $crate::world::BlockFlags::NOTIFY_ALL,
                    );
                }

                if is_dirty {
                    self.mark_dirty();
                }
            }

            fn resource_location(&self) -> &'static str {
                Self::ID
            }

            fn get_position(&self) -> BlockPos {
                self.position
            }

            fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
            where
                Self: Sized,
            {
                let cooking_total_time = AtomicI32::new(
                    $crate::block::entities::furnace_like_block_entity::read_cooking_int(nbt, "cooking_total_time"),
                );
                let cooking_time_spent = AtomicI32::new(
                    $crate::block::entities::furnace_like_block_entity::read_cooking_int(nbt, "cooking_time_spent"),
                );
                let lit_total_time = AtomicI32::new(
                    $crate::block::entities::furnace_like_block_entity::read_cooking_int(nbt, "lit_total_time"),
                );
                let lit_time_remaining = AtomicI32::new(
                    $crate::block::entities::furnace_like_block_entity::read_cooking_int(nbt, "lit_time_remaining"),
                );
                // Load RecipesUsed from NBT (vanilla format: map of recipe ID -> craft count)
                let mut recipes_used_map = HashMap::new();
                if let Some(recipes_compound) = nbt.get_compound("RecipesUsed") {
                    for (recipe_id, tag) in &recipes_compound.child_tags {
                        if let pumpkin_nbt::tag::NbtTag::Int(count) = tag {
                            recipes_used_map.insert(recipe_id.to_string(), *count as u32);
                        }
                    }
                }

                let mut furnace = Self {
                    position,
                    dirty: AtomicBool::new(false),
                    comparator_dirty: AtomicBool::new(false),
                    items: std::sync::RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
                    cooking_total_time,
                    cooking_time_spent,
                    lit_total_time,
                    lit_time_remaining,
                    speed_multiplier: crossbeam::atomic::AtomicCell::new(
                        $crate::block::entities::furnace_like_block_entity::read_cooking_speed(nbt),
                    ),
                    recipes_used: std::sync::Mutex::new(recipes_used_map),
                };
                pumpkin_inventory::sync_read_items_from_nbt(nbt, furnace.items.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner));

                furnace
            }

            fn write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                $crate::block::entities::furnace_like_block_entity::write_cooking_state(self, nbt);

                // Save RecipesUsed in vanilla format (map of recipe ID -> craft count)
                {
                    let recipes = self
                        .recipes_used
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !recipes.is_empty() {
                        let mut recipes_compound = pumpkin_nbt::compound::NbtCompound::new();
                        for (recipe_id, count) in recipes.iter() {
                            recipes_compound.put(
                                recipe_id.as_str(),
                                pumpkin_nbt::tag::NbtTag::Int(*count as i32),
                            );
                        }
                        nbt.put(
                            "RecipesUsed",
                            pumpkin_nbt::tag::NbtTag::Compound(recipes_compound),
                        );
                    }
                }

                self.write_inventory_nbt(nbt, true);
            }

            fn get_inventory(
                self: Arc<Self>,
            ) -> Option<Arc<dyn pumpkin_inventory::Inventory>> {
                Some(self)
            }

            fn is_comparator_dirty(&self) -> bool {
                self.comparator_dirty.load(Ordering::Relaxed)
            }

            fn clear_comparator_dirty(&self) {
                self.comparator_dirty.store(false, Ordering::Relaxed);
            }


            fn is_dirty(&self) -> bool {
                self.dirty.load(Ordering::Relaxed)
            }

            fn chunk_data_nbt(&self) -> Option<pumpkin_nbt::compound::NbtCompound> {
                let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
                $crate::block::entities::furnace_like_block_entity::write_cooking_state(self, &mut nbt);

                if let Ok(recipes) = self.recipes_used.lock() {
                    if !recipes.is_empty() {
                        let mut recipes_compound = pumpkin_nbt::compound::NbtCompound::new();
                        for (recipe_id, count) in recipes.iter() {
                            recipes_compound.put(
                                recipe_id.as_str(),
                                pumpkin_nbt::tag::NbtTag::Int(*count as i32),
                            );
                        }
                        nbt.put(
                            "RecipesUsed",
                            pumpkin_nbt::tag::NbtTag::Compound(recipes_compound),
                        );
                    }
                }

                if let Ok(guard) = self.items.try_read() {
                    pumpkin_inventory::sync_write_items_to_nbt(&*guard, &mut nbt);
                }
                Some(nbt)
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn to_property_delegate(
                self: Arc<Self>,
            ) -> Option<Arc<dyn $crate::block::entities::PropertyDelegate>> {
                Some(self as Arc<dyn $crate::block::entities::PropertyDelegate>)
            }

            fn to_experience_container(
                self: Arc<Self>,
            ) -> Option<
                Arc<dyn $crate::block::entities::furnace_like_block_entity::ExperienceContainer>,
            > {
                Some(
                    self as Arc<
                        dyn $crate::block::entities::furnace_like_block_entity::ExperienceContainer,
                    >,
                )
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::entities::{
            blasting_furnace::BlastingFurnaceBlockEntity, block_entity_from_nbt,
            furnace::FurnaceBlockEntity, smoker::SmokerBlockEntity,
        },
        world::World,
    };
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::{
        Block, block_properties::FurnaceLikeProperties, data_component_impl::FloatProvider,
        dimension::Dimension, item::Item,
    };
    use pumpkin_util::{
        math::{position::BlockPos, vector2::Vector2},
        world_seed::Seed,
    };
    use pumpkin_world::{chunk::ChunkData, level::Level, world_info::LevelData};
    use std::sync::{Arc, Weak};

    const POSITION: BlockPos = BlockPos::new(4097, 200, 1);

    async fn world() -> (tempfile::TempDir, Arc<World>) {
        let folder = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            folder.path().into(),
            0,
            Dimension::OVERWORLD,
        );
        level.shutdown().await.unwrap();
        level
            .loaded_chunks
            .insert(Vector2::new(256, 0), ChunkData::empty_sync(256, 0));
        let world = Arc::new(World::load(
            level,
            Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
            Dimension::OVERWORLD,
            crate::block::registry::default_registry(),
            Weak::new(),
            Arc::default(),
        ));
        (folder, world)
    }

    fn place(world: &World, position: BlockPos, block: &'static Block, lit: bool) {
        let mut properties = FurnaceLikeProperties::default(block);
        properties.lit = lit;
        world
            .level
            .loaded_chunks
            .get(&position.chunk_position())
            .unwrap()
            .set_block_absolute_y(
                (position.0.x & 15) as usize,
                position.0.y,
                (position.0.z & 15) as usize,
                properties.to_state_id(block),
            );
    }

    fn saved_item(slot: i8, id: &str, count: i32) -> NbtTag {
        let mut item = NbtCompound::new();
        item.put_byte("Slot", slot);
        item.put_string("id", id.to_string());
        item.put_int("count", count);
        NbtTag::Compound(item)
    }

    /// Observed after exactly 237 ticks in official Java 26.3. Source world
    /// manifest SHA-256: de91961415ddb941e59f4741b005b1d768a5beabb1461e2b8ccbf3952ecf3603.
    fn java_reference(kind: &str) -> NbtCompound {
        let (x, input, output, recipe, inputs, outputs, total, remaining, lit_total, speed) =
            match kind {
                "furnace" => (
                    4097,
                    "raw_iron",
                    "iron_ingot",
                    "iron_ingot_from_smelting_raw_iron",
                    15,
                    1,
                    200,
                    1364,
                    1600,
                    1.0,
                ),
                "blast_furnace" => (
                    4099,
                    "raw_iron",
                    "iron_ingot",
                    "iron_ingot_from_blasting_raw_iron",
                    14,
                    2,
                    100,
                    564,
                    800,
                    2.0,
                ),
                "smoker" => (
                    4101,
                    "porkchop",
                    "cooked_porkchop",
                    "cooked_porkchop_from_smoking",
                    14,
                    2,
                    100,
                    564,
                    800,
                    2.0,
                ),
                _ => unreachable!(),
            };
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", format!("minecraft:{kind}"));
        nbt.put_int("x", x);
        nbt.put_int("y", 200);
        nbt.put_int("z", 1);
        nbt.put_int("cooking_time_spent", 37);
        nbt.put_int("cooking_total_time", total);
        nbt.put_int("lit_time_remaining", remaining);
        nbt.put_int("lit_total_time", lit_total);
        nbt.put_float("speed_multiplier", speed);
        nbt.put_list(
            "Items",
            vec![
                saved_item(0, &format!("minecraft:{input}"), inputs),
                saved_item(2, &format!("minecraft:{output}"), outputs),
            ],
        );
        let mut recipes = NbtCompound::new();
        recipes.put_int(&format!("minecraft:{recipe}"), outputs);
        nbt.put_compound("RecipesUsed", recipes);
        nbt
    }

    #[tokio::test]
    async fn cooking_continues_the_actual_java_237_tick_furnace_blast_and_smoker_states() {
        let (_folder, world) = world().await;
        let entities: Vec<_> = ["furnace", "blast_furnace", "smoker"]
            .into_iter()
            .map(|kind| {
                let nbt = java_reference(kind);
                let entity = block_entity_from_nbt(&nbt).unwrap();
                let block = match kind {
                    "furnace" => &Block::FURNACE,
                    "blast_furnace" => &Block::BLAST_FURNACE,
                    _ => &Block::SMOKER,
                };
                place(&world, entity.get_position(), block, true);
                let recipe = nbt
                    .get_compound("RecipesUsed")
                    .unwrap()
                    .child_tags
                    .keys()
                    .next()
                    .unwrap()
                    .to_string();
                (entity, recipe)
            })
            .collect();
        for (ticks, expected) in [
            (
                0,
                [
                    (37, 200, 1364, 1600, 1.0, 15, 1),
                    (37, 100, 564, 800, 2.0, 14, 2),
                    (37, 100, 564, 800, 2.0, 14, 2),
                ],
            ),
            (
                63,
                [
                    (100, 200, 1301, 1600, 1.0, 15, 1),
                    (0, 100, 501, 800, 2.0, 13, 3),
                    (0, 100, 501, 800, 2.0, 13, 3),
                ],
            ),
            (
                100,
                [
                    (0, 200, 1201, 1600, 1.0, 14, 2),
                    (0, 100, 401, 800, 2.0, 12, 4),
                    (0, 100, 401, 800, 2.0, 12, 4),
                ],
            ),
        ] {
            for _ in 0..ticks {
                for (entity, _) in &entities {
                    entity.tick(&world);
                }
            }
            for ((entity, recipe), (spent, total, remaining, lit_total, speed, input, output)) in
                entities.iter().zip(expected)
            {
                let mut saved = NbtCompound::new();
                entity.write_internal(&mut saved);
                for (key, value) in [
                    ("cooking_time_spent", spent),
                    ("cooking_total_time", total),
                    ("lit_time_remaining", remaining),
                    ("lit_total_time", lit_total),
                ] {
                    assert_eq!(
                        saved.get_int(key),
                        Some(value),
                        "{} {key}",
                        entity.resource_location()
                    );
                }
                assert_eq!(saved.get_float("speed_multiplier"), Some(speed));
                assert_eq!(
                    saved.get_compound("RecipesUsed").unwrap().get_int(recipe),
                    Some(output)
                );
                let inventory = entity.clone().get_inventory().unwrap();
                assert_eq!(i32::from(inventory.get_stack(0).item_count), input);
                assert!(inventory.get_stack(1).is_empty());
                assert_eq!(i32::from(inventory.get_stack(2).item_count), output);
            }
        }
    }

    #[test]
    fn cooking_numeric_nbt_matches_java_floor_truncation_and_saturation() {
        for (tag, expected) in [
            (NbtTag::Byte(-7), -7),
            (NbtTag::Short(37), 37),
            (NbtTag::Int(70000), 70000),
            (NbtTag::Int(i32::MIN), i32::MIN),
            (NbtTag::Long(0x1_0000_0001), 1),
            (NbtTag::Long(0xffff_ffff), -1),
            (NbtTag::Float(-1.5), -2),
            (NbtTag::Double(-1.5), -2),
            (NbtTag::Float(f32::NAN), 0),
            (NbtTag::Double(f64::NAN), 0),
            (NbtTag::Float(f32::INFINITY), i32::MAX),
            (NbtTag::Double(f64::NEG_INFINITY), i32::MIN),
        ] {
            let mut nbt = NbtCompound::new();
            nbt.put("cooking_total_time", tag);
            let furnace = FurnaceBlockEntity::from_nbt(&nbt, POSITION);
            assert_eq!(furnace.get_cooking_total_time(), expected);
            assert_eq!(furnace.get_speed_multiplier(), 1.0);
            let mut saved = NbtCompound::new();
            furnace.write_nbt(&mut saved);
            assert_eq!(saved.get_int("cooking_total_time"), Some(expected));
        }
        let mut nbt = NbtCompound::new();
        nbt.put("speed_multiplier", NbtTag::Double(0.5));
        let furnace = FurnaceBlockEntity::from_nbt(&nbt, POSITION);
        assert_eq!(furnace.get_speed_multiplier(), 0.5);
        assert_eq!(furnace.get_cooking_time_spent(), 0);
        assert_eq!(furnace.get_lit_time_remaining(), 0);
        furnace.set_lit_time_remaining(-1);
        furnace.set_lit_total_time(i32::MAX);
        let mut saved = NbtCompound::new();
        furnace.write_nbt(&mut saved);
        assert_eq!(saved.get_int("lit_time_remaining"), Some(-1));
        assert_eq!(saved.get_int("lit_total_time"), Some(i32::MAX));
        assert_eq!(saved.get_float("speed_multiplier"), Some(0.5));
    }

    #[test]
    fn cooking_speed_special_values_and_literal_fuel_follow_java_resolution() {
        for (speed, expected) in [
            (2.0, 100),
            (3.0, 67),
            (0.0, 200),
            (-2.0, 200),
            (f32::NAN, 200),
            (f32::INFINITY, 0),
            (f32::MIN_POSITIVE, i32::MAX),
        ] {
            assert_eq!(cooking_time(200, speed), expected);
        }
        assert_eq!(rescale_cooking_progress(37, 200, 100), 19);
        for duration in [-7, 1600, 100000, i32::MAX] {
            let fuel = CookingFuelImpl {
                burn_time: IntProvider::Inline(duration),
                speed_multiplier: FloatProvider::Inline(2.0),
            };
            assert_eq!(cooking_fuel_duration(&fuel, false), duration);
            assert_eq!(cooking_fuel_duration(&fuel, true), duration);
        }
        let coal = ItemStack::new(1, &Item::COAL);
        let fuel = coal.get_data_component::<CookingFuelImpl>().unwrap();
        assert_eq!(cooking_fuel_duration(fuel, false), 1600);
        assert_eq!(cooking_fuel_duration(fuel, true), 800);
        assert_eq!(fuel.get_speed_multiplier(false), 1.0);
        assert_eq!(fuel.get_speed_multiplier(true), 2.0);
    }

    #[tokio::test]
    async fn cooking_new_fuel_rescales_partial_progress_without_halving_literal_duration() {
        let (_folder, world) = world().await;
        place(&world, POSITION, &Block::BLAST_FURNACE, false);
        let furnace = BlastingFurnaceBlockEntity::new(POSITION);
        furnace.set_stack(0, ItemStack::new(16, &Item::RAW_IRON));
        furnace.set_cooking_time_spent(37);
        let mut coal = ItemStack::new(1, &Item::COAL);
        coal.set_data_component(CookingFuelImpl {
            burn_time: IntProvider::Inline(1600),
            speed_multiplier: FloatProvider::Inline(2.0),
        });
        furnace.set_stack(1, coal);
        furnace.tick(&world);
        assert_eq!(furnace.get_lit_time_remaining(), 1600);
        assert_eq!(furnace.get_lit_total_time(), 1600);
        assert_eq!(furnace.get_speed_multiplier(), 2.0);
        assert_eq!(furnace.get_cooking_total_time(), 100);
        assert_eq!(furnace.get_cooking_time_spent(), 20);
        assert!(furnace.get_stack(1).is_empty());
    }

    #[test]
    fn cooking_input_uses_its_own_recipe_kind_and_retained_speed() {
        let smoker = SmokerBlockEntity::new(POSITION);
        smoker.speed_multiplier.store(2.0);
        smoker.set_stack(0, ItemStack::new(1, &Item::PORKCHOP));
        assert_eq!(smoker.get_cooking_total_time(), 100);
        smoker.set_stack(0, ItemStack::new(1, &Item::RAW_IRON));
        assert_eq!(
            smoker.get_cooking_total_time(),
            200,
            "No smoking recipe uses Java's 200-tick fallback"
        );
        let blast = BlastingFurnaceBlockEntity::new(POSITION);
        blast.speed_multiplier.store(2.0);
        blast.set_stack(0, ItemStack::new(1, &Item::PORKCHOP));
        assert_eq!(blast.get_cooking_total_time(), 200);
        blast.set_stack(0, ItemStack::new(1, &Item::RAW_IRON));
        assert_eq!(blast.get_cooking_total_time(), 100);
    }

    #[tokio::test]
    async fn cooking_lava_retains_its_bucket_then_fills_it_when_drying_sponge() {
        let (_folder, world) = world().await;
        place(&world, POSITION, &Block::FURNACE, false);
        let furnace = FurnaceBlockEntity::new(POSITION);
        furnace.set_stack(0, ItemStack::new(16, &Item::WET_SPONGE));
        furnace.set_stack(1, ItemStack::new(1, &Item::LAVA_BUCKET));
        furnace.tick(&world);
        assert_eq!(furnace.get_stack(1).item.id, Item::BUCKET.id);
        assert_eq!(furnace.get_lit_time_remaining(), 20000);
        assert_eq!(furnace.get_cooking_time_spent(), 1);
        for _ in 0..199 {
            furnace.tick(&world);
        }
        assert_eq!(furnace.get_stack(1).item.id, Item::WATER_BUCKET.id);
        assert_eq!(furnace.get_stack(0).item_count, 15);
        assert_eq!(furnace.get_stack(2).item.id, Item::SPONGE.id);
        assert_eq!(furnace.get_stack(2).item_count, 1);
        assert_eq!(furnace.get_lit_time_remaining(), 19801);
        assert_eq!(furnace.get_cooking_time_spent(), 0);
    }

    #[tokio::test]
    async fn cooking_signed_timer_boundaries_complete_or_wrap_without_panicking() {
        let (_folder, world) = world().await;
        place(&world, POSITION, &Block::FURNACE, true);
        for (spent, total, outputs, expected_spent) in [
            (10, 5, 1, 0),
            (0, 0, 1, 0),
            (0, -2, 1, 0),
            (i32::MAX, i32::MAX, 0, i32::MIN),
        ] {
            let furnace = FurnaceBlockEntity::new(POSITION);
            furnace.set_stack(0, ItemStack::new(1, &Item::RAW_IRON));
            furnace.set_cooking_time_spent(spent);
            furnace.set_cooking_total_time(total);
            furnace.set_lit_time_remaining(2);
            furnace.tick(&world);
            assert_eq!(furnace.get_stack(2).item_count, outputs);
            assert_eq!(furnace.get_cooking_time_spent(), expected_spent);
        }
        let furnace = FurnaceBlockEntity::new(POSITION);
        furnace.set_cooking_time_spent(1);
        furnace.set_cooking_total_time(-2);
        furnace.tick(&world);
        assert_eq!(furnace.get_cooking_time_spent(), -2);
    }
}
