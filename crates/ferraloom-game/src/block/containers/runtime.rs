//! Immutable datapack services and region-owned random sequences for containers.
use crate::{
    crafting::RecipeManager,
    loot::{LootData, LootFacts, RandomSequences},
};
use std::sync::Arc;

/// Published alongside the level's datapack snapshot. Facts must report missing
/// world services explicitly; the container port never substitutes guessed facts.
pub struct ContainerResources {
    pub recipes: Arc<RecipeManager>,
    pub loot: Arc<LootData>,
    pub facts: Arc<dyn LootFacts + Send + Sync>,
}
#[derive(Default)]
pub struct ContainerRuntime {
    pub resources: Option<Arc<ContainerResources>>,
    pub sequences: RandomSequences,
    pub world_seed: i64,
}
