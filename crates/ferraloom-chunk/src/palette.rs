use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, OnceLock},
};

use ferraloom_data::BlockState;

use crate::{BitStorage, ContainerError};

/// Dense registry ID mapping, shared across all containers of a dimension.
#[derive(Debug)]
pub struct IdMap<T> {
    values: Vec<T>,
    ids: HashMap<T, u32>,
}

impl<T: Copy + Eq + Hash> IdMap<T> {
    pub fn new(values: Vec<T>) -> Result<Self, ContainerError> {
        if values.is_empty() {
            return Err(ContainerError::EmptyRegistry);
        }
        let ids: HashMap<_, _> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, i as u32))
            .collect();
        if ids.len() != values.len() {
            return Err(ContainerError::DuplicateRegistryValue);
        }
        Ok(Self { values, ids })
    }
    pub fn values(&self) -> &[T] {
        &self.values
    }
    pub fn id(&self, value: T) -> Option<u32> {
        self.ids.get(&value).copied()
    }
    pub fn value(&self, id: u32) -> Option<T> {
        self.values.get(id as usize).copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerKind {
    BlockStates,
    Biomes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteKind {
    SingleValue,
    Linear,
    HashMap,
    Global,
}

/// Port of Configuration.Simple / Configuration.Global. Global disk width is
/// distinct from memory width, even when both happen to have the same bit count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Configuration {
    pub palette: PaletteKind,
    pub bits_in_memory: u8,
    pub bits_in_storage: u8,
}

impl Configuration {
    pub fn always_repack(self) -> bool {
        self.palette == PaletteKind::Global
    }
}

/// Port of Strategy's two 26.3 configurations and Y/Z/X indexing.
#[derive(Clone, Debug)]
pub struct Strategy<T> {
    kind: ContainerKind,
    registry: Arc<IdMap<T>>,
}

fn required_bits(count: usize) -> u8 {
    if count <= 1 {
        0
    } else {
        (usize::BITS - (count - 1).leading_zeros()) as u8
    }
}

impl<T: Copy + Eq + Hash> Strategy<T> {
    pub fn new(kind: ContainerKind, registry: Arc<IdMap<T>>) -> Self {
        Self { kind, registry }
    }
    pub fn biomes(registry: Arc<IdMap<T>>) -> Self {
        Self::new(ContainerKind::Biomes, registry)
    }
    pub fn kind(&self) -> ContainerKind {
        self.kind
    }
    pub fn registry(&self) -> &IdMap<T> {
        &self.registry
    }
    pub fn entry_count(&self) -> usize {
        1 << (3 * self.bits_per_axis())
    }
    pub fn bits_per_axis(&self) -> u8 {
        match self.kind {
            ContainerKind::BlockStates => 4,
            ContainerKind::Biomes => 2,
        }
    }
    pub fn index(&self, x: u8, y: u8, z: u8) -> usize {
        let bits = self.bits_per_axis();
        assert!(x < 1 << bits && y < 1 << bits && z < 1 << bits);
        (usize::from(y) << (bits * 2)) | (usize::from(z) << bits) | usize::from(x)
    }
    pub fn configuration_for_bits(&self, bits: u8) -> Configuration {
        let (palette, memory) = match (self.kind, bits) {
            (_, 0) => (PaletteKind::SingleValue, 0),
            (ContainerKind::BlockStates, 1..=4) => (PaletteKind::Linear, 4),
            (ContainerKind::BlockStates, 5..=8) => (PaletteKind::HashMap, bits),
            (ContainerKind::Biomes, 1..=3) => (PaletteKind::Linear, bits),
            _ => (
                PaletteKind::Global,
                required_bits(self.registry.values.len()),
            ),
        };
        Configuration {
            palette,
            bits_in_memory: memory,
            bits_in_storage: if palette == PaletteKind::Global {
                bits
            } else {
                memory
            },
        }
    }
    pub fn configuration_for_palette_size(&self, count: usize) -> Configuration {
        self.configuration_for_bits(required_bits(count))
    }
}

impl Strategy<BlockState> {
    pub fn block_states() -> Self {
        // The generated block-state registry is process-wide, as in vanilla's
        // Block.BLOCK_STATE_REGISTRY. Sections share its immutable ID mapping.
        static REGISTRY: OnceLock<Arc<IdMap<BlockState>>> = OnceLock::new();
        let registry = REGISTRY.get_or_init(|| {
            let states = (0..ferraloom_data::BLOCK_STATES.len())
                .map(|i| BlockState::from_id(i as u16).expect("generated block state ID"))
                .collect();
            Arc::new(IdMap::new(states).expect("generated registry"))
        });
        Self::new(ContainerKind::BlockStates, registry.clone())
    }
}

#[derive(Clone, Debug)]
enum Palette<T> {
    Single(Option<T>),
    Linear(Vec<T>),
    Hash {
        entries: Vec<T>,
        ids: HashMap<T, u32>,
    },
    Global,
}

impl<T: Copy + Eq + Hash> Palette<T> {
    fn new(config: Configuration, entries: Vec<T>) -> Result<Self, ContainerError> {
        Ok(match config.palette {
            PaletteKind::SingleValue => {
                if entries.len() > 1 {
                    return Err(ContainerError::PaletteTooLarge);
                }
                Self::Single(entries.first().copied())
            }
            PaletteKind::Linear => {
                if entries.len() > 1 << config.bits_in_memory {
                    return Err(ContainerError::PaletteTooLarge);
                }
                Self::Linear(entries)
            }
            PaletteKind::HashMap => {
                let ids = entries
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i as u32))
                    .collect();
                Self::Hash { entries, ids }
            }
            PaletteKind::Global => Self::Global,
        })
    }
    fn entries(&self) -> &[T] {
        match self {
            Self::Single(v) => v.as_slice(),
            Self::Linear(v) | Self::Hash { entries: v, .. } => v,
            Self::Global => &[],
        }
    }
    fn value(&self, id: u32, registry: &IdMap<T>) -> T {
        match self {
            Self::Global => registry.value(id).expect("missing global palette entry"),
            _ => self.entries()[id as usize],
        }
    }
    fn id_for(&mut self, value: T, config: Configuration, registry: &IdMap<T>) -> Option<u32> {
        match self {
            Self::Single(v) => match v {
                Some(old) if *old != value => None,
                _ => {
                    *v = Some(value);
                    Some(0)
                }
            },
            Self::Linear(entries) => {
                if let Some(i) = entries.iter().position(|&v| v == value) {
                    return Some(i as u32);
                }
                if entries.len() == 1 << config.bits_in_memory {
                    return None;
                }
                let id = entries.len() as u32;
                entries.push(value);
                Some(id)
            }
            Self::Hash { entries, ids } => {
                if let Some(&id) = ids.get(&value) {
                    return Some(id);
                }
                let id = entries.len() as u32;
                entries.push(value);
                ids.insert(value, id);
                (id < 1 << config.bits_in_memory).then_some(id)
            }
            Self::Global => Some(registry.id(value).unwrap_or(0)),
        }
    }
}

