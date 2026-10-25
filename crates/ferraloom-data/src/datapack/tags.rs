//! Registry-typed tag keys and immutable, resolved tag membership.
use super::{Error, Identifier, ResourceManager, TagDiagnostic, Tags, load_tags};
use std::{collections::BTreeMap, marker::PhantomData};

/// Registry ids used as bit positions. Dynamic registries supply their own ids.
pub trait TagElement: Copy + 'static {
    const VANILLA_TAG_KEYS: &'static [TagKey<Self>] = &[];
    fn tag_index(self) -> usize;
}

/// Static registries can resolve tag entries without an external registry view.
pub trait StaticTagElement: TagElement + Eq {
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
impl<T: TagElement> TagSet<T> {
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
impl<T: StaticTagElement> TagSet<T> {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        Self::from_tags(
            load_tags(resources, T::REGISTRY, |name, _| T::from_tag_name(name)),
            T::REGISTRY_SIZE,
        )
    }
}

include!(concat!(env!("OUT_DIR"), "/tag_keys.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::Pack;
    use serde_json::{Map, Value};
    use std::collections::BTreeSet;

    fn resources(trade_rebalance: bool) -> ResourceManager {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let vanilla = Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap();
        if !trade_rebalance {
            return ResourceManager::new(vec![vanilla]);
        }
        let base = ResourceManager::new(vec![vanilla.clone()]);
        let mut trade = Pack::new("trade_rebalance");
        let prefix = "datapacks/trade_rebalance/data/";
        let entries = base.list(prefix, ".json");
        assert!(!entries.is_empty(), "optional vanilla pack must be present");
        for name in entries {
            let (namespace, path) = name
                .path()
                .strip_prefix(prefix)
                .unwrap()
                .split_once('/')
                .unwrap();
            trade.insert(
                Identifier::new(namespace, path).unwrap(),
                base.require(&name).unwrap().bytes.to_vec(),
            );
        }
        ResourceManager::new(vec![vanilla, trade])
    }
    fn compare<T: TagElement + Eq + std::fmt::Debug>(
        actual: &TagSet<T>,
        expected: &Map<String, Value>,
        registry: &str,
        names: &BTreeMap<Identifier, T>,
    ) {
        assert!(
            actual.diagnostics.is_empty(),
            "{registry}: {:?}",
            actual.diagnostics
        );
        assert_eq!(actual.len(), expected.len(), "{registry} tag count");
        for (name, values) in expected {
            let id = actual.resolve_name(&name.parse().unwrap()).unwrap();
            assert_eq!(actual.name(id).to_string(), *name);
            let members = values.as_array().unwrap();
            let members = members
                .iter()
                .map(|value| {
                    let name = value.as_str().unwrap().parse::<Identifier>().unwrap();
                    *names
                        .get(&name)
                        .unwrap_or_else(|| panic!("{registry} tag {name}: missing element"))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual.values(id),
                members,
                "{registry} #{name}: TagLoader encounter order"
            );
            for (element_name, &element) in names {
                assert_eq!(
                    actual.contains(id, element),
                    members.contains(&element),
                    "{registry} #{name}: {element_name}"
                );
            }
        }
    }
    fn compare_static<T: StaticTagElement + std::fmt::Debug>(
        resources: &ResourceManager,
        expected: &Map<String, Value>,
    ) {
        let names = crate::registry_entries(&format!("minecraft:{}", T::REGISTRY))
            .unwrap()
            .iter()
            .map(|name| {
                let name = name.parse::<Identifier>().unwrap();
                let entry = T::from_tag_name(&name).unwrap();
                (name, entry)
            })
            .collect();
        compare(
            &TagSet::<T>::load(resources).unwrap(),
            expected,
            T::REGISTRY,
            &names,
        );
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct DynamicEntry(usize);
    impl TagElement for DynamicEntry {
        fn tag_index(self) -> usize {
            self.0
        }
    }
    fn compare_dynamic(resources: &ResourceManager, registry: &str, expected: &Map<String, Value>) {
        // Production dynamic registries supply their own lookup and numeric IDs.
        // Derive this fixture's lookup from the actual jar resources, never from
        // the expected tag members, and pass it through the same TagSet builder.
        let names: BTreeMap<_, _> = resources
            .registry(registry)
            .into_keys()
            .enumerate()
            .map(|(index, name)| (name, DynamicEntry(index)))
            .collect();
        assert!(!names.is_empty(), "missing live {registry} resources");
        let loaded = load_tags(resources, registry, |name, _| names.get(name).copied());
        let actual = TagSet::from_tags(loaded, names.len()).unwrap();
        compare(&actual, expected, registry, &names);
    }
    #[test]
    fn resolved_tags_match_vanilla_tagloader_order_including_trade_rebalance() {
        let native: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registry_parity/tags.json"
        ))
        .unwrap();
        for (profile, trade) in [("vanilla", false), ("trade_rebalance", true)] {
            let resources = resources(trade);
            let expected = native[profile].as_object().unwrap();
            // Verify every tag resource is covered, including extra registries
            // or empty tags; comparing only a known subset could hide omissions.
            let expected_files: BTreeSet<_> = expected
                .iter()
                .flat_map(|(registry, tags)| {
                    let registry = registry.strip_prefix("minecraft:").unwrap();
                    tags.as_object().unwrap().keys().map(move |name| {
                        let name = name.parse::<Identifier>().unwrap();
                        Identifier::new(
                            name.namespace(),
                            format!("tags/{registry}/{}.json", name.path()),
                        )
                        .unwrap()
                    })
                })
                .collect();
            assert_eq!(
                resources.list("tags/", ".json"),
                expected_files,
                "{profile} resource coverage"
            );
            for (registry, tags) in expected {
                let expected = tags.as_object().unwrap();
                match registry.as_str() {
                    "minecraft:block" => compare_static::<crate::Block>(&resources, expected),
                    "minecraft:item" => compare_static::<crate::Item>(&resources, expected),
                    "minecraft:entity_type" => {
                        compare_static::<crate::EntityType>(&resources, expected)
                    }
                    "minecraft:fluid" => {
                        compare_static::<crate::registries::fluid::Id>(&resources, expected)
                    }
                    "minecraft:game_event" => {
                        compare_static::<crate::registries::game_event::Id>(&resources, expected)
                    }
                    "minecraft:potion" => {
                        compare_static::<crate::registries::potion::Id>(&resources, expected)
                    }
                    "minecraft:point_of_interest_type" => {
                        compare_static::<crate::registries::point_of_interest_type::Id>(
                            &resources, expected,
                        )
                    }
                    _ => compare_dynamic(
                        &resources,
                        registry.strip_prefix("minecraft:").unwrap(),
                        expected,
                    ),
                }
            }
        }
    }
}
