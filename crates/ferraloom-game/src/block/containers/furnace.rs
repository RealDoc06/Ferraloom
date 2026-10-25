//! Port of AbstractFurnaceBlockEntity's persisted cooking state and inventory.
use super::storage::{self, BASE_COMPONENTS, BaseContainerBlockEntity};
use crate::{
    block::entity::{
        BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState, BlockEntityType,
    },
    crafting::RecipeType,
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{ComponentTypeId, DataComponentMap},
};
use ferraloom_util::random::RandomSource;

/// Recipe keys are interned identities in vanilla's Reference2IntOpenHashMap;
/// deterministic insertion order is the accepted identity-hash variation.
#[derive(Clone, Debug, Default)]
pub struct RecipesUsed(Vec<(Identifier, i32)>);
impl RecipesUsed {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn get(&self, key: &Identifier) -> Option<&i32> {
        self.0.iter().find(|(id, _)| id == key).map(|(_, n)| n)
    }
    pub fn insert(&mut self, key: Identifier, count: i32) -> Option<i32> {
        if let Some((_, current)) = self.0.iter_mut().find(|(id, _)| id == &key) {
            return Some(std::mem::replace(current, count));
        }
        self.0.push((key, count));
        None
    }
    /// Port of Reference2IntMap#addTo; changing a count keeps its key's place.
    pub fn record(&mut self, key: Identifier) {
        if let Some((_, current)) = self.0.iter_mut().find(|(id, _)| id == &key) {
            *current = current.wrapping_add(1);
        } else {
            self.0.push((key, 1));
        }
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&Identifier, &i32)> + ExactSizeIterator {
        self.0.iter().map(|(id, count)| (id, count))
    }
    pub fn values(&self) -> impl Iterator<Item = &i32> {
        self.0.iter().map(|(_, count)| count)
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}
impl FromIterator<(Identifier, i32)> for RecipesUsed {
    fn from_iter<T: IntoIterator<Item = (Identifier, i32)>>(iter: T) -> Self {
        let mut result = Self::default();
        for (id, count) in iter {
            result.insert(id, count);
        }
        result
    }
}

/// Port of AbstractFurnaceBlockEntity#getRecipesToAwardAndPopExperience.
/// Each entry's XP is spawned before looking up the next entry; the returned
/// holder list is vanilla's explicit capture, not a copy of container contents.
pub fn get_recipes_to_award_and_pop_experience<'a>(
    handle: BlockEntityRef,
    position: ferraloom_util::Vec3,
    recipes: &'a crate::crafting::RecipeManager,
    level: &mut Level,
) -> Result<Vec<&'a crate::crafting::RecipeHolder>, LevelError> {
    let mut result = Vec::new();
    let mut ordinal = 0;
    loop {
        let entry = level
            .block_entity::<AbstractFurnaceBlockEntity>(handle)?
            .recipes_used
            .iter()
            .nth(ordinal)
            .map(|(id, count)| (id.clone(), *count));
        let Some((id, count)) = entry else {
            break;
        };
        ordinal += 1;
        let key = id
            .as_str()
            .parse()
            .map_err(|_| LevelError::Inventory("furnace recipe identifier"))?;
        if let Some(recipe) = recipes.by_key(&key) {
            result.push(recipe);
            let experience = recipe
                .value
                .experience()
                .ok_or(LevelError::Inventory("furnace cooking recipe"))?;
            let product = count as f32 * experience;
            let mut reward = ferraloom_util::mth::floor(f64::from(product));
            let fraction = product - reward as f32;
            if fraction != 0.0 && level.random().next_float() < fraction {
                reward = reward.wrapping_add(1);
            }
            crate::entity::experience_orb::award(
                level,
                position,
                ferraloom_util::Vec3::ZERO,
                reward,
            )?;
        }
    }
    Ok(result)
}