/// Port of PalettedContainerRO.PackedData. None is vanilla's unknown (-1) bit count.
#[derive(Clone, Debug)]
pub struct PackedData<T> {
    pub palette_entries: Vec<T>,
    pub storage: Option<Vec<u64>>,
    pub bits_per_entry: Option<u8>,
}

/// Port of PalettedContainer. Cloning copies mutable data and shares only registry facts.
#[derive(Clone, Debug)]
pub struct PalettedContainer<T> {
    strategy: Strategy<T>,
    config: Configuration,
    palette: Palette<T>,
    storage: BitStorage,
}

impl<T: Copy + Eq + Hash> PalettedContainer<T> {
    pub fn new(strategy: Strategy<T>, initial: T) -> Self {
        Self {
            config: strategy.configuration_for_bits(0),
            palette: Palette::Single(Some(initial)),
            storage: BitStorage::new(0, strategy.entry_count()).expect("zero storage"),
            strategy,
        }
    }
    pub fn strategy(&self) -> &Strategy<T> {
        &self.strategy
    }
    pub fn configuration(&self) -> Configuration {
        self.config
    }
    pub fn palette_kind(&self) -> PaletteKind {
        self.config.palette
    }
    /// Local palette entries in insertion order. Global wire palettes have no entries.
    pub fn palette_entries(&self) -> &[T] {
        self.palette.entries()
    }
    pub fn bits_per_entry(&self) -> u8 {
        self.storage.bits()
    }
    pub fn raw(&self) -> &[u64] {
        self.storage.raw()
    }
    pub fn get(&self, x: u8, y: u8, z: u8) -> T {
        self.get_by_index(self.strategy.index(x, y, z))
    }
    pub fn get_by_index(&self, index: usize) -> T {
        self.palette
            .value(self.storage.get(index), &self.strategy.registry)
    }
    pub fn set(&mut self, x: u8, y: u8, z: u8, value: T) -> T {
        self.set_by_index(self.strategy.index(x, y, z), value)
    }
    pub fn set_by_index(&mut self, index: usize, value: T) -> T {
        assert!(index < self.strategy.entry_count());
        let id = self
            .palette
            .id_for(value, self.config, &self.strategy.registry)
            .unwrap_or_else(|| self.resize(self.config.bits_in_memory + 1, value));
        let old = self.storage.get_and_set(index, id);
        self.palette.value(old, &self.strategy.registry)
    }
    fn resize(&mut self, bits: u8, last_value: T) -> u32 {
        let config = self.strategy.configuration_for_bits(bits);
        let mut palette = Palette::new(config, Vec::new()).expect("empty palette configuration");
        let mut storage = BitStorage::new(config.bits_in_memory, self.strategy.entry_count())
            .expect("registry bit width");
        if self.palette.entries().len() == 1 {
            let id = palette
                .id_for(self.palette.entries()[0], config, &self.strategy.registry)
                .expect("resized capacity");
            storage.fill(id);
        } else {
            for index in 0..self.strategy.entry_count() {
                let id = palette
                    .id_for(self.get_by_index(index), config, &self.strategy.registry)
                    .expect("resized capacity");
                storage.get_and_set(index, id);
            }
        }
        self.palette = palette;
        self.storage = storage;
        self.config = config;
        self.palette
            .id_for(last_value, config, &self.strategy.registry)
            .expect("resized capacity")
    }
    /// Import an in-memory palette view, for protocol/storage adapters without codecs.
    pub fn from_raw_parts(
        strategy: Strategy<T>,
        bits: u8,
        entries: Vec<T>,
        raw: Vec<u64>,
    ) -> Result<Self, ContainerError> {
        let config = strategy.configuration_for_bits(bits);
        let palette = Palette::new(config, entries)?;
        if config.palette == PaletteKind::SingleValue && palette.entries().is_empty() {
            return Err(ContainerError::EmptyPalette);
        }
        let storage = BitStorage::from_raw(config.bits_in_memory, strategy.entry_count(), raw)?;
        Ok(Self {
            strategy,
            config,
            palette,
            storage,
        })
    }
    pub fn recreate(&self) -> Self {
        Self::new(
            self.strategy.clone(),
            self.palette.value(0, &self.strategy.registry),
        )
    }
    pub fn maybe_has(&self, mut predicate: impl FnMut(T) -> bool) -> bool {
        self.config.palette == PaletteKind::Global
            || self.palette.entries().iter().copied().any(&mut predicate)
    }
    /// Counts used values; ordering is not a palette compaction operation.
    pub fn counts(&self) -> Vec<(T, usize)> {
        // PalettedContainer#count reports a size-one palette without walking
        // its storage, including a freshly allocated uniform section.
        if let [value] = self.palette.entries() {
            return vec![(*value, self.strategy.entry_count())];
        }
        let mut counts = HashMap::new();
        for index in 0..self.strategy.entry_count() {
            *counts.entry(self.storage.get(index)).or_insert(0) += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_unstable_by_key(|&(id, _)| id);
        counts
            .into_iter()
            .map(|(id, count)| (self.palette.value(id, &self.strategy.registry), count))
            .collect()
    }
    /// Port of pack: discard unused entries, assign IDs in first cell-use order,
    /// then use disk width even for a currently global in-memory palette.
    pub fn pack(&self) -> PackedData<T> {
        // Reencoding a size-one palette always yields ID 0 with zero disk
        // bits. Avoid allocating and hashing an identical ID for every cell.
        if let [value] = self.palette.entries() {
            return PackedData {
                palette_entries: vec![*value],
                storage: None,
                bits_per_entry: Some(0),
            };
        }
        let mut entries = Vec::new();
        let mut ids = HashMap::new();
        let contents: Vec<_> = (0..self.strategy.entry_count())
            .map(|index| {
                let value = self.get_by_index(index);
                *ids.entry(value).or_insert_with(|| {
                    let id = entries.len() as u32;
                    entries.push(value);
                    id
                })
            })
            .collect();
        let bits = self
            .strategy
            .configuration_for_palette_size(entries.len())
            .bits_in_storage;
        let storage = if bits == 0 {
            None
        } else {
            Some(
                crate::SimpleBitStorage::from_values(bits, &contents)
                    .expect("packed width")
                    .raw()
                    .to_vec(),
            )
        };
        PackedData {
            palette_entries: entries,
            storage,
            bits_per_entry: Some(bits),
        }
    }
    /// Port of unpack, including mandatory disk-local to global-ID reencoding.
    pub fn unpack(strategy: Strategy<T>, packed: PackedData<T>) -> Result<Self, ContainerError> {
        if packed.palette_entries.is_empty() {
            return Err(ContainerError::EmptyPalette);
        }
        let config = strategy.configuration_for_palette_size(packed.palette_entries.len());
        if let Some(actual) = packed.bits_per_entry
            && actual != config.bits_in_storage
        {
            return Err(ContainerError::DeclaredBits {
                expected: config.bits_in_storage,
                actual,
            });
        }
        if config.bits_in_memory == 0 {
            return Ok(Self::new(strategy, packed.palette_entries[0]));
        }
        let old = BitStorage::from_raw(
            config.bits_in_storage,
            strategy.entry_count(),
            packed.storage.ok_or(ContainerError::MissingStorage)?,
        )?;
        let palette = Palette::new(config, packed.palette_entries.clone())?;
        if !config.always_repack() && config.bits_in_memory == config.bits_in_storage {
            return Ok(Self {
                strategy,
                config,
                palette,
                storage: old,
            });
        }
        let mut storage = BitStorage::new(config.bits_in_memory, strategy.entry_count())?;
        for index in 0..strategy.entry_count() {
            let value = packed.palette_entries[old.get(index) as usize];
            storage.get_and_set(index, strategy.registry.id(value).unwrap_or(0));
        }
        Ok(Self {
            strategy,
            config,
            palette,
            storage,
        })
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn block_state_strategies_share_the_canonical_generated_registry() {
        let first = Strategy::block_states();
        let second = Strategy::block_states();
        assert!(Arc::ptr_eq(&first.registry, &second.registry));
        assert_eq!(
            first.registry.values().len(),
            ferraloom_data::BLOCK_STATES.len()
        );
        for (id, &state) in first.registry.values().iter().enumerate() {
            assert_eq!(first.registry.id(state), Some(id as u32));
            assert_eq!(state, BlockState::from_id(id as u16).unwrap());
        }
    }
}
