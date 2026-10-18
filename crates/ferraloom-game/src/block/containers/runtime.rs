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
    pub advancements: Arc<ServerAdvancementManager>,
    pub recipe_tags: BTreeMap<Identifier, Vec<Identifier>>,
    pub recipe_synchronization: Arc<RecipeSynchronization>,
}
#[derive(Default)]
pub struct ContainerRuntime {
    pub resources: Option<Arc<ContainerResources>>,
    pub sequences: RandomSequences,
    pub world_seed: i64,
}
