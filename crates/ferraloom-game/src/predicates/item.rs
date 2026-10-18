use super::{PredicateContext, PredicateRegistry};
use crate::loot::{Identifier, LootError, Result};
use ferraloom_data::Item;
use ferraloom_nbt::compare_nbt;
use ferraloom_protocol::{codec::HolderSet, component::*};
use serde_json::Value;

/// Port of ItemPredicate. The persistent schema is shared with container lock predicates.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemPredicate(pub LockCode);
impl ItemPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        // Persistent codecs supply the component schema; bounds need the matching
        // decode-time swapped-endpoint validation (rather than encode-time only).
        if let Some(count) = v.get("count") {
            super::IntBounds::decode(count)?;
        }
        Ok(Self(LockCode::from_json(v)?))
    }
    pub fn matches(&self, stack: &ItemStack, facts: &dyn PredicateContext) -> Result<bool> {
        let item = stack
            .0
            .as_ref()
            .filter(|_| !stack.is_empty())
            .map_or(Item::AIR, |s| s.item);
        let count = stack
            .0
            .as_ref()
            .filter(|_| !stack.is_empty())
            .map_or(0, |s| s.count);
        if let Some(set) = &self.0.items {
            let matches =
                match &set.0 {
                    HolderSet::Named(id) => {
                        let tags = facts.item_tags()?;
                        let key = id.to_string().parse().map_err(
                            |e: ferraloom_data::datapack::Error| LootError::Decode(e.to_string()),
                        )?;
                        let tag =
                            tags.resolve_name(&key)
                                .ok_or_else(|| LootError::MissingReference {
                                    registry: "item tag",
                                    id: id.clone(),
                                })?;
                        tags.contains(tag, item)
                    }
                    HolderSet::Direct(ids) => ids.iter().any(|id| id.as_str() == item.name()),
                };
            if !matches {
                return Ok(false);
            }
        }
        if !bounds(&self.0.count, count) {
            return Ok(false);
        }
        match_components(&self.0.components, &stack.components()?, facts)
    }
}
fn bounds(range: &ferraloom_protocol::component::IntBounds, value: i32) -> bool {
    range.min.is_none_or(|min| value >= min) && range.max.is_none_or(|max| value <= max)
}
/// Port of DataComponentMatchers#test. Exact predicates use the effective map.
pub fn match_components(
    test: &DataComponentMatchers,
    actual: &DataComponentMap,
    facts: &dyn PredicateContext,
) -> Result<bool> {
    for expected in &test.exact {
        if actual.get(expected.component_type()) != Some(expected) {
            return Ok(false);
        }
    }
    for predicate in &test.partial.0 {
        if !match_partial(predicate, actual, facts)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn int_component(map: &DataComponentMap, id: ComponentTypeId, fallback: i32) -> i32 {
    match map.get(id) {
        Some(DataComponent::Damage(v) | DataComponent::MaxDamage(v)) => v.0,
        _ => fallback,
    }
}
pub(crate) fn matches_set(
    set: &HolderSet,
    id: &Identifier,
    registry: PredicateRegistry,
    facts: &dyn PredicateContext,
) -> Result<bool> {
    match set {
        HolderSet::Direct(ids) => Ok(ids.contains(id)),
        HolderSet::Named(tag) => facts.tag_contains(registry, tag, id),
    }
}
fn match_partial(
    test: &ComponentPredicate,
    map: &DataComponentMap,
    facts: &dyn PredicateContext,
) -> Result<bool> {
    Ok(match test {
        ComponentPredicate::AnyValue(id) => map.get(*id).is_some(),
        ComponentPredicate::Damage(p) => {
            let max = int_component(map, ComponentTypeId::MAX_DAMAGE, 0);
            let damage = int_component(map, ComponentTypeId::DAMAGE, 0);
            max > 0 && bounds(&p.durability, max.wrapping_sub(damage)) && bounds(&p.damage, damage)
        }
        ComponentPredicate::CustomData(p) => match map.get(ComponentTypeId::CUSTOM_DATA) {
            Some(DataComponent::CustomData(data)) => compare_nbt(Some(&p.0.0), Some(&data.0), true),
            _ => false,
        },
        // Evaluate these through their typed representations as the gameplay
        // component semantics are ported. Unknown semantics never match by guess.
        p => {
            return Err(LootError::Unsupported(format!(
                "component_predicate.{}",
                p.key()
            )));
        }
    })
}
