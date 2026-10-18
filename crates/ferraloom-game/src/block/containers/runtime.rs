//! Immutable datapack services and region-owned random sequences for containers.
use crate::{
    advancements::ServerAdvancementManager,
    crafting::{RecipeManager, RecipeSynchronization},
    loot::{LootData, RandomSequences},
};
use ferraloom_protocol::Identifier;
use std::{collections::BTreeMap, sync::Arc};

/// Immutable services published alongside the level's datapack snapshot.
/// World facts are borrowed from Level for each evaluation.
pub struct ContainerResources {
    pub recipes: Arc<RecipeManager>,
    pub loot: Arc<LootData>,
    pub crafting: Arc<crate::inventory::RuntimeCraftingContext>,
    pub advancements: Arc<ServerAdvancementManager>,
    pub recipe_tags: BTreeMap<Identifier, Vec<Identifier>>,
    pub recipe_synchronization: Arc<RecipeSynchronization>,
}
pub struct ContainerRuntime {
    pub resources: Option<Arc<ContainerResources>>,
    pub sequences: RandomSequences,
    pub world_seed: i64,
    pub(crate) crafter_recipes: crate::crafting::RecipeCache,
}
impl Default for ContainerRuntime {
    fn default() -> Self {
        Self {
            resources: None,
            sequences: RandomSequences::default(),
            world_seed: 0,
            crafter_recipes: crate::crafting::RecipeCache::new(10),
        }
    }
}
