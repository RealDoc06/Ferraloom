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
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct AbstractFurnaceBlockEntity {
    pub parent: BaseContainerBlockEntity,
    pub lit_time_remaining: i32,
    pub lit_total_time: i32,
    pub cooking_timer: i32,
    pub cooking_total_time: i32,
    pub speed_multiplier: f32,
    pub recipes_used: BTreeMap<Identifier, i32>,
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
            recipes_used: BTreeMap::new(),
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
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Furnace(entity) = state else {
            unreachable!()
        };
        entity.parent.load_base(tag);
        entity.parent.items.fill(Default::default());
        storage::load_items(tag, &mut entity.parent.items);
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
        for (key, value) in &entity.recipes_used {
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
        Err(LevelError::UnsupportedBlockEntity(
            level.get_block_state(entity.pos)?.block(),
        ))
    }
}
