use super::{EntityPredicate, PredicateContext, PredicateRegistry};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::loot::{DamageSource, Identifier, Result, codec};
use ferraloom_util::Vec3;

/// Port of TagPredicate#matches. Expected false inverts membership, not validity.
#[derive(Clone, Debug, PartialEq)]
pub struct TagPredicate {
    pub tag: ferraloom_protocol::codec::HolderSet,
    pub expected: bool,
}
impl TagPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            tag: ferraloom_protocol::codec::HolderSet::from_input(codec::field(v, "id")?)?,
            expected: codec::boolean(codec::field(v, "expected")?)?,
        })
    }
    pub fn matches(
        &self,
        value: &Identifier,
        registry: PredicateRegistry,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        Ok(super::item::matches_set(&self.tag, value, registry, facts)? == self.expected)
    }
}
/// Port of DamageSourcePredicate#matches, preserving direct/source evaluation order.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageSourcePredicate {
    pub tags: Vec<TagPredicate>,
    pub direct_entity: Option<EntityPredicate>,
    pub source_entity: Option<EntityPredicate>,
    pub is_direct: Option<bool>,
}
impl DamageSourcePredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            tags: codec::optional(v, "tags", |v| codec::list(v, TagPredicate::decode))?
                .unwrap_or_default(),
            direct_entity: codec::optional(v, "direct_entity", EntityPredicate::decode)?,
            source_entity: codec::optional(v, "source_entity", EntityPredicate::decode)?,
            is_direct: codec::optional(v, "is_direct", codec::boolean)?,
        })
    }
    pub fn matches(
        &self,
        source: &DamageSource,
        origin: Vec3,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        for tag in &self.tags {
            if !tag.matches(&source.damage_type, PredicateRegistry::DamageType, facts)? {
                return Ok(false);
            }
        }
        if let Some(test) = &self.direct_entity
            && !test.matches(source.direct, Some(origin), facts)?
        {
            return Ok(false);
        }
        if let Some(test) = &self.source_entity
            && !test.matches(source.source, Some(origin), facts)?
        {
            return Ok(false);
        }
        Ok(self
            .is_direct
            .is_none_or(|expected| expected == (source.source == source.direct)))
    }
}

/// Port of DamagePredicate#matches. The caller supplies the player's origin,
/// level facts and the original and actual damage from the damage calculation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamagePredicate {
    pub dealt: super::DoubleBounds,
    pub taken: super::DoubleBounds,
    pub source_entity: Option<EntityPredicate>,
    pub blocked: Option<bool>,
    pub damage_type: Option<DamageSourcePredicate>,
}
impl DamagePredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            dealt: codec::optional(v, "dealt", super::DoubleBounds::decode)?.unwrap_or_default(),
            taken: codec::optional(v, "taken", super::DoubleBounds::decode)?.unwrap_or_default(),
            source_entity: codec::optional(v, "source_entity", EntityPredicate::decode)?,
            blocked: codec::optional(v, "blocked", codec::boolean)?,
            damage_type: codec::optional(v, "type", DamageSourcePredicate::decode)?,
        })
    }
    pub fn matches(
        &self,
        origin: Vec3,
        source: &DamageSource,
        original: f32,
        actual: f32,
        blocked: bool,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        if !self.dealt.matches(f64::from(original)) || !self.taken.matches(f64::from(actual)) {
            return Ok(false);
        }
        if let Some(test) = &self.source_entity
            && !test.matches(source.source, Some(origin), facts)?
        {
            return Ok(false);
        }
        if self.blocked.is_some_and(|expected| expected != blocked) {
            return Ok(false);
        }
        self.damage_type
            .as_ref()
            .map_or(Ok(true), |test| test.matches(source, origin, facts))
    }
}
