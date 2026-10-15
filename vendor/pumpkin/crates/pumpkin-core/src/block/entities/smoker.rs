use crossbeam::atomic::AtomicCell;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::Inventory;

use std::{
    array::from_fn,
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex, RwLock,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

use pumpkin_data::recipes::CookingRecipeKind;
use pumpkin_util::math::position::BlockPos;

use crate::{
    block::entities::furnace_like_block_entity::CookingBlockEntityBase,
    impl_block_entity_for_cooking, impl_clearable_for_cooking, impl_cooking_block_entity_base,
    impl_experience_container_for_cooking, impl_inventory_for_cooking,
    impl_property_delegate_for_cooking,
};

pub struct SmokerBlockEntity {
    pub position: BlockPos,
    pub dirty: AtomicBool,
    pub comparator_dirty: AtomicBool,

    pub cooking_time_spent: AtomicI32,
    pub cooking_total_time: AtomicI32,
    pub lit_time_remaining: AtomicI32,
    pub lit_total_time: AtomicI32,
    pub speed_multiplier: AtomicCell<f32>,

    pub items: RwLock<[ItemStack; Self::INVENTORY_SIZE]>,

    /// Tracks recipes used for XP calculation (vanilla `RecipesUsed` NBT format)
    /// Maps recipe ID -> craft count
    pub recipes_used: StdMutex<HashMap<String, u32>>,
}

impl SmokerBlockEntity {
    pub const INVENTORY_SIZE: usize = 3;
    pub(crate) const RECIPE_KIND: CookingRecipeKind = CookingRecipeKind::Smoking;
    pub const ID: &'static str = "minecraft:smoker";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
            items: RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
            cooking_total_time: AtomicI32::new(0),
            cooking_time_spent: AtomicI32::new(0),
            lit_total_time: AtomicI32::new(0),
            lit_time_remaining: AtomicI32::new(0),
            speed_multiplier: AtomicCell::new(1.0),
            recipes_used: StdMutex::new(HashMap::new()),
        }
    }
}

impl_cooking_block_entity_base!(SmokerBlockEntity);
impl_block_entity_for_cooking!(SmokerBlockEntity, CookingRecipeKind::Smoking);
impl_inventory_for_cooking!(SmokerBlockEntity);
impl_clearable_for_cooking!(SmokerBlockEntity);
impl_property_delegate_for_cooking!(SmokerBlockEntity);
impl_experience_container_for_cooking!(SmokerBlockEntity);
