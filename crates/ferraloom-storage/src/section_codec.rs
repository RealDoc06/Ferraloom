use crate::{
    BlockStatePaletteEntry, ChunkSection, Error, LongArray, NbtField, PackedPalette, Result,
};
use ferraloom_chunk::{
    BiomeId, ContainerKind, LevelChunkSection, PackedData, PalettedContainer, SimpleBitStorage,
    Strategy,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, NbtString};
use std::{collections::HashMap, hash::Hash};

/// Bridge to the world's registry context. Biome IDs must come from the live dimension registry.
/// Block state name/property resolution is owned by the game registry, not numeric disk IDs.
pub trait SectionRegistry {
    fn block_state(&self, entry: &BlockStatePaletteEntry) -> Result<BlockState>;
    fn block_state_entry(&self, state: BlockState) -> Result<BlockStatePaletteEntry>;
    fn biome(&self, name: &NbtString) -> Result<BiomeId>;
    fn biome_name(&self, biome: BiomeId) -> Result<NbtString>;
    fn biome_strategy(&self) -> Strategy<BiomeId>;
    fn default_biome(&self) -> BiomeId;
}
fn container_error(error: ferraloom_chunk::ContainerError) -> Error {
    Error::Invalid(format!("invalid chunk palette: {error:?}"))
}

