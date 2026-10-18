//! Disjoint canonical block-entity / loot random-stream borrows. Pure loot
//! evaluation cannot call back into Level while these fields are borrowed.
use super::{Level, LevelError};
use crate::{
    block::{
        containers::runtime::{ContainerResources, ContainerRuntime},
        entity::{BlockEntityRef, BlockEntityState},
    },
    loot::LootRandomSources,
};
use ferraloom_util::ChunkPos;
use std::sync::Arc;
impl Level {
    /// Port of CrafterBlock#getPotentialResults with its shared ten-entry cache.
    /// Matching borrows the canonical grid and never calls back into the level.
    pub(crate) fn crafter_menu_result(
        &mut self,
        player: crate::entity::EntityRef,
        menu: crate::inventory::MenuRef,
    ) -> crate::inventory::Result<crate::inventory::ItemStack> {
        use crate::{
            crafting::{CraftingInputView, RecipeInput},
            entity::{EntityDataView, player::ServerPlayer},
            inventory::{ItemStack, MenuContainer, MenuError},
        };
        let services = self.container_resources()?;
        let region = self.region_mut();
        let owner = region
            .entities
            .get(&player)
            .and_then(ServerPlayer::view)
            .ok_or(LevelError::MissingEntity(player))?;
        let menu = owner.menus.menu(menu)?;
        let grid: &[ItemStack] = if let Some(handle) = menu.block_entities.first() {
            let state = region
                .chunks
                .get(&ChunkPos::containing(handle.pos))
                .and_then(|chunk| chunk.block_entities.get(handle.pos))
                .filter(|state| state.data().generation == handle.generation)
                .or_else(|| region.block_entity_state.detached.get(handle))
                .ok_or(LevelError::MissingBlockEntity(*handle))?;
            &state
                .container()
                .ok_or(MenuError::Unsupported("crafter grid container"))?
                .items
        } else {
            match &menu.local[0] {
                MenuContainer::Simple(container) => container.items(),
                MenuContainer::Crafting(container) => container.items(),
                MenuContainer::Result(_) => return Err(MenuError::Unsupported("crafter grid")),
            }
        };
        let input = CraftingInputView::of_positioned(3, 3, grid)?.input;
        match region.container_runtime.crafter_recipes.get(
            &services.recipes,
            &input,
            services.crafting.as_ref(),
        )? {
            Some(recipe) => Ok(recipe
                .value
                .assemble(RecipeInput::Crafting(&input), services.crafting.as_ref())?),
            None => Ok(ItemStack::default()),
        }
    }
    pub fn with_loot_sources<R>(
        &mut self,
        operation: impl FnOnce(&mut LootRandomSources<'_>) -> R,
    ) -> R {
        let region = self.region_mut();
        operation(&mut LootRandomSources {
            world_seed: region.container_runtime.world_seed,
            sequences: &mut region.container_runtime.sequences,
            level_random: &mut region.random,
        })
    }
    pub fn container_runtime(&self) -> &ContainerRuntime {
        &self.region().container_runtime
    }
    pub fn container_runtime_mut(&mut self) -> &mut ContainerRuntime {
        &mut self.region_mut().container_runtime
    }
    pub fn container_resources(&self) -> Result<Arc<ContainerResources>, LevelError> {
        self.container_runtime()
            .resources
            .clone()
            .ok_or(LevelError::Inventory("container datapack resources"))
    }
    pub fn set_container_resources(&mut self, resources: Arc<ContainerResources>, world_seed: i64) {
        let runtime = self.container_runtime_mut();
        runtime.resources = Some(resources);
        runtime.world_seed = world_seed;
    }
    pub fn with_block_entity_and_loot_sources<R>(
        &mut self,
        handle: BlockEntityRef,
        operation: impl FnOnce(&mut BlockEntityState, &mut LootRandomSources<'_>) -> R,
    ) -> Result<R, LevelError> {
        let region = self.region_mut();
        let active = region
            .chunks
            .get_mut(&ChunkPos::containing(handle.pos))
            .and_then(|chunk| chunk.block_entities.get_mut(handle.pos))
            .filter(|state| state.data().generation == handle.generation);
        let state = match active {
            Some(state) => state,
            None => region
                .block_entity_state
                .detached
                .get_mut(&handle)
                .ok_or(LevelError::MissingBlockEntity(handle))?,
        };
        let mut sources = LootRandomSources {
            world_seed: region.container_runtime.world_seed,
            sequences: &mut region.container_runtime.sequences,
            level_random: &mut region.random,
        };
        Ok(operation(state, &mut sources))
    }
    /// Port of the same-state sendBlockUpdated call after an accepted sign edit.
    /// Detached receiver writes are retained, but never synchronize a replacement.
    pub fn block_entity_updated(&mut self, handle: BlockEntityRef) -> Result<(), LevelError> {
        if self.block_entity_ref(handle.pos)? != Some(handle) {
            return Ok(());
        }
        let state = self.get_block_state(handle.pos)?;
        self.send_block_updated(handle.pos, state, state);
        Ok(())
    }
}
