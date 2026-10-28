//! Invocation-owned live loot access. Source borrows end at each RNG operation
//! and each dynamic getItem, before the evaluator runs its next consumer.
use crate::{
    block::{
        containers::{access, storage::BaseContainerBlockEntity},
        entity::{BlockEntityRef, BlockEntityState},
    },
    inventory::Container,
    level::Level,
    loot::{
        LootData, LootError, LootFacts, LootParams, LootRandomSelection, LootRuntime, NameableName,
        Result,
    },
};
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    Identifier,
    component::{DataComponentMap, ItemStack},
};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use std::collections::BTreeMap;

enum SelectedStream {
    Level,
    Sequence(Identifier),
    Seeded(LegacyRandomSource),
}
#[derive(Clone, Copy, Debug)]
pub enum DynamicDrop {
    ContainerContents(BlockEntityRef),
    Unported(&'static str),
}
pub struct LiveLootRuntime<'a> {
    level: &'a mut Level,
    random: SelectedStream,
    dynamic: BTreeMap<Identifier, DynamicDrop>,
    block_entity_source: Option<BlockEntityRef>,
    roots: Vec<BlockEntityRef>,
}
impl<'a> LiveLootRuntime<'a> {
    pub fn new(level: &'a mut Level, selection: LootRandomSelection) -> Self {
        let random = match selection {
            LootRandomSelection::Level => SelectedStream::Level,
            LootRandomSelection::Seeded(seed) => {
                SelectedStream::Seeded(LegacyRandomSource::new(seed))
            }
            LootRandomSelection::Sequence(id) => {
                // LootContext.Builder#create obtains a named stream eagerly,
                // even if this evaluation never draws from it. Obtaining does
                // not dirty RandomSequences or advance the stream.
                {
                    let runtime = level.container_runtime_mut();
                    let _source = runtime.sequences.get(&id, runtime.world_seed);
                }
                SelectedStream::Sequence(id)
            }
        };
        Self {
            level,
            random,
            dynamic: BTreeMap::new(),
            block_entity_source: None,
            roots: Vec::new(),
        }
    }
    pub fn for_table(
        level: &'a mut Level,
        loot: &LootData,
        table: &Identifier,
        seed: i64,
    ) -> Result<Self> {
        Ok(Self::new(level, loot.random_selection(table, seed)?))
    }
    pub fn level(&self) -> &Level {
        self.level
    }
    pub fn level_mut(&mut self) -> &mut Level {
        self.level
    }
    /// Retain the actual BLOCK_ENTITY receiver, independently of any dynamic
    /// contents provider. Reentrant replacement cannot redirect later reads.
    pub fn bind_block_entity_source(&mut self, handle: BlockEntityRef) -> Result<()> {
        self.level.retain_block_entity(handle).map_err(required)?;
        if let Some(old) = self.block_entity_source.replace(handle) {
            self.level.release_block_entity(old);
        }
        Ok(())
    }
    pub fn block_entity_source(&self) -> Option<BlockEntityRef> {
        self.block_entity_source
    }
    fn retained_receiver(&self, params: &LootParams) -> Result<Option<&BlockEntityState>> {
        if params.block_entity.is_none() && params.block_entity_components.is_none() {
            return Ok(None);
        }
        let handle = self
            .block_entity_source
            .ok_or_else(|| LootError::Unsupported("block_entity.retained_receiver".into()))?;
        self.level
            .block_entity_state(handle)
            .map(Some)
            .map_err(required)
    }

