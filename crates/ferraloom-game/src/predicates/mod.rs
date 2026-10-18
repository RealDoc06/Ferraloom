//! Shared advancement, command and loot predicates.
mod block;
mod bounds;
mod entity;
mod item;
mod location;
pub use block::*;
pub use bounds::*;
pub use entity::*;
pub use item::*;
pub use location::*;

use crate::{
    entity::EntityRef,
    loot::{Identifier, LootError, Result},
};
use ferraloom_data::datapack::TagSet;
use ferraloom_data::{Block, EntityType, Item};
use ferraloom_nbt::Tag;
use ferraloom_util::{BlockPos, Vec3};

/// Registry identity for predicates whose holders belong to dynamic registries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateRegistry {
    Biome,
    Structure,
    Fluid,
    Enchantment,
    Potion,
    VillagerType,
    DamageType,
}
/// World facts remain typed and query canonical state at the evaluation point.
/// Absence and unsupported facts have distinct meanings.
pub trait PredicateContext {
    fn item_tags(&self) -> Result<&TagSet<Item>> {
        Err(LootError::Unsupported("tags.item".into()))
    }
    fn block_tags(&self) -> Result<&TagSet<Block>> {
        Err(LootError::Unsupported("tags.block".into()))
    }
    fn entity_tags(&self) -> Result<&TagSet<EntityType>> {
        Err(LootError::Unsupported("tags.entity_type".into()))
    }
    fn tag_contains(
        &self,
        registry: PredicateRegistry,
        _tag: &Identifier,
        _value: &Identifier,
    ) -> Result<bool> {
        Err(LootError::Unsupported(format!("tags.{registry:?}")))
    }
    fn loaded(&self, _pos: BlockPos) -> Result<bool> {
        Err(LootError::Unsupported("level.is_loaded".into()))
    }
    fn block(&self, _pos: BlockPos) -> Result<ferraloom_data::BlockState> {
        Err(LootError::Unsupported("level.block_state".into()))
    }
    fn block_entity_nbt(&self, _pos: BlockPos) -> Result<Option<Tag>> {
        Err(LootError::Unsupported("block_entity.nbt".into()))
    }
    fn block_entity_components(
        &self,
        _pos: BlockPos,
    ) -> Result<Option<ferraloom_protocol::component::DataComponentMap>> {
        Err(LootError::Unsupported("block_entity.components".into()))
    }
    fn dimension(&self) -> Result<Identifier> {
        Err(LootError::Unsupported("level.dimension".into()))
    }
    fn biome(&self, _pos: BlockPos) -> Result<Identifier> {
        Err(LootError::Unsupported("level.biome".into()))
    }
    fn in_structure(
        &self,
        _pos: BlockPos,
        _set: &ferraloom_protocol::codec::HolderSet,
    ) -> Result<bool> {
        Err(LootError::Unsupported("level.structure".into()))
    }
    fn smokey(&self, _pos: BlockPos) -> Result<bool> {
        Err(LootError::Unsupported("level.smokey".into()))
    }
    fn light(&self, _pos: BlockPos) -> Result<i32> {
        Err(LootError::Unsupported(
            "level.max_local_raw_brightness".into(),
        ))
    }
    fn can_see_sky(&self, _pos: BlockPos) -> Result<bool> {
        Err(LootError::Unsupported("level.can_see_sky".into()))
    }
    fn fluid(&self, _pos: BlockPos) -> Result<FluidState> {
        Err(LootError::Unsupported("level.fluid_state".into()))
    }
    fn entity_type(&self, _entity: EntityRef) -> Result<EntityType> {
        Err(LootError::Unsupported("entity.type".into()))
    }
    fn entity_position(&self, _entity: EntityRef) -> Result<Vec3> {
        Err(LootError::Unsupported("entity.position".into()))
    }
    fn entity_nbt(&self, _entity: EntityRef) -> Result<Tag> {
        Err(LootError::Unsupported(
            "entity.nbt_with_selected_item".into(),
        ))
    }
    fn entity_flags(&self, _entity: EntityRef) -> Result<EntityFlags> {
        Err(LootError::Unsupported("entity.flags".into()))
    }
}
