//! Canonical facts for one loot invocation. The runtime reborrows Level's random
//! source only around random operations; no inventory or detached snapshot lives here.
use super::{
    Level, LevelError,
    environment::{AttributeKey, TriState},
};
use crate::{
    block::{LevelReaderRef, entity::BlockEntityRef},
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    loot::{EnvironmentValue, Identifier, LootError, LootFacts, LootParams, Result},
    predicates::{EntityFlags, PredicateContext, PredicateRegistry},
};
use ferraloom_data::{
    Block, BlockState, EntityType, Item,
    datapack::TagSet,
    registries::{attribute_type::Id as AttributeType, environment_attribute::Id as Attribute},
};
use ferraloom_nbt::Tag;
use ferraloom_protocol::component::{DataComponentMap, EquipmentSlot, ItemStack};
use ferraloom_util::{BlockPos, Vec3};
use std::collections::BTreeSet;

// The per-invocation runtime returns its held Level through the existing cold
// facts interface. Random callbacks take a separate short mutable reborrow.
impl Level {
    fn loot_block_entity(&self, pos: BlockPos) -> Result<Option<BlockEntityRef>> {
        self.block_entity_ref(pos).map_err(required)
    }
    fn loot_attribute<T: ferraloom_worldgen::environment::AttributeValue>(
        &self,
        attribute: Attribute,
        params: &LootParams,
    ) -> Result<T> {
        let key = AttributeKey::<T>::new(attribute);
        if attribute.data().positional {
            let pos = params.origin.ok_or(LootError::MissingParameter(
                crate::loot::ContextParam::Origin,
            ))?;
            self.get_value_at(key, pos, None).map_err(required)
        } else {
            self.get_dimension_value(key).map_err(required)
        }
    }
}
fn required(error: LevelError) -> LootError {
    LootError::Unsupported(format!("canonical level fact: {error:?}"))
}
fn identifier(value: &ferraloom_data::datapack::Identifier) -> Result<Identifier> {
    Ok(Identifier::parse(&value.to_string())?)
}
fn attribute(id: &Identifier) -> Result<Attribute> {
    Attribute::from_name(id.as_str()).ok_or_else(|| LootError::MissingReference {
        registry: "environment_attribute",
        id: id.clone(),
    })
}
impl PredicateContext for Level {
    fn item_tags(&self) -> Result<&TagSet<Item>> {
        self.item_tags().map_err(required)
    }
    fn block_tags(&self) -> Result<&TagSet<Block>> {
        self.block_tags().map_err(required)
    }
    fn entity_tags(&self) -> Result<&TagSet<EntityType>> {
        self.entity_type_tags().map_err(required)
    }
    fn tag_contains(
        &self,
        registry: PredicateRegistry,
        tag: &Identifier,
        value: &Identifier,
    ) -> Result<bool> {
        match registry {
            PredicateRegistry::Fluid => {
                let tags = self.fluid_tags().map_err(required)?;
                let name = tag.as_str().parse()?;
                let value = ferraloom_data::registries::fluid::Id::from_name(value.as_str())
                    .ok_or_else(|| LootError::MissingReference {
                        registry: "fluid",
                        id: value.clone(),
                    })?;
                Ok(tags
                    .resolve_name(&name)
                    .is_some_and(|tag| tags.contains(tag, value)))
            }
            _ => Err(LootError::Unsupported(format!("tags.{registry:?}"))),
        }
    }
    fn loaded(&self, pos: BlockPos) -> Result<bool> {
        Ok(Level::is_loaded(self, pos))
    }
    fn smokey(&self, pos: BlockPos) -> Result<bool> {
        crate::block::campfire::is_smokey_pos(&LevelReaderRef::Server(self), pos).map_err(required)
    }
    fn block(&self, pos: BlockPos) -> Result<BlockState> {
        self.get_block_state(pos).map_err(required)
    }
    fn block_entity_nbt(&self, pos: BlockPos) -> Result<Option<Tag>> {
        self.loot_block_entity(pos)?
            .map(|handle| {
                let receiver = self.block_entity_state(handle).map_err(required)?;
                Ok(Tag::Compound(receiver.save_with_full_metadata()?))
            })
            .transpose()
    }
    fn block_entity_components(&self, pos: BlockPos) -> Result<Option<DataComponentMap>> {
        self.loot_block_entity(pos)?
            .map(|handle| {
                Ok(self
                    .block_entity_state(handle)
                    .map_err(required)?
                    .collect_components())
            })
            .transpose()
    }
    fn dimension(&self) -> Result<Identifier> {
        identifier(self.dimension_key().map_err(required)?)
    }
    fn biome(&self, pos: BlockPos) -> Result<Identifier> {
        identifier(self.biome_at(pos).map_err(required)?)
    }
    fn in_structure(
        &self,
        pos: BlockPos,
        set: &ferraloom_protocol::codec::HolderSet,
    ) -> Result<bool> {
        Ok(self
            .structure_manager()
            .get_structure_with_piece_at_in(pos, set)
            .map_err(|error| match error {
                LevelError::MissingChunk(chunk) => {
                    // StructureManager reads the query's references before
                    // consulting STRUCTURE_STARTS in referenced source chunks.
                    let status = if chunk == ferraloom_util::ChunkPos::containing(pos) {
                        super::chunk_holder::ChunkStatus::StructureReferences
                    } else {
                        super::chunk_holder::ChunkStatus::StructureStarts
                    };
                    LootError::Unsupported(format!(
                        "StructureManager#getStructureWithPieceAt: MissingChunk({chunk:?}), required status {status:?}"
                    ))
                }
                error => required(error),
            })?
            .is_some())
    }
    fn light(&self, pos: BlockPos) -> Result<i32> {
        self.max_local_raw_brightness(pos).map_err(required)
    }
    fn can_see_sky(&self, pos: BlockPos) -> Result<bool> {
        LevelReaderRef::Server(self)
            .can_see_sky(pos)
            .map_err(required)
    }
    fn player_experience_level(&self, entity: EntityRef) -> Result<Option<i32>> {
        if self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .entity_type
            != EntityType::PLAYER
        {
            return Ok(None);
        }
        Ok(Some(
            self.entity::<ServerPlayer>(entity)
                .map_err(required)?
                .experience_level,
        ))
    }
    fn player_gamemode(&self, entity: EntityRef) -> Result<ferraloom_protocol::play::GameType> {
        use ferraloom_protocol::play::GameType as Wire;
        Ok(
            match self
                .entity::<ServerPlayer>(entity)
                .map_err(required)?
                .game_mode
            {
                GameType::Survival => Wire::Survival,
                GameType::Creative => Wire::Creative,
                GameType::Adventure => Wire::Adventure,
                GameType::Spectator => Wire::Spectator,
            },
        )
    }
    fn player_stat(
        &self,
        entity: EntityRef,
        stat_type: &Identifier,
        stat: &Identifier,
    ) -> Result<i32> {
        let kind = crate::stats::StatType::from_name(stat_type.as_str()).ok_or_else(|| {
            LootError::MissingReference {
                registry: "stat_type",
                id: stat_type.clone(),
            }
        })?;
        Ok(self
            .entity::<ServerPlayer>(entity)
            .map_err(required)?
            .stats
            .get_value(kind.decode_stat(stat)?))
    }
    fn player_recipe(&self, entity: EntityRef, recipe: &Identifier) -> Result<bool> {
        Ok(self
            .entity::<ServerPlayer>(entity)
            .map_err(required)?
            .recipe_book
            .contains(&recipe.as_str().parse()?))
    }
    fn player_advancement(
        &self,
        entity: EntityRef,
        id: &Identifier,
    ) -> Result<Option<crate::predicates::AdvancementFacts>> {
        Ok(self
            .entity::<ServerPlayer>(entity)
            .map_err(required)?
            .advancements
            .get_progress(id)
            .map(|progress| crate::predicates::AdvancementFacts {
                done: progress.is_done(),
                criteria: progress
                    .criteria
                    .iter()
                    .map(|(name, criterion)| (name.clone(), criterion.obtained.is_some()))
                    .collect(),
            }))
    }
    fn player_input(&self, entity: EntityRef) -> Result<[bool; 7]> {
        let input = self
            .entity::<ServerPlayer>(entity)
            .map_err(required)?
            .last_client_input;
        Ok([
            input.forward,
            input.backward,
            input.left,
            input.right,
            input.jump,
            input.shift,
            input.sprint,
        ])
    }
    fn entity_type(&self, entity: EntityRef) -> Result<EntityType> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .entity_type)
    }
    fn entity_position(&self, entity: EntityRef) -> Result<Vec3> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .position)
    }
    /// Port of NbtPredicate#getEntityTagToCompare through the canonical serializer.
    fn entity_nbt(&self, entity: EntityRef) -> Result<Tag> {
        self.entity_nbt_to_compare(entity)
            .map(Tag::Compound)
            .map_err(required)
    }
    fn entity_has_equipment(&self, entity: EntityRef) -> Result<bool> {
        Ok(self
            .entity_state(entity)
            .map_err(required)?
            .as_living()
            .is_some())
    }
    fn entity_equipment_slot(
        &self,
        entity: EntityRef,
        slot: EquipmentSlot,
    ) -> Result<Option<ItemStack>> {
        Ok(self
            .entity_behavior(entity)
            .map_err(required)?
            .item_by_slot(entity, slot, self)
            .map_err(required)?
            .cloned())
    }
    fn entity_equipment(
        &self,
        entity: EntityRef,
    ) -> Result<Option<Vec<(EquipmentSlot, ItemStack)>>> {
        if !self.entity_has_equipment(entity)? {
            return Ok(None);
        }
        crate::entity::player::equipment::SLOTS
            .into_iter()
            .map(|slot| {
                Ok((
                    slot,
                    self.entity_equipment_slot(entity, slot)?
                        .expect("living equipment slot"),
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }
    fn entity_on_pos(&self, entity: EntityRef) -> Result<BlockPos> {
        self.entity_behavior(entity)
            .map_err(required)?
            .on_pos(entity, self)
            .map_err(required)
    }
    fn entity_movement_block_pos(&self, entity: EntityRef) -> Result<BlockPos> {
        self.entity_behavior(entity)
            .map_err(required)?
            .block_pos_below_that_affects_my_movement(entity, self)
            .map_err(required)
    }
    fn entity_flags(&self, entity: EntityRef) -> Result<EntityFlags> {
        let state = self.entity_state(entity).map_err(required)?;
        let base = state.as_entity();
        let living = state.as_living().is_some();
        let fall_flying = living && base.shared_flag(7);
        Ok(EntityFlags {
            on_ground: base.on_ground,
            on_fire: crate::entity::inside_block::is_on_fire(entity, self).map_err(required)?,
            crouching: base.pose() == ferraloom_protocol::entity_data::Pose::Crouching,
            sprinting: base.shared_flag(3),
            swimming: self
                .entity_behavior(entity)
                .map_err(required)?
                .is_swimming(entity, self)
                .map_err(required)?,
            flying: fall_flying
                || matches!(state, EntityState::Player(player) if player.abilities.flying),
            baby: self
                .entity_behavior(entity)
                .map_err(required)?
                .is_baby(entity, self)
                .map_err(required)?,
            living,
            in_water: base.was_touching_water,
            fall_flying,
        })
    }
    fn entity_tick_count(&self, entity: EntityRef) -> Result<i32> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .tick_count)
    }
    fn entity_vehicle(&self, entity: EntityRef) -> Result<Option<EntityRef>> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .vehicle
            .get())
    }
    fn entity_passengers(&self, entity: EntityRef) -> Result<Vec<EntityRef>> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .passengers
            .clone())
    }
    fn entity_scoreboard_tags(&self, entity: EntityRef) -> Result<BTreeSet<String>> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .tags
            .iter()
            .cloned()
            .collect())
    }
}
impl LootFacts for Level {
    /// Port of StorageNbtProvider#get on the server's canonical CommandStorage.
    /// Missing services and deferred namespace load failures remain required.
    fn storage(&self, id: &Identifier) -> Result<Tag> {
        self.command_storage()
            .ok_or_else(|| LootError::Unsupported("command_storage".into()))?
            .get(id)
            .map(Tag::Compound)
            .map_err(LootError::CommandStorage)
    }
    fn item_enabled(&self, stack: &ItemStack) -> Result<bool> {
        Ok(stack.is_empty()
            || stack
                .0
                .as_ref()
                .is_some_and(|stack| stack.item.is_enabled_in_vanilla()))
    }
    fn weather(&self) -> Result<(bool, bool)> {
        Ok((self.is_raining(), self.is_thundering()))
    }
    fn clock_ticks(&self, clock: &Identifier) -> Result<i64> {
        self.clock_ticks(&clock.as_str().parse()?).map_err(required)
    }
    fn environment_float(&self, id: &Identifier, params: &LootParams) -> Result<f32> {
        self.loot_attribute(attribute(id)?, params)
    }
    fn environment_value(&self, id: &Identifier, params: &LootParams) -> Result<EnvironmentValue> {
        let attribute = attribute(id)?;
        Ok(match attribute.data().attribute_type {
            AttributeType::BOOLEAN => {
                EnvironmentValue::Boolean(self.loot_attribute(attribute, params)?)
            }
            AttributeType::FLOAT | AttributeType::ANGLE_DEGREES => {
                EnvironmentValue::Float(self.loot_attribute(attribute, params)?)
            }
            AttributeType::TRI_STATE => EnvironmentValue::Enum(
                match self.loot_attribute::<TriState>(attribute, params)? {
                    TriState::True => "true",
                    TriState::False => "false",
                    TriState::Default => "default",
                }
                .into(),
            ),
            _ => {
                return Err(required(LevelError::UnsupportedEnvironmentAttribute(
                    attribute,
                )));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_data::datapack::ResourceManager;
    use ferraloom_util::ChunkPos;
    use std::sync::Arc;
    static EMPTY: Behaviors = Behaviors::EMPTY;

    #[test]
    fn missing_command_storage_snapshot_is_a_required_loot_fact() {
        let level = Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        let id = "test:read".parse().unwrap();
        let error = LootFacts::storage(&level, &id).unwrap_err();
        assert!(matches!(error, LootError::Unsupported(ref fact) if fact == "command_storage"));
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn command_storage_loot_read_preserves_the_actual_error_source() {
        use crate::commands::storage::{CommandStorage, StorageReadError};
        let mut level = Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        // The test executable is a file, not a directory. Preload retains that
        // real IO failure until the namespace's first required read.
        let storage = CommandStorage::preload(&std::env::current_exe().unwrap());
        let id = "test:read".parse().unwrap();
        let original = storage.get(&id).unwrap_err();
        level.set_command_storage(storage.snapshot());
        let error = LootFacts::storage(&level, &id).unwrap_err();
        let source = std::error::Error::source(&error).unwrap();
        let source = source.downcast_ref::<StorageReadError>().unwrap();
        assert_eq!(source.namespace, "test");
        assert!(Arc::ptr_eq(&source.source, &original.source));
        assert!(std::error::Error::source(source).is_some());
        assert_eq!(error.to_string(), original.to_string());
    }

    #[test]
    fn canonical_facts_observe_mutations_and_keep_missing_context_explicit() {
        let mut level = Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        assert!(matches!(
            PredicateContext::item_tags(&level),
            Err(LootError::Unsupported(_))
        ));
        let pos = BlockPos::new(8, 8, 8);
        assert!(!PredicateContext::loaded(&level, pos).unwrap());
        let err = PredicateContext::block(&level, pos).unwrap_err();
        assert!(
            matches!(err, LootError::Unsupported(ref message) if message.contains("MissingChunk"))
        );
        level
            .add_chunk(LevelChunk::new(
                ChunkPos::new(0, 0),
                0..1,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        assert!(PredicateContext::loaded(&level, pos).unwrap());
        assert!(!PredicateContext::loaded(&level, BlockPos::new(8, 16, 8)).unwrap());
        for block in [Block::STONE, Block::AIR, Block::DIRT] {
            level.set_block_state(pos, block.default_state()).unwrap();
            assert_eq!(
                PredicateContext::block(&level, pos).unwrap(),
                block.default_state()
            );
        }
        let tags = Arc::new(TagSet::<Item>::load(&ResourceManager::new(vec![])).unwrap());
        level.set_item_tags(tags.clone());
        assert!(std::ptr::eq(
            PredicateContext::item_tags(&level).unwrap(),
            tags.as_ref()
        ));
        assert!(LootFacts::item_enabled(&level, &ItemStack::default()).unwrap());
        assert!(
            LootFacts::item_enabled(&level, &ItemStack::new(Item::STONE, 1, Default::default()))
                .unwrap()
        );
        assert!(matches!(
            LootFacts::score(
                &level,
                crate::loot::ScoreHolder::Name("missing"),
                "objective"
            ),
            Err(LootError::Unsupported(_))
        ));
        let missing = LootFacts::environment_float(
            &level,
            &Identifier::parse("gameplay/sky_light_level").unwrap(),
            &LootParams::default(),
        );
        assert!(matches!(missing, Err(LootError::Unsupported(_))));
    }
}
