use super::{EntityPredicate, PredicateContext, PredicateRegistry};
use crate::loot::{DamageSource, Identifier, Result, codec};
use ferraloom_util::Vec3;
use serde_json::Value;

/// Port of TagPredicate#matches. Expected false inverts membership, not validity.
#[derive(Clone, Debug, PartialEq)]
pub struct TagPredicate {
    pub tag: ferraloom_protocol::codec::HolderSet,
    pub expected: bool,
}
impl TagPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self {
            tag: <ferraloom_protocol::codec::HolderSet as ferraloom_protocol::component::Persistent>::from_json(codec::field(v,"id")?)?,
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
    pub fn decode(v: &Value) -> Result<Self> {
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
        if let Some(test) = &self.direct_entity {
            if !test.matches(source.direct, Some(origin), facts)? {
                return Ok(false);
            }
        }
        if let Some(test) = &self.source_entity {
            if !test.matches(source.source, Some(origin), facts)? {
                return Ok(false);
            }
        }
        Ok(self
            .is_direct
            .is_none_or(|expected| expected == (source.source == source.direct)))
    }
}
