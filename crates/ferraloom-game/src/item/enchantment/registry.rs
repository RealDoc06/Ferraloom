//! Immutable enchantment registry loaded from the actual merged datapack.
use super::{EnchantmentDefinition, EnchantmentValueEffectComponent};
use crate::loot::{LootError, Result, codec};
use ferraloom_data::{
    datapack::ResourceManager, registries::enchantment_effect_component_type::Id as EffectComponent,
};
use ferraloom_protocol::{Identifier, chat::Component, codec::HolderSet, component::Persistent};
use serde_json::Value;
use std::collections::BTreeMap;

/// Other effect families retain their native payload in this same owner. Their
/// execution remains required until the corresponding typed effect port lands.
#[derive(Clone, Debug)]
pub enum EnchantmentEffect {
    Value(EnchantmentValueEffectComponent),
    Unported(Value),
}

/// Port of Enchantment's immutable definition and DataComponentMap.
#[derive(Clone, Debug)]
pub struct Enchantment {
    pub description: Component,
    pub definition: EnchantmentDefinition,
    pub exclusive_set: HolderSet,
    effects: BTreeMap<EffectComponent, EnchantmentEffect>,
}
impl Enchantment {
    fn decode(value: &Value) -> Result<Self> {
        let mut effects = BTreeMap::new();
        if let Some(value) = value.get("effects") {
            for (key, value) in value
                .as_object()
                .ok_or_else(|| codec::error("enchantment effects must be a map"))?
            {
                let key = Identifier::parse(key)?;
                let component = EffectComponent::from_name(key.as_str()).ok_or_else(|| {
                    codec::error(format!("unknown enchantment effect component {key}"))
                })?;
                let effect = match EnchantmentValueEffectComponent::decode(component, value)? {
                    Some(effect) => EnchantmentEffect::Value(effect),
                    None => EnchantmentEffect::Unported(value.clone()),
                };
                effects.insert(component, effect);
            }
        }
        Ok(Self {
            description: Component::from_json(&codec::field(value, "description")?.to_string())?,
            definition: EnchantmentDefinition::decode(value)?,
            exclusive_set: value
                .get("exclusive_set")
                .map(HolderSet::from_json)
                .transpose()?
                .unwrap_or_else(|| HolderSet::Direct(Vec::new())),
            effects,
        })
    }
    /// DataComponentMap#has, independent of whether that effect is executable.
    pub fn has_effect(&self, component: EffectComponent) -> bool {
        self.effects.contains_key(&component)
    }
    pub(crate) fn effect(&self, component: EffectComponent) -> Option<&EnchantmentEffect> {
        self.effects.get(&component)
    }
    pub fn effect_components(&self) -> impl Iterator<Item = EffectComponent> + '_ {
        self.effects.keys().copied()
    }
    pub fn value_effect(
        &self,
        component: EffectComponent,
    ) -> Result<Option<&EnchantmentValueEffectComponent>> {
        match self.effects.get(&component) {
            Some(EnchantmentEffect::Value(value)) => Ok(Some(value)),
            Some(EnchantmentEffect::Unported(_)) => Err(LootError::Unsupported(format!(
                "enchantment effect {}",
                component.name()
            ))),
            None => Ok(None),
        }
    }
}

/// The ResourceManager's merged key order is also RegistrySet#load's protocol
/// order. Values decode once here; hot callers retain this immutable owner.
#[derive(Debug)]
pub struct EnchantmentRegistry {
    entries: Vec<(Identifier, Enchantment)>,
    by_name: BTreeMap<Identifier, usize>,
}
impl EnchantmentRegistry {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let mut entries = Vec::new();
        let mut by_name = BTreeMap::new();
        for (key, resource) in resources.registry("enchantment") {
            let key: Identifier = key.to_string().parse()?;
            let enchantment = Enchantment::decode(&resource.json()?)
                .map_err(|error| codec::error(format!("{key}: {error}")))?;
            by_name.insert(key.clone(), entries.len());
            entries.push((key, enchantment));
        }
        Ok(Self { entries, by_name })
    }
    pub fn get(&self, key: &Identifier) -> Option<&Enchantment> {
        self.by_name.get(key).map(|&id| &self.entries[id].1)
    }
    pub fn by_id(&self, id: i32) -> Option<&Enchantment> {
        self.entries
            .get(usize::try_from(id).ok()?)
            .map(|(_, value)| value)
    }
    pub fn id(&self, key: &Identifier) -> Option<i32> {
        self.by_name.get(key).map(|&id| id as i32)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Identifier, &Enchantment)> {
        self.entries.iter().map(|(key, value)| (key, value))
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
