use super::{Identifier, Result, codec};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::random::{
    PositionalRandomFactory, Random, RandomSource, Seed128bit, XoroshiroRandomSource,
    seed_from_hash_of, upgrade_seed_to_128bit_unmixed,
};
use std::collections::BTreeMap;

/// Port of RandomSequences and RandomSequence. Gaussian cache is not persisted
/// by vanilla's codec. The owner persists this value as `random_sequences` data.
#[derive(Clone, Debug)]
pub struct RandomSequences {
    pub salt: i32,
    pub include_world_seed: bool,
    pub include_sequence_id: bool,
    sequences: BTreeMap<Identifier, XoroshiroRandomSource>,
    dirty: bool,
}
impl Default for RandomSequences {
    fn default() -> Self {
        Self {
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: BTreeMap::new(),
            dirty: false,
        }
    }
}
impl RandomSequences {
    pub fn create_sequence(
        key: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> XoroshiroRandomSource {
        let seed = if include_world_seed { world_seed } else { 0 } ^ i64::from(salt);
        let mut state = upgrade_seed_to_128bit_unmixed(seed);
        if include_sequence_id {
            state = state.xor(seed_from_hash_of(key.as_str()));
        }
        XoroshiroRandomSource::from_seed(state.mixed())
    }
    /// Port of RandomSequences#get. Merely obtaining a stream does not dirty it.
    pub fn get(&mut self, key: &Identifier, world_seed: i64) -> DirtyMarkingRandomSource<'_> {
        let source = self.sequences.entry(key.clone()).or_insert_with(|| {
            Self::create_sequence(
                key,
                world_seed,
                self.salt,
                self.include_world_seed,
                self.include_sequence_id,
            )
        });
        DirtyMarkingRandomSource {
            source,
            dirty: &mut self.dirty,
        }
    }
    /// Port of RandomSequences#setSeedDefaults, which leaves the dirty flag alone.
    pub fn set_seed_defaults(
        &mut self,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.salt = salt;
        self.include_world_seed = include_world_seed;
        self.include_sequence_id = include_sequence_id;
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
    pub fn clear(&mut self) -> usize {
        let count = self.sequences.len();
        self.sequences.clear();
        count
    }
    pub fn reset(
        &mut self,
        key: Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        let source = Self::create_sequence(
            &key,
            world_seed,
            salt,
            include_world_seed,
            include_sequence_id,
        );
        self.sequences.insert(key, source);
    }
    pub fn to_nbt(&self) -> Tag {
        let mut root = CompoundTag::new();
        root.insert("salt", Tag::Int(self.salt));
        if !self.include_world_seed {
            root.insert("include_world_seed", Tag::Byte(0));
        }
        if !self.include_sequence_id {
            root.insert("include_sequence_id", Tag::Byte(0));
        }
        let mut sequences = CompoundTag::new();
        for (key, source) in &self.sequences {
            let state = source.seed_state();
            let mut entry = CompoundTag::new();
            entry.insert("source", Tag::LongArray(vec![state.seed_lo, state.seed_hi]));
            sequences.insert(key.as_str(), Tag::Compound(entry));
        }
        root.insert("sequences", Tag::Compound(sequences));
        Tag::Compound(root)
    }
    pub fn from_nbt(tag: &Tag) -> Result<Self> {
        let root = tag
            .as_compound()
            .ok_or_else(|| codec::error("random_sequences compound"))?;
        let salt = i32::from_nbt(
            root.get("salt")
                .ok_or_else(|| codec::error("random_sequences salt"))?,
        )?;
        let sequences = root
            .get("sequences")
            .and_then(Tag::as_compound)
            .ok_or_else(|| codec::error("random_sequences map"))?;
        let mut result = Self {
            salt,
            include_world_seed: root
                .get("include_world_seed")
                .map(bool::from_nbt)
                .transpose()?
                .unwrap_or(true),
            include_sequence_id: root
                .get("include_sequence_id")
                .map(bool::from_nbt)
                .transpose()?
                .unwrap_or(true),
            ..Self::default()
        };
        for (key, entry) in sequences.iter() {
            let source = entry
                .as_compound()
                .and_then(|v| v.get("source"))
                .ok_or_else(|| codec::error("random sequence source"))?;
            let words = Vec::<i64>::from_nbt(source)?;
            if words.len() != 2 {
                return Err(codec::error("random sequence requires two words"));
            }
            result.sequences.insert(
                Identifier::parse(&key.to_string_lossy())?,
                XoroshiroRandomSource::from_words(words[0], words[1]),
            );
        }
        Ok(result)
    }
}

/// Port of RandomSequences.DirtyMarkingRandomSource. All draws delegate to the
/// shared RNG; default helpers call these methods so consume_count(0) stays clean.
pub struct DirtyMarkingRandomSource<'a> {
    source: &'a mut XoroshiroRandomSource,
    dirty: &'a mut bool,
}
impl DirtyMarkingRandomSource<'_> {
    pub fn seed_state(&self) -> Seed128bit {
        self.source.seed_state()
    }
}
impl RandomSource for DirtyMarkingRandomSource<'_> {
    fn set_seed(&mut self, seed: i64) {
        *self.dirty = true;
        self.source.set_seed(seed);
    }
    fn next_bits(&mut self, bits: u32) -> i32 {
        *self.dirty = true;
        self.source.next_bits(bits)
    }
    fn next_int(&mut self) -> i32 {
        *self.dirty = true;
        self.source.next_int()
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        *self.dirty = true;
        self.source.next_int_bound(bound)
    }
    fn next_long(&mut self) -> i64 {
        *self.dirty = true;
        self.source.next_long()
    }
    fn next_boolean(&mut self) -> bool {
        *self.dirty = true;
        self.source.next_boolean()
    }
    fn next_float(&mut self) -> f32 {
        *self.dirty = true;
        self.source.next_float()
    }
    fn next_double(&mut self) -> f64 {
        *self.dirty = true;
        self.source.next_double()
    }
    fn next_gaussian(&mut self) -> f64 {
        *self.dirty = true;
        self.source.next_gaussian()
    }
    fn fork(&mut self) -> Random {
        *self.dirty = true;
        self.source.fork()
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        *self.dirty = true;
        self.source.fork_positional()
    }
}
