//! Immutable datapack services and region-owned random sequences for containers.
use crate::{
    crafting::RecipeManager,
    loot::{LootData, RandomSequences},
};
use std::sync::Arc;

/// Immutable services published alongside the level's datapack snapshot.
/// World facts are borrowed from Level for each evaluation.
pub struct ContainerResources {
    pub recipes: Arc<RecipeManager>,
    pub loot: Arc<LootData>,
    pub crafting: Arc<crate::inventory::RuntimeCraftingContext>,
}
#[derive(Default)]
pub struct ContainerRuntime {
    pub resources: Option<Arc<ContainerResources>>,
    pub sequences: RandomSequences,
    pub world_seed: i64,
}