impl<T: NbtField> PackedPalette<T> {
    fn unpack<V: Copy + Eq + Hash>(
        &self,
        strategy: Strategy<V>,
        resolve: impl Fn(&T) -> Result<V>,
    ) -> Result<PalettedContainer<V>> {
        let entries: Vec<V> = self.palette.iter().map(resolve).collect::<Result<_>>()?;
        if entries.is_empty() {
            return Err(Error::Invalid("empty chunk palette".into()));
        }
        if entries
            .iter()
            .any(|&value| strategy.registry().id(value).is_none())
        {
            return Err(Error::Invalid(
                "palette value is absent from runtime registry".into(),
            ));
        }
        let config = strategy.configuration_for_palette_size(entries.len());
        let raw = self
            .data
            .as_ref()
            .map(|data| data.0.iter().map(|&v| v as u64).collect::<Vec<_>>());
        // The container's hot-path getters assume valid indices. Check untrusted disk data first.
        if config.bits_in_storage != 0 {
            let storage = SimpleBitStorage::from_raw(
                config.bits_in_storage,
                strategy.entry_count(),
                raw.clone()
                    .ok_or_else(|| Error::Invalid("palette is missing packed data".into()))?,
            )
            .map_err(container_error)?;
            if (0..storage.len()).any(|index| storage.get(index) as usize >= entries.len()) {
                return Err(Error::Invalid(
                    "packed chunk palette index is out of bounds".into(),
                ));
            }
        }
        PalettedContainer::unpack(
            strategy,
            PackedData {
                palette_entries: entries,
                storage: raw,
                bits_per_entry: None,
            },
        )
        .map_err(container_error)
    }
    fn update<V: Copy + Eq + Hash>(
        &mut self,
        container: &PalettedContainer<V>,
        resolve: impl Fn(&T) -> Result<V>,
        encode: impl Fn(V) -> Result<T>,
    ) -> Result<()> {
        let previous = self.unpack(container.strategy().clone(), &resolve)?;
        if (0..container.strategy().entry_count())
            .all(|index| previous.get_by_index(index) == container.get_by_index(index))
        {
            return Ok(());
        }
        // Keep unused original palette entries too: extensions on them are still part of the save.
        let mut ids = HashMap::new();
        for (index, entry) in self.palette.iter().enumerate() {
            ids.entry(resolve(entry)?).or_insert(index as u32);
        }
        let packed = container.pack();
        for value in packed.palette_entries {
            if let std::collections::hash_map::Entry::Vacant(entry) = ids.entry(value) {
                entry.insert(self.palette.len() as u32);
                self.palette.push(encode(value)?);
            }
        }
        let bits = container
            .strategy()
            .configuration_for_palette_size(self.palette.len())
            .bits_in_storage;
        self.data = if bits == 0 {
            None
        } else {
            let values: Vec<_> = (0..container.strategy().entry_count())
                .map(|index| ids[&container.get_by_index(index)])
                .collect();
            Some(LongArray(
                SimpleBitStorage::from_values(bits, &values)
                    .map_err(container_error)?
                    .raw()
                    .iter()
                    .map(|&v| v as i64)
                    .collect(),
            ))
        };
        Ok(())
    }
}
impl ChunkSection {
    /// Port of SerializableChunkData#parse's section container decoding.
    /// Light-only sections return None; missing palettes within a populated section use defaults.
    pub fn to_runtime(&self, registry: &impl SectionRegistry) -> Result<Option<LevelChunkSection>> {
        if self.block_states.is_none() && self.biomes.is_none() {
            return Ok(None);
        }
        let biome_strategy = registry.biome_strategy();
        if biome_strategy.kind() != ContainerKind::Biomes
            || biome_strategy
                .registry()
                .id(registry.default_biome())
                .is_none()
        {
            return Err(Error::Invalid(
                "invalid runtime biome strategy/default".into(),
            ));
        }
        let blocks = match &self.block_states {
            Some(palette) => palette.unpack(Strategy::block_states(), |entry| {
                registry.block_state(entry)
            })?,
            None => PalettedContainer::new(Strategy::block_states(), Block::AIR.default_state()),
        };
        let biomes = match &self.biomes {
            Some(palette) => palette.unpack(biome_strategy, |name| registry.biome(name))?,
            None => PalettedContainer::new(biome_strategy, registry.default_biome()),
        };
        Ok(Some(LevelChunkSection::from_containers(blocks, biomes)))
    }
    /// Updates known palettes while preserving section fields, lights and original palette metadata.
    pub fn update_from_runtime(
        &mut self,
        section: &LevelChunkSection,
        registry: &impl SectionRegistry,
    ) -> Result<()> {
        if let Some(palette) = &mut self.block_states {
            palette.update(
                section.states(),
                |entry| registry.block_state(entry),
                |state| registry.block_state_entry(state),
            )?;
        } else {
            let packed = section.states().pack();
            self.block_states = Some(PackedPalette {
                palette: packed
                    .palette_entries
                    .into_iter()
                    .map(|state| registry.block_state_entry(state))
                    .collect::<Result<_>>()?,
                data: packed
                    .storage
                    .map(|data| LongArray(data.into_iter().map(|v| v as i64).collect())),
                extra: CompoundTag::new(),
            });
        }
        if let Some(palette) = &mut self.biomes {
            palette.update(
                section.biomes(),
                |name| registry.biome(name),
                |biome| registry.biome_name(biome),
            )?;
        } else {
            let packed = section.biomes().pack();
            self.biomes = Some(PackedPalette {
                palette: packed
                    .palette_entries
                    .into_iter()
                    .map(|id| registry.biome_name(id))
                    .collect::<Result<_>>()?,
                data: packed
                    .storage
                    .map(|data| LongArray(data.into_iter().map(|v| v as i64).collect())),
                extra: CompoundTag::new(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_chunk::IdMap;
    use ferraloom_nbt::Tag;
    use std::sync::Arc;
    struct Registry;
    impl SectionRegistry for Registry {
        fn block_state(&self, entry: &BlockStatePaletteEntry) -> Result<BlockState> {
            let name = entry
                .name
                .as_ref()
                .ok_or_else(|| Error::Invalid("missing block name".into()))?
                .to_string_checked()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            Block::from_name(&name)
                .map(Block::default_state)
                .ok_or_else(|| Error::Invalid("unknown block".into()))
        }
        fn block_state_entry(&self, state: BlockState) -> Result<BlockStatePaletteEntry> {
            Ok(BlockStatePaletteEntry {
                name: Some(state.block().name().into()),
                ..Default::default()
            })
        }
        fn biome(&self, name: &NbtString) -> Result<BiomeId> {
            match name.to_string_lossy().as_str() {
                "test:custom" => Ok(BiomeId(900)),
                "minecraft:plains" => Ok(BiomeId(7)),
                _ => Err(Error::Invalid("unknown biome".into())),
            }
        }
        fn biome_name(&self, biome: BiomeId) -> Result<NbtString> {
            match biome.0 {
                900 => Ok("test:custom".into()),
                7 => Ok("minecraft:plains".into()),
                _ => Err(Error::Invalid("unknown biome id".into())),
            }
        }
        fn biome_strategy(&self) -> Strategy<BiomeId> {
            Strategy::biomes(Arc::new(
                IdMap::new(vec![BiomeId(7), BiomeId(900)]).unwrap(),
            ))
        }
        fn default_biome(&self) -> BiomeId {
            BiomeId(7)
        }
    }
    #[test]
    fn runtime_sections_use_live_biomes_and_keep_unknown_palette_fields() {
        let tag=ferraloom_nbt::parse_compound(r#"{Y:0b,block_states:{palette:[{Name:"minecraft:air",extension:17}]},biomes:{palette:["test:custom"]},future:9}"#).unwrap();
        let mut section = ChunkSection::from_nbt(tag.clone()).unwrap();
        let mut runtime = section.to_runtime(&Registry).unwrap().unwrap();
        assert_eq!(runtime.biomes().get(0, 0, 0), BiomeId(900));
        section.update_from_runtime(&runtime, &Registry).unwrap();
        assert_eq!(section.to_nbt(), tag);
        runtime.set_block_state(1, 2, 3, Block::STONE.default_state());
        section.update_from_runtime(&runtime, &Registry).unwrap();
        assert_eq!(section.extra.get_int("future"), Some(9));
        assert_eq!(
            section.block_states.as_ref().unwrap().palette[0]
                .extra
                .get_int("extension"),
            Some(17)
        );
        let roundtrip = section.to_runtime(&Registry).unwrap().unwrap();
        assert_eq!(
            roundtrip.get_block_state(1, 2, 3),
            Block::STONE.default_state()
        );
        assert_eq!(
            roundtrip.get_block_state(0, 0, 0),
            Block::AIR.default_state()
        );
        // Corrupt local indices must produce an error before any chunk getter can panic.
        section
            .block_states
            .as_mut()
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .0[0] = i64::MAX;
        assert!(section.to_runtime(&Registry).is_err());
        assert_eq!(section.extra.get("future"), Some(&Tag::Int(9)));
    }
}
