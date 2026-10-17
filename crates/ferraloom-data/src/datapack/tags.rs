//! Registry-typed tag keys and immutable, resolved tag membership.
use super::{Error, Identifier, ResourceManager, TagDiagnostic, Tags, load_tags};
use std::{collections::BTreeMap, marker::PhantomData};

/// Registry ids used as bit positions. Dynamic registries supply their own ids.
pub trait TagEntry: Copy + 'static {
    const VANILLA_TAG_KEYS: &'static [TagKey<Self>] = &[];
    fn tag_index(self) -> usize;
}

/// Static registries can resolve tag entries without an external registry view.
pub trait StaticTagEntry: TagEntry + Eq {
    const REGISTRY: &'static str;
    const REGISTRY_SIZE: usize;
    fn from_tag_name(name: &Identifier) -> Option<Self>;
}

/// Port of TagKey. Generated constants live in `datapack::tags::BlockTags`, etc.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TagKey<T> {
    name: &'static str,
    ordinal: Option<usize>,
    marker: PhantomData<fn() -> T>,
}
impl<T> TagKey<T> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            ordinal: None,
            marker: PhantomData,
        }
    }
    const fn vanilla(name: &'static str, ordinal: usize) -> Self {
        Self {
            name,
            ordinal: Some(ordinal),
            marker: PhantomData,
        }
    }
    pub const fn name(self) -> &'static str {
        self.name
    }
    pub const fn ordinal(self) -> Option<usize> {
        self.ordinal
    }
}

/// Dense handle resolved once from a TagSet. Use it only with that tag set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TagId<T> {
    index: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> TagId<T> {
    pub const fn index(self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug)]
struct ResolvedTag<T> {
    name: Identifier,
    values: Box<[T]>,
    bits: Box<[u64]>,
}

/// Immutable TagLoader output with one bit per registry entry per tag. Values
/// retain vanilla's encounter order for consumers that iterate rather than test.
#[derive(Clone, Debug)]
pub struct TagSet<T> {
    names: BTreeMap<Identifier, TagId<T>>,
    tags: Vec<ResolvedTag<T>>,
    vanilla: Vec<Option<TagId<T>>>,
    pub diagnostics: Vec<TagDiagnostic>,
}
impl<T: TagEntry> TagSet<T> {
    pub fn from_tags(tags: Tags<T>, registry_size: usize) -> Result<Self, Error> {
        let mut names = BTreeMap::new();
        let mut resolved = Vec::with_capacity(tags.values.len());
        for (name, values) in tags.values {
            let mut bits = vec![0u64; registry_size.div_ceil(64)];
            for value in &values {
                let index = value.tag_index();
                if index >= registry_size {
                    return Err(Error::InvalidTag(format!(
                        "registry id {index} in {name} exceeds registry size {registry_size}"
                    )));
                }
                bits[index / 64] |= 1u64 << (index % 64);
            }
            names.insert(
                name.clone(),
                TagId {
                    index: resolved.len(),
                    marker: PhantomData,
                },
            );
            resolved.push(ResolvedTag {
                name,
                values: values.into_boxed_slice(),
                bits: bits.into_boxed_slice(),
            });
        }
        let vanilla = T::VANILLA_TAG_KEYS
            .iter()
            .map(|key| {
                key.name
                    .parse()
                    .ok()
                    .and_then(|name| names.get(&name).copied())
            })
            .collect();
        Ok(Self {
            names,
            tags: resolved,
            vanilla,
            diagnostics: tags.diagnostics,
        })
    }
    pub fn resolve(&self, key: TagKey<T>) -> Option<TagId<T>> {
        match key.ordinal {
            Some(ordinal) => self.vanilla[ordinal],
            None => self.resolve_name(&key.name.parse().ok()?),
        }
    }
    pub fn resolve_name(&self, name: &Identifier) -> Option<TagId<T>> {
        self.names.get(name).copied()
    }
    #[inline]
    pub fn contains(&self, tag: TagId<T>, value: T) -> bool {
        let index = value.tag_index();
        self.tags[tag.index]
            .bits
            .get(index / 64)
            .is_some_and(|word| word & (1u64 << (index % 64)) != 0)
    }
    /// Generated keys use a pre-resolved ordinal, with no name parsing or map
    /// lookup. Resolve custom keys once, then use `contains` with their TagId.
    #[inline]
    pub fn contains_key(&self, key: TagKey<T>, value: T) -> bool {
        let ordinal = key
            .ordinal
            .expect("resolve custom tag keys before testing membership");
        self.vanilla[ordinal].is_some_and(|tag| self.contains(tag, value))
    }
    pub fn values(&self, tag: TagId<T>) -> &[T] {
        &self.tags[tag.index].values
    }
    pub fn name(&self, tag: TagId<T>) -> &Identifier {
        &self.tags[tag.index].name
    }
    pub fn len(&self) -> usize {
        self.tags.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}
impl<T: StaticTagEntry> TagSet<T> {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        Self::from_tags(
            load_tags(resources, T::REGISTRY, |name, _| T::from_tag_name(name)),
            T::REGISTRY_SIZE,
        )
    }
}

include!(concat!(env!("OUT_DIR"), "/tag_keys.rs"));
