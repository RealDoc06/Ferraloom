use super::{Identifier, Result, codec};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::random::{
    XoroshiroRandomSource, seed_from_hash_of, upgrade_seed_to_128bit_unmixed,
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
    /// Conservatively mark dirty when lending mutable access, before any draw.
    pub fn get(&mut self, key: &Identifier, world_seed: i64) -> &mut XoroshiroRandomSource {
        self.dirty = true;
        self.sequences.entry(key.clone()).or_insert_with(|| {
            Self::create_sequence(
                key,
                world_seed,
                self.salt,
                self.include_world_seed,
                self.include_sequence_id,
            )
        })
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
        self.dirty = true;
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
        self.dirty = true;
    }
    pub fn to_nbt(&self) -> Tag {
        let mut root = CompoundTag::new();
        root.insert("salt", Tag::Int(self.salt));
        root.insert(
            "include_world_seed",
            Tag::Byte(i8::from(self.include_world_seed)),
        );
        root.insert(
            "include_sequence_id",
            Tag::Byte(i8::from(self.include_sequence_id)),
        );
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
        let salt = root
            .get("salt")
            .and_then(Tag::as_int)
            .ok_or_else(|| codec::error("random_sequences salt"))?;
        let sequences = root
            .get("sequences")
            .and_then(Tag::as_compound)
            .ok_or_else(|| codec::error("random_sequences map"))?;
        let mut result = Self {
            salt,
            include_world_seed: root
                .get("include_world_seed")
                .and_then(Tag::as_int)
                .is_none_or(|v| v != 0),
            include_sequence_id: root
                .get("include_sequence_id")
                .and_then(Tag::as_int)
                .is_none_or(|v| v != 0),
            ..Self::default()
        };
        for (key, entry) in sequences.iter() {
            let words = entry
                .as_compound()
                .and_then(|v| v.get("source"))
                .and_then(Tag::as_long_array)
                .ok_or_else(|| codec::error("random sequence source"))?;
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
