use super::{PredicateContext, PredicateRegistry};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::loot::{Identifier, LootError, Result};
use ferraloom_data::Item;
use ferraloom_nbt::compare_nbt;
use ferraloom_protocol::{codec::HolderSet, component::*};

/// Port of ItemPredicate. The persistent schema is shared with container lock predicates.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemPredicate(pub LockCode);
impl ItemPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        // Persistent codecs supply the component schema; bounds need the matching
        // decode-time swapped-endpoint validation (rather than encode-time only).
        if let Some(count) = v.get("count") {
            super::IntBounds::decode(count)?;
        }
        let value = LockCode::from_input(v)?;
        if let Some(set) = &value.items {
            super::validate_static_holders(&set.0, "item", |id| Item::from_name(id).is_some())?;
        }
        value.to_persistent()?;
        Ok(Self(value))
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
            map.get(ComponentTypeId::DAMAGE).is_some()
                && bounds(&p.durability, max.wrapping_sub(damage))
                && bounds(&p.damage, damage)
        }
        ComponentPredicate::CustomData(p) => match map.get(ComponentTypeId::CUSTOM_DATA) {
            Some(DataComponent::CustomData(data)) => compare_nbt(Some(&p.0.0), Some(&data.0), true),
            _ => false,
        },
        ComponentPredicate::Enchantments(tests) | ComponentPredicate::StoredEnchantments(tests) => {
            let id = if matches!(test, ComponentPredicate::Enchantments(_)) {
                ComponentTypeId::ENCHANTMENTS
            } else {
                ComponentTypeId::STORED_ENCHANTMENTS
            };
            let Some(DataComponent::Enchantments(value) | DataComponent::StoredEnchantments(value)) =
                map.get(id)
            else {
                return Ok(false);
            };
            for test in tests {
                let mut found = false;
                for (enchantment, level) in &value.0 {
                    if level.0 == 0 || !bounds(&test.levels, level.0) {
                        continue;
                    }
                    if let Some(set) = &test.enchantments
                        && !matches_set(set, &enchantment.0, PredicateRegistry::Enchantment, facts)?
                    {
                        continue;
                    }
                    found = true;
                    break;
                }
                if !found {
                    return Ok(false);
                }
            }
            true
        }
        ComponentPredicate::Container(test) => {
            let Some(DataComponent::Container(value)) = map.get(ComponentTypeId::CONTAINER) else {
                return Ok(false);
            };
            let items = value
                .0
                .iter()
                .flatten()
                .filter(|s| s.count > 0)
                .collect::<Vec<_>>();
            match &test.items {
                None => true,
                Some(test) => match_collection(test, &items, |test, value| {
                    ItemPredicate(test.clone()).matches(
                        &ItemStack::new(value.item, value.count, value.components.clone()),
                        facts,
                    )
                })?,
            }
        }
        ComponentPredicate::BundleContents(test) => {
            let Some(DataComponent::BundleContents(value)) =
                map.get(ComponentTypeId::BUNDLE_CONTENTS)
            else {
                return Ok(false);
            };
            match &test.items {
                None => true,
                Some(test) => match_collection(test, value, |test, value| {
                    ItemPredicate(test.clone()).matches(
                        &ItemStack::new(value.item, value.count, value.components.clone()),
                        facts,
                    )
                })?,
            }
        }
        ComponentPredicate::FireworkExplosion(test) => {
            let Some(DataComponent::FireworkExplosion(value)) =
                map.get(ComponentTypeId::FIREWORK_EXPLOSION)
            else {
                return Ok(false);
            };
            firework_matches(test, value)
        }
        ComponentPredicate::Fireworks(test) => {
            let Some(DataComponent::Fireworks(value)) = map.get(ComponentTypeId::FIREWORKS) else {
                return Ok(false);
            };
            bounds(&test.flight_duration, value.flight_duration.0)
                && match &test.explosions {
                    None => true,
                    Some(test) => match_collection(test, &value.explosions.0, |p, v| {
                        Ok(firework_matches(p, v))
                    })?,
                }
        }
        ComponentPredicate::WritableBook(test) => {
            let Some(DataComponent::WritableBookContent(value)) =
                map.get(ComponentTypeId::WRITABLE_BOOK_CONTENT)
            else {
                return Ok(false);
            };
            match &test.pages {
                None => true,
                Some(test) => match_collection(test, &value.0, |p, v| Ok(*p == v.raw.0))?,
            }
        }
        ComponentPredicate::WrittenBook(test) => {
            let Some(DataComponent::WrittenBookContent(value)) =
                map.get(ComponentTypeId::WRITTEN_BOOK_CONTENT)
            else {
                return Ok(false);
            };
            test.author.as_ref().is_none_or(|p| *p == value.author)
                && test.title.as_ref().is_none_or(|p| *p == value.title.raw.0)
                && bounds(&test.generation, value.generation.0)
                && test.resolved.is_none_or(|p| p == value.resolved)
                && match &test.pages {
                    None => true,
                    Some(test) => match_collection(test, &value.pages, |p, v| Ok(*p == v.raw))?,
                }
        }
        ComponentPredicate::VillagerType(set) => {
            let Some(DataComponent::VillagerVariant(value)) =
                map.get(ComponentTypeId::VILLAGER_VARIANT)
            else {
                return Ok(false);
            };
            matches_set(set, &value.0, PredicateRegistry::VillagerType, facts)?
        }
        ComponentPredicate::JukeboxPlayable(test) => {
            let Some(DataComponent::JukeboxPlayable(value)) =
                map.get(ComponentTypeId::JUKEBOX_PLAYABLE)
            else {
                return Ok(false);
            };
            match &test.song {
                None => true,
                Some(set) => match &value.0 {
                    ferraloom_protocol::codec::Holder::Reference(id) => {
                        matches_set(set, id, PredicateRegistry::JukeboxSong, facts)?
                    }
                    _ => false,
                },
            }
        }
        ComponentPredicate::Trim(test) => {
            let Some(DataComponent::Trim(value)) = map.get(ComponentTypeId::TRIM) else {
                return Ok(false);
            };
            let material = match &test.material {
                None => true,
                Some(set) => match &value.material.0 {
                    ferraloom_protocol::codec::Holder::Reference(id) => {
                        matches_set(set, id, PredicateRegistry::TrimMaterial, facts)?
                    }
                    _ => false,
                },
            };
            material
                && match &test.pattern {
                    None => true,
                    Some(set) => match &value.pattern.0 {
                        ferraloom_protocol::codec::Holder::Reference(id) => {
                            matches_set(set, id, PredicateRegistry::TrimPattern, facts)?
                        }
                        _ => false,
                    },
                }
        }
        ComponentPredicate::AttributeModifiers(test) => {
            let Some(DataComponent::AttributeModifiers(value)) =
                map.get(ComponentTypeId::ATTRIBUTE_MODIFIERS)
            else {
                return Ok(false);
            };
            match &test.modifiers {
                None => true,
                Some(test) => match_collection(test, value, |p, v| {
                    if let Some(set) = &p.attribute
                        && !matches_set(set, &v.attribute.0, PredicateRegistry::Attribute, facts)?
                    {
                        return Ok(false);
                    }
                    Ok(p.id.as_ref().is_none_or(|id| *id == v.modifier.id)
                        && super::DoubleBounds {
                            min: p.amount.min.map(|v| v.0),
                            max: p.amount.max.map(|v| v.0),
                        }
                        .matches(v.modifier.amount.0)
                        && p.operation.is_none_or(|op| op == v.modifier.operation)
                        && p.slot.is_none_or(|slot| slot == v.slot))
                })?,
            }
        }
        ComponentPredicate::Potions(test) => {
            let Some(DataComponent::PotionContents(value)) =
                map.get(ComponentTypeId::POTION_CONTENTS)
            else {
                return Ok(false);
            };
            if let Some(set) = &test.potions {
                let Some(potion) = &value.potion else {
                    return Ok(false);
                };
                if !matches_set(set, &potion.0, PredicateRegistry::Potion, facts)? {
                    return Ok(false);
                }
            }
            if let Some(test) = &test.effects {
                let mut effects = if let Some(potion) = &value.potion {
                    facts.potion_effects(&potion.0)?
                } else {
                    Vec::new()
                };
                effects.extend(value.custom_effects.clone());
                match_collection(test, &effects, |test, value| {
                    let Some((_, test)) = test.0.iter().find(|(effect, _)| *effect == value.effect)
                    else {
                        return Ok(false);
                    };
                    let v = &value.details;
                    if !bounds(&test.amplifier, v.amplifier.0)
                        || !bounds(&test.duration, v.duration.0)
                        || test.ambient.is_some_and(|a| a != v.ambient)
                        || test.visible.is_some_and(|a| a != v.show_particles)
                    {
                        return Ok(false);
                    }
                    Ok(true)
                })?
            } else {
                true
            }
        }
    })
}

fn firework_matches(test: &FireworkPredicate, value: &FireworkExplosion) -> bool {
    test.shape.is_none_or(|v| v == value.shape)
        && test.has_twinkle.is_none_or(|v| v == value.has_twinkle)
        && test.has_trail.is_none_or(|v| v == value.has_trail)
}
/// Port of CollectionPredicate, including reuse of a value for several contains tests.
fn match_collection<P, V>(
    test: &CollectionPredicate<P>,
    values: &[V],
    matches: impl Fn(&P, &V) -> Result<bool>,
) -> Result<bool> {
    if let Some(tests) = &test.contains {
        for test in tests {
            let mut found = false;
            for value in values {
                if matches(test, value)? {
                    found = true;
                    break;
                }
            }
            if !found {
                return Ok(false);
            }
        }
    }
    if let Some(tests) = &test.count {
        for test in tests {
            let mut count = 0;
            for value in values {
                if matches(&test.test, value)? {
                    count += 1;
                }
            }
            if !bounds(&test.count, count) {
                return Ok(false);
            }
        }
    }
    Ok(test
        .size
        .as_ref()
        .is_none_or(|test| bounds(test, values.len() as i32)))
}