    /// Port of LootParams.Builder#withDynamicDrop, including duplicate rejection.
    pub fn register_dynamic_drop(&mut self, id: Identifier, provider: DynamicDrop) -> Result<()> {
        if self.dynamic.contains_key(&id) {
            return Err(LootError::Decode(format!("duplicated dynamic drop {id}")));
        }
        if let DynamicDrop::ContainerContents(handle) = provider {
            self.level.retain_block_entity(handle).map_err(required)?;
            self.roots.push(handle);
        }
        self.dynamic.insert(id, provider);
        Ok(())
    }
}
impl LootRuntime for LiveLootRuntime<'_> {
    fn facts(&self) -> &dyn LootFacts {
        &*self.level
    }
    fn block_entity_nbt(&self, params: &LootParams) -> Result<Option<Tag>> {
        self.retained_receiver(params)?
            .map(|receiver| {
                receiver
                    .save_with_full_metadata()
                    .map(Tag::Compound)
                    .map_err(|error| LootError::Decode(error.to_string()))
            })
            .transpose()
    }
    fn block_entity_components(&self, params: &LootParams) -> Result<Option<DataComponentMap>> {
        Ok(self
            .retained_receiver(params)?
            .map(BlockEntityState::collect_components))
    }
    fn block_entity_display_name(&self, params: &LootParams) -> Result<Option<NameableName>> {
        let Some(receiver) = self.retained_receiver(params)? else {
            return Ok(None);
        };
        if !receiver.data().kind.is_nameable() {
            return Ok(None);
        }
        let custom_name = match receiver {
            BlockEntityState::Banner(entity) => entity.custom_name.clone(),
            BlockEntityState::Beacon(entity) => entity.custom_name.clone(),
            BlockEntityState::EnchantingTable(entity) => entity.custom_name.clone(),
            _ => receiver
                .container()
                .ok_or_else(|| {
                    LootError::Unsupported(format!(
                        "block_entity.custom_name: {}",
                        receiver.data().kind.name()
                    ))
                })?
                .custom_name
                .clone(),
        };
        Ok(Some(NameableName { custom_name }))
    }
    fn with_random(&mut self, operation: &mut dyn FnMut(&mut dyn RandomSource)) {
        match &mut self.random {
            SelectedStream::Level => operation(self.level.random()),
            SelectedStream::Seeded(random) => operation(random),
            SelectedStream::Sequence(id) => {
                let runtime = self.level.container_runtime_mut();
                let mut random = runtime.sequences.get(id, runtime.world_seed);
                operation(&mut random);
            }
        }
    }
    fn dynamic_drop_next(
        &mut self,
        id: &Identifier,
        ordinal: usize,
        _params: &LootParams,
    ) -> Result<Option<ItemStack>> {
        match self.dynamic.get(id).copied() {
            None => Ok(None),
            Some(DynamicDrop::Unported(method)) => Err(LootError::Unsupported(method.into())),
            Some(DynamicDrop::ContainerContents(handle)) => {
                let size = self
                    .level
                    .block_entity::<BaseContainerBlockEntity>(handle)
                    .map_err(required)?
                    .size();
                if ordinal >= size {
                    return Ok(None);
                }
                Ok(Some(
                    access::get_item(handle, ordinal, self.level).map_err(required)?,
                ))
            }
        }
    }
}
impl Drop for LiveLootRuntime<'_> {
    fn drop(&mut self) {
        if let Some(source) = self.block_entity_source {
            self.level.release_block_entity(source);
        }
        for root in self.roots.drain(..) {
            self.level.release_block_entity(root);
        }
    }
}
fn required(error: crate::level::LevelError) -> LootError {
    LootError::Unsupported(format!("canonical live loot access: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Behaviors, level::Region};
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    #[test]
    fn captured_data_without_a_bound_live_receiver_is_not_a_fallback() {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let runtime = LiveLootRuntime::new(&mut level, LootRandomSelection::Level);
        let absent = LootParams::default();
        assert!(runtime.block_entity_nbt(&absent).unwrap().is_none());
        assert!(runtime.block_entity_components(&absent).unwrap().is_none());
        assert!(
            runtime
                .block_entity_display_name(&absent)
                .unwrap()
                .is_none()
        );
        let present = LootParams {
            block_entity: Some(Tag::Compound(ferraloom_nbt::CompoundTag::new())),
            ..Default::default()
        };
        assert!(matches!(
            runtime.block_entity_nbt(&present),
            Err(LootError::Unsupported(_))
        ));
        assert!(matches!(
            runtime.block_entity_components(&present),
            Err(LootError::Unsupported(_))
        ));
        assert!(matches!(
            runtime.block_entity_display_name(&present),
            Err(LootError::Unsupported(_))
        ));
    }
    #[test]
    fn seeded_live_runtime_preserves_the_gaussian_cache_between_operations() {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let mut runtime = LiveLootRuntime::new(&mut level, LootRandomSelection::Seeded(-12345));
        let mut expected = LegacyRandomSource::new(-12345);
        for _ in 0..4 {
            let mut actual = 0.0;
            runtime.with_random(&mut |random| actual = random.next_gaussian());
            assert_eq!(actual.to_bits(), expected.next_gaussian().to_bits());
        }
    }
    #[test]
    fn named_live_runtime_initializes_without_a_draw_and_rejects_duplicate_providers() {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let id = Identifier::parse("test:empty").unwrap();
        let before = level.container_runtime().sequences.to_nbt();
        let mut runtime =
            LiveLootRuntime::new(&mut level, LootRandomSelection::Sequence(id.clone()));
        assert_ne!(
            runtime.level().container_runtime().sequences.to_nbt(),
            before
        );
        assert!(!runtime.level().container_runtime().sequences.is_dirty());
        assert!(
            runtime
                .dynamic_drop_next(&id, 0, &LootParams::default())
                .unwrap()
                .is_none()
        );
        runtime
            .register_dynamic_drop(id.clone(), DynamicDrop::Unported("test:pending_provider"))
            .unwrap();
        assert!(matches!(
            runtime
                .register_dynamic_drop(id.clone(), DynamicDrop::Unported("test:pending_provider")),
            Err(LootError::Decode(_))
        ));
        assert!(
            matches!(runtime.dynamic_drop_next(&id, 0, &LootParams::default()), Err(LootError::Unsupported(method)) if method == "test:pending_provider")
        );
    }
}