/// Port of AbstractFurnaceBlockEntity#awardUsedRecipesAndPopExperience. All XP
/// precedes unlock callbacks and their packet, then each recipe-crafted trigger
/// reads the furnace's current item receivers before recipesUsed is cleared.
pub fn award_used_recipes_and_pop_experience(
    handle: BlockEntityRef,
    player: crate::entity::EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let resources = level.container_resources()?;
    let position = level.entity::<crate::entity::EntityData>(player)?.position;
    let recipes =
        get_recipes_to_award_and_pop_experience(handle, position, &resources.recipes, level)?;
    crate::advancements::award_recipes(level, player, recipes.iter().copied())?;
    for recipe in recipes {
        let mut operation = crate::inventory::StackOperation::new(level);
        let mut items = Vec::with_capacity(3);
        for slot in 0..3 {
            items.push(
                operation
                    .pin(crate::inventory::ContainerSlot {
                        container: crate::inventory::ContainerHandle::BlockEntity(handle),
                        slot,
                    })
                    .map_err(furnace_menu_error)?,
            );
        }
        crate::advancements::criteria::recipe_crafted(operation.level, player, recipe, &items)?;
    }
    level
        .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?
        .recipes_used
        .clear();
    Ok(())
}

fn furnace_menu_error(error: crate::inventory::MenuError) -> LevelError {
    match error {
        crate::inventory::MenuError::Level(error) => error,
        _ => LevelError::Inventory("furnace item receiver"),
    }
}

