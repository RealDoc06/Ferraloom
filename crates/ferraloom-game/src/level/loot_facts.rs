//! Canonical facts for one loot invocation. The runtime reborrows Level's random
//! source only around random operations; no inventory or detached snapshot lives here.
use super::{
    Level, LevelError,
    environment::{AttributeKey, TriState},
};
use crate::{
    block::{LevelReaderRef, entity::BlockEntityRef},
    entity::{EntityData, EntityRef},
    loot::{EnvironmentValue, Identifier, LootError, LootFacts, LootParams, Result},
    predicates::{PredicateContext, PredicateRegistry},
};
use ferraloom_data::{
    Block, BlockState, EntityType, Item,
    datapack::TagSet,
    registries::{attribute_type::Id as AttributeType, environment_attribute::Id as Attribute},
};
use ferraloom_nbt::Tag;
use ferraloom_protocol::component::{DataComponentMap, ItemStack};
use ferraloom_util::{BlockPos, ChunkPos, Vec3};
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
            .map_err(required)?
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
    fn entity_tick_count(&self, entity: EntityRef) -> Result<i32> {
        Ok(self
            .entity::<EntityData>(entity)
            .map_err(required)?
            .tick_count)
    }
    fn entity_vehicle(&self, entity: EntityRef) -> Result<Option<EntityRef>> {
        Ok(self.entity::<EntityData>(entity).map_err(required)?.vehicle)
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
    use std::sync::Arc;
    static EMPTY: Behaviors = Behaviors::EMPTY;

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
