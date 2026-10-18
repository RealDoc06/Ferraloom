//! Shared advancement, command and loot predicates.
mod block;
mod bounds;
mod damage;
mod entity;
mod item;
mod location;
mod movement;
mod player;
pub use block::*;
pub use bounds::*;
pub use damage::*;
pub use entity::*;
pub use item::*;
pub use location::*;
pub use movement::*;
pub use player::*;

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
    fn entity_slots(
        &self,
        _entity: EntityRef,
        _slots: &[i32],
    ) -> Result<Vec<ferraloom_protocol::component::ItemStack>> {
        Err(LootError::Unsupported("entity.slots".into()))
    }
    /// Per-slot reads let the live context preserve equipment predicate order.
    /// Standalone contexts can retain their existing owned equipment adapter.
    fn entity_has_equipment(&self, entity: EntityRef) -> Result<bool> {
        Ok(self.entity_equipment(entity)?.is_some())
    }
    fn entity_equipment_slot(
        &self,
        entity: EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
    ) -> Result<Option<ferraloom_protocol::component::ItemStack>> {
        Ok(self.entity_equipment(entity)?.map(|items| {
            items
                .into_iter()
                .find(|(current, _)| *current == slot)
                .map(|(_, stack)| stack)
                .unwrap_or_default()
        }))
    }
    fn entity_effects(
        &self,
        _entity: EntityRef,
    ) -> Result<Option<Vec<ferraloom_protocol::component::MobEffectInstance>>> {
        Err(LootError::Unsupported("entity.living_effects".into()))
    }
    fn entity_movement(&self, _entity: EntityRef) -> Result<(Vec3, f64)> {
        Err(LootError::Unsupported(
            "entity.known_movement_and_fall_distance".into(),
        ))
    }
    fn entity_on_pos(&self, _entity: EntityRef) -> Result<BlockPos> {
        Err(LootError::Unsupported("entity.on_pos".into()))
    }
    fn entity_movement_block_pos(&self, _entity: EntityRef) -> Result<BlockPos> {
        Err(LootError::Unsupported(
            "entity.block_pos_below_movement".into(),
        ))
    }
    fn lightning_state(&self, _entity: EntityRef) -> Result<Option<(i32, Vec<EntityRef>)>> {
        Err(LootError::Unsupported("entity.lightning_state".into()))
    }
    fn player_experience_level(&self, _entity: EntityRef) -> Result<Option<i32>> {
        Err(LootError::Unsupported(
            "entity.server_player_experience_level".into(),
        ))
    }
    fn player_food(&self, _entity: EntityRef) -> Result<(i32, f32)> {
        Err(LootError::Unsupported("player.food".into()))
    }
    fn player_gamemode(&self, _entity: EntityRef) -> Result<ferraloom_protocol::play::GameType> {
        Err(LootError::Unsupported("player.gamemode".into()))
    }
    fn player_stat(
        &self,
        _entity: EntityRef,
        _stat_type: &Identifier,
        _stat: &Identifier,
    ) -> Result<i32> {
        Err(LootError::Unsupported("player.stat".into()))
    }
    fn player_recipe(&self, _entity: EntityRef, _recipe: &Identifier) -> Result<bool> {
        Err(LootError::Unsupported("player.recipe_book".into()))
    }
    fn player_advancement(
        &self,
        _entity: EntityRef,
        _advancement: &Identifier,
    ) -> Result<Option<AdvancementFacts>> {
        Err(LootError::Unsupported("player.advancement_progress".into()))
    }
    fn player_looking_at(&self, _entity: EntityRef) -> Result<Option<EntityRef>> {
        Err(LootError::Unsupported(
            "player.looking_at_raycast_100".into(),
        ))
    }
    fn player_has_line_of_sight(&self, _entity: EntityRef, _target: EntityRef) -> Result<bool> {
        Err(LootError::Unsupported("player.line_of_sight".into()))
    }
    fn player_input(&self, _entity: EntityRef) -> Result<[bool; 7]> {
        Err(LootError::Unsupported("player.last_client_input".into()))
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

/// Fixed registry holders reject absent entries before evaluation, like RegistryOps.
pub(crate) fn validate_static_holders(
    holders: &ferraloom_protocol::codec::HolderSet,
    registry: &'static str,
    contains: impl Fn(&str) -> bool,
) -> Result<()> {
    if let ferraloom_protocol::codec::HolderSet::Direct(ids) = holders {
        for id in ids {
            if !contains(id.as_str()) {
                return Err(LootError::MissingReference {
                    registry,
                    id: id.clone(),
                });
            }
        }
    }
    Ok(())
}
