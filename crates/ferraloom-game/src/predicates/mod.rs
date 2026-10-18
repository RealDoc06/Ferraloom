//! Shared advancement, command and loot predicates.
mod block;
mod bounds;
mod damage;
mod entity;
mod item;
mod location;
pub(crate) use block::property_values;
pub use block::*;
pub use bounds::*;
pub use damage::*;
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
    Instrument,
    JukeboxSong,
    TrimMaterial,
    TrimPattern,
    Attribute,
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
    fn resolve_component(
        &self,
        _component: &ferraloom_protocol::chat::Component,
        _entity: EntityRef,
    ) -> Result<ferraloom_protocol::chat::Component> {
        Err(LootError::Unsupported("component.resolution".into()))
    }
    fn entity_custom_name(
        &self,
        _entity: EntityRef,
    ) -> Result<Option<ferraloom_protocol::chat::Component>> {
        Err(LootError::Unsupported("entity.custom_name".into()))
    }
    fn registry_values(&self, registry: PredicateRegistry) -> Result<Vec<Identifier>> {
        Err(LootError::Unsupported(format!("registry.{registry:?}")))
    }
    fn registry_tag_values(
        &self,
        registry: PredicateRegistry,
        _tag: &Identifier,
    ) -> Result<Vec<Identifier>> {
        Err(LootError::Unsupported(format!(
            "registry.{registry:?}.tag_values"
        )))
    }
    fn potion_effects(
        &self,
        _id: &Identifier,
    ) -> Result<Vec<ferraloom_protocol::component::MobEffectInstance>> {
        Err(LootError::Unsupported("potion.effects".into()))
    }
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
    fn entity_equipment(
        &self,
        _entity: EntityRef,
    ) -> Result<
        Option<
            Vec<(
                ferraloom_protocol::component::EquipmentSlot,
                ferraloom_protocol::component::ItemStack,
            )>,
        >,
    > {
        Err(LootError::Unsupported("entity.living_equipment".into()))
    }
    fn entity_specific(&self, _entity: EntityRef) -> Result<EntitySpecificFacts> {
        Err(LootError::Unsupported("entity.class_specific_state".into()))
    }
    fn entity_team(&self, _entity: EntityRef) -> Result<Option<String>> {
        Err(LootError::Unsupported("entity.team".into()))
    }
    fn entity_tick_count(&self, _entity: EntityRef) -> Result<i32> {
        Err(LootError::Unsupported("entity.tick_count".into()))
    }
    fn entity_vehicle(&self, _entity: EntityRef) -> Result<Option<EntityRef>> {
        Err(LootError::Unsupported("entity.vehicle".into()))
    }
    fn entity_target(&self, _entity: EntityRef) -> Result<Option<EntityRef>> {
        Err(LootError::Unsupported("entity.mob_target".into()))
    }
    fn entity_passengers(&self, _entity: EntityRef) -> Result<Vec<EntityRef>> {
        Err(LootError::Unsupported("entity.passengers".into()))
    }
    fn entity_scoreboard_tags(
        &self,
        _entity: EntityRef,
    ) -> Result<std::collections::BTreeSet<String>> {
        Err(LootError::Unsupported("entity.scoreboard_tags".into()))
    }
    fn entity_components(
        &self,
        _entity: EntityRef,
    ) -> Result<ferraloom_protocol::component::DataComponentMap> {
        Err(LootError::Unsupported("entity.components".into()))
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
