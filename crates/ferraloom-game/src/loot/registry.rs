use super::{
    ContextFloatProvider, ContextIntProvider, Identifier, LootError, LootItemCondition,
    LootItemFunction, LootTable, Result, codec,
};
use ferraloom_data::datapack::{ResourceManager, load_tags};
use serde_json::Value;
use std::collections::BTreeMap;

/// Port of RegistryCodecs#holder. References stay symbolic across reload validation.
#[derive(Clone, Debug, PartialEq)]
pub enum Holder<T> {
    Reference(Identifier),
    Inline(Box<T>),
}
impl<T> Holder<T> {
    pub fn decode(v: &Value, decode: impl FnOnce(&Value) -> Result<T>) -> Result<Self> {
        if v.is_string() {
            if let Ok(id) = codec::id(v) {
                return Ok(Self::Reference(id));
            }
            Ok(Self::Inline(Box::new(decode(v)?)))
        } else {
            Ok(Self::Inline(Box::new(decode(v)?)))
        }
    }
    pub fn direct(value: T) -> Self {
        Self::Inline(Box::new(value))
    }
}
/// Port of RegistryCodecs#holderSet, preserving holder and tag iteration order.
#[derive(Clone, Debug, PartialEq)]
pub enum HolderSet<T> {
    Tag(Identifier),
    Direct(Vec<Holder<T>>),
}
impl<T> HolderSet<T> {
    pub fn decode(v: &Value, decode: impl Fn(&Value) -> Result<T> + Copy) -> Result<Self> {
        if let Some(tag) = v.as_str().and_then(|s| s.strip_prefix('#')) {
            return Ok(Self::Tag(Identifier::parse(tag)?));
        }
        if v.is_array() {
            return Ok(Self::Direct(codec::list(v, |v| Holder::decode(v, decode))?));
        }
        Ok(Self::Direct(vec![Holder::decode(v, decode)?]))
    }
}
/// Port of ReloadableServerRegistries' loot registries. Reload into a fresh
/// value, validate it, then publish it with the datapack snapshot.
#[derive(Clone, Debug, Default)]
pub struct LootData {
    pub tables: BTreeMap<Identifier, LootTable>,
    pub predicates: BTreeMap<Identifier, LootItemCondition>,
    pub modifiers: BTreeMap<Identifier, LootItemFunction>,
    pub int_providers: BTreeMap<Identifier, ContextIntProvider>,
    pub float_providers: BTreeMap<Identifier, ContextFloatProvider>,
    pub tags: BTreeMap<(&'static str, Identifier), Vec<Identifier>>,
}
pub(crate) trait RegistryValue: Sized {
    const REGISTRY: &'static str;
    fn registry(data: &LootData) -> &BTreeMap<Identifier, Self>;
}
macro_rules! registry {
    ($ty:ty,$field:ident,$name:literal) => {
        impl RegistryValue for $ty {
            const REGISTRY: &'static str = $name;
            fn registry(data: &LootData) -> &BTreeMap<Identifier, Self> {
                &data.$field
            }
        }
    };
}
registry!(LootTable, tables, "loot_table");
registry!(LootItemCondition, predicates, "predicate");
registry!(LootItemFunction, modifiers, "item_modifier");
registry!(ContextIntProvider, int_providers, "context_int_provider");
registry!(
    ContextFloatProvider,
    float_providers,
    "context_float_provider"
);
impl<T: RegistryValue> Holder<T> {
    pub(crate) fn resolve<'a>(&'a self, data: &'a LootData) -> Result<&'a T> {
        match self {
            Self::Inline(value) => Ok(value),
            Self::Reference(id) => {
                T::registry(data)
                    .get(id)
                    .ok_or_else(|| LootError::MissingReference {
                        registry: T::REGISTRY,
                        id: id.clone(),
                    })
            }
        }
    }
}
impl<T: RegistryValue> HolderSet<T> {
    pub(crate) fn resolve<'a>(&'a self, data: &'a LootData) -> Result<Vec<Holder<T>>>
    where
        T: Clone,
    {
        match self {
            Self::Direct(values) => Ok(values.clone()),
            Self::Tag(id) => data
                .tags
                .get(&(T::REGISTRY, id.clone()))
                .map(|ids| ids.iter().cloned().map(Holder::Reference).collect())
                .ok_or_else(|| LootError::MissingReference {
                    registry: T::REGISTRY,
                    id: id.clone(),
                }),
        }
    }
}
impl LootData {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let mut data = Self::default();
        macro_rules! load {
            ($field:ident,$ty:ty) => {{
                for (id, resource) in resources.registry(<$ty as RegistryValue>::REGISTRY) {
                    data.$field.insert(
                        Identifier::parse(&id.to_string())?,
                        <$ty>::decode(&resource.json()?)?,
                    );
                }
                let loaded = load_tags(resources, <$ty as RegistryValue>::REGISTRY, |id, _| {
                    Identifier::parse(&id.to_string())
                        .ok()
                        .filter(|id| data.$field.contains_key(id))
                });
                if !loaded.diagnostics.is_empty() {
                    return Err(codec::error(format!(
                        "invalid {} tags: {:?}",
                        <$ty as RegistryValue>::REGISTRY,
                        loaded.diagnostics
                    )));
                }
                for (id, values) in loaded.values {
                    data.tags.insert(
                        (
                            <$ty as RegistryValue>::REGISTRY,
                            Identifier::parse(&id.to_string())?,
                        ),
                        values,
                    );
                }
            }};
        }
        load!(tables, LootTable);
        load!(predicates, LootItemCondition);
        load!(modifiers, LootItemFunction);
        load!(int_providers, ContextIntProvider);
        load!(float_providers, ContextFloatProvider);
        Ok(data)
    }
}