/// Port of AbstractFurnaceBlockEntity#setItem, including input-change reset.
pub fn set_item(
    handle: BlockEntityRef,
    slot: usize,
    stack: ferraloom_protocol::component::ItemStack,
    level: &mut Level,
) -> Result<(), LevelError> {
    super::access::set_item(handle, slot, stack, level)
}
pub(crate) fn set_item_value(
    handle: BlockEntityRef,
    slot: usize,
    value: crate::inventory::StackValue,
    level: &mut Level,
) -> Result<(), LevelError> {
    use crate::{entity::player::inventory::is_same_item_same_components, inventory::Container};
    let reset = {
        let furnace = level.block_entity::<AbstractFurnaceBlockEntity>(handle)?;
        let stack = value
            .as_stack(level)
            .map_err(|_| LevelError::Inventory("furnace stack receiver"))?;
        let same =
            !stack.is_empty() && is_same_item_same_components(&furnace.parent.items[slot], stack);
        slot == 0 && !same && furnace.parent.parent.has_level
    };
    super::access::replace_and_limit(handle, slot, value, level)?;
    if reset {
        let resources = level.container_resources()?;
        let furnace = level.block_entity::<AbstractFurnaceBlockEntity>(handle)?;
        let recipe = resources
            .recipes
            .get_recipe_for(
                furnace.recipe_type(),
                crate::crafting::RecipeInput::Single(&furnace.parent.items[0]),
                &(),
                furnace.last_recipe.as_ref(),
            )
            .map_err(|_| LevelError::Inventory("furnace recipe lookup"))?;
        let total = recipe.map_or(200, |recipe| {
            furnace.total_cook_time(recipe.value.cooking_time().expect("cooking recipe"))
        });
        let hint = recipe.map(|recipe| recipe.id.clone());
        let furnace = level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
        if hint.is_some() {
            furnace.last_recipe = hint;
        }
        furnace.cooking_total_time = total;
        furnace.cooking_timer = 0;
        furnace.parent.set_changed();
        handle.set_changed(level)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AbstractFurnaceBlockEntity {
    pub parent: BaseContainerBlockEntity,
    pub lit_time_remaining: i32,
    pub lit_total_time: i32,
    pub cooking_timer: i32,
    pub cooking_total_time: i32,
    pub speed_multiplier: f32,
    pub recipes_used: RecipesUsed,
    pub last_recipe: Option<ferraloom_data::datapack::Identifier>,
}
impl AbstractFurnaceBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: BaseContainerBlockEntity::new(parent, 3),
            lit_time_remaining: 0,
            lit_total_time: 0,
            cooking_timer: 0,
            cooking_total_time: 0,
            speed_multiplier: 1.0,
            recipes_used: RecipesUsed::default(),
            last_recipe: None,
        }
    }
    pub fn recipe_type(&self) -> RecipeType {
        match self.parent.parent.kind {
            BlockEntityType::SMOKER => RecipeType::Smoking,
            BlockEntityType::BLAST_FURNACE => RecipeType::Blasting,
            _ => RecipeType::Smelting,
        }
    }
    pub fn total_cook_time(&self, recipe_time: i32) -> i32 {
        if self.speed_multiplier > 0.0 {
            (recipe_time as f32 / self.speed_multiplier).ceil() as i32
        } else {
            recipe_time
        }
    }
    pub fn data(&self) -> [i32; 4] {
        [
            self.lit_time_remaining,
            self.lit_total_time,
            self.cooking_timer,
            self.cooking_total_time,
        ]
    }
}
pub struct FurnaceBehavior;
pub static FURNACE_ENTITY: FurnaceBehavior = FurnaceBehavior;
impl BlockEntity for FurnaceBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        super::furnace_tick::server_tick(handle, level)
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["CustomName", "lock", "Items"] {
            tag.remove(key);
        }
    }
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Furnace(entity) = state else {
            unreachable!()
        };
        entity.parent.load_base(tag);
        entity.parent.load_items(tag);
        entity.cooking_timer = tag.get_int_or("cooking_time_spent", 0);
        entity.cooking_total_time = tag.get_int_or("cooking_total_time", 0);
        entity.lit_time_remaining = tag.get_int_or("lit_time_remaining", 0);
        entity.lit_total_time = tag.get_int_or("lit_total_time", 0);
        entity.speed_multiplier = tag.get_float_or("speed_multiplier", 1.0);
        entity.recipes_used = tag
            .get_compound("RecipesUsed")
            .into_iter()
            .flat_map(CompoundTag::iter)
            .filter_map(|(key, value)| {
                Some((
                    Identifier::parse(&key.to_string_lossy()).ok()?,
                    value.as_int()?,
                ))
            })
            .collect();
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Furnace(entity) = state else {
            unreachable!()
        };
        let mut tag = entity.parent.save_base()?;
        storage::save_items(&mut tag, &entity.parent.items, true)?;
        for (key, value) in [
            ("cooking_time_spent", entity.cooking_timer),
            ("cooking_total_time", entity.cooking_total_time),
            ("lit_time_remaining", entity.lit_time_remaining),
            ("lit_total_time", entity.lit_total_time),
        ] {
            tag.insert(key, Tag::Int(value));
        }
        tag.insert("speed_multiplier", Tag::Float(entity.speed_multiplier));
        let mut used = CompoundTag::new();
        for (key, value) in entity.recipes_used.iter() {
            used.insert(key.as_str(), Tag::Int(*value));
        }
        tag.insert("RecipesUsed", Tag::Compound(used));
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Furnace(entity) = state else {
            unreachable!()
        };
        entity.parent.apply_components(map);
        BASE_COMPONENTS
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Furnace(entity) = state else {
            unreachable!()
        };
        entity.parent.collect_components(map);
    }
    fn pre_remove_side_effects(
        &self,
        entity: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::drops::drop_contents(entity, level)?;
        if !level
            .block_entity::<AbstractFurnaceBlockEntity>(entity)?
            .recipes_used
            .is_empty()
        {
            let resources = level.container_resources()?;
            let position = ferraloom_util::Vec3::new(
                f64::from(entity.pos.x) + 0.5,
                f64::from(entity.pos.y) + 0.5,
                f64::from(entity.pos.z) + 0.5,
            );
            get_recipes_to_award_and_pop_experience(entity, position, &resources.recipes, level)?;
        }
        Ok(())
    }
}
