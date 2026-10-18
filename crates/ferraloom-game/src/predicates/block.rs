use super::{PredicateContext, match_components};
use crate::loot::{Identifier, LootError, Result, codec};
use ferraloom_data::BlockState;
use ferraloom_nbt::{Tag, compare_nbt};
use ferraloom_protocol::{
    codec::{Either, HolderSet},
    component::{self, Persistent},
};
use ferraloom_util::BlockPos;
use serde_json::Value;

/// Port of StatePropertiesPredicate. Values compare in the property's typed order.
#[derive(Clone, Debug, PartialEq)]
pub struct StatePropertiesPredicate(pub component::StatePropertiesPredicate);
impl StatePropertiesPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self(component::StatePropertiesPredicate::from_json(v)?))
    }
    pub fn matches(&self, state: BlockState) -> bool {
        self.0.properties.iter().all(|p| {
            let Some(layout) = state
                .block()
                .data()
                .properties
                .iter()
                .find(|v| v.name == p.name)
            else {
                return false;
            };
            let Some((_, actual)) = state.properties().find(|(name, _)| *name == p.name) else {
                return false;
            };
            let index = |value: &str| layout.names.iter().position(|v| *v == value);
            let actual = index(actual).expect("state property value");
            match &p.matcher {
                Either::Left(value) => index(value) == Some(actual),
                Either::Right(range) => {
                    range
                        .min
                        .as_ref()
                        .is_none_or(|v| index(v).is_some_and(|min| actual >= min))
                        && range
                            .max
                            .as_ref()
                            .is_none_or(|v| index(v).is_some_and(|max| actual <= max))
                }
            }
        })
    }
}
/// Port of NbtPredicate#matches using NbtUtils#compareNbt's partial list semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct NbtPredicate(pub Tag);
impl NbtPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self(component::NbtPredicate::from_json(v)?.0.0))
    }
    pub fn matches(&self, actual: Option<&Tag>) -> bool {
        compare_nbt(Some(&self.0), actual, true)
    }
}
/// Port of BlockPredicate, including loaded-position and block-entity gates.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPredicate(pub component::BlockPredicate);
impl BlockPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self(component::BlockPredicate::from_json(v)?))
    }
    pub fn matches_state(&self, state: BlockState, facts: &dyn PredicateContext) -> Result<bool> {
        if let Some(set) = &self.0.blocks {
            let contains = match &set.0 {
                HolderSet::Direct(ids) => ids.iter().any(|v| v.as_str() == state.block().name()),
                HolderSet::Named(id) => {
                    let tags = facts.block_tags()?;
                    let key =
                        id.to_string()
                            .parse()
                            .map_err(|e: ferraloom_data::datapack::Error| {
                                LootError::Decode(e.to_string())
                            })?;
                    let tag =
                        tags.resolve_name(&key)
                            .ok_or_else(|| LootError::MissingReference {
                                registry: "block tag",
                                id: id.clone(),
                            })?;
                    tags.contains(tag, state.block())
                }
            };
            if !contains {
                return Ok(false);
            }
        }
        Ok(self
            .0
            .properties
            .as_ref()
            .is_none_or(|v| StatePropertiesPredicate(v.clone()).matches(state)))
    }
    pub fn needs_block_entity(&self) -> bool {
        self.0.nbt.is_some()
            || !self.0.components.exact.is_empty()
            || !self.0.components.partial.0.is_empty()
    }
    pub fn matches_entity(
        &self,
        nbt: Option<&Tag>,
        components: Option<&component::DataComponentMap>,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        if let Some(test) = &self.0.nbt {
            if !compare_nbt(Some(&test.0.0), nbt, true) {
                return Ok(false);
            }
        }
        if !self.0.components.exact.is_empty() || !self.0.components.partial.0.is_empty() {
            return components.map_or(Ok(false), |map| {
                match_components(&self.0.components, map, facts)
            });
        }
        Ok(true)
    }
    pub fn matches(&self, pos: BlockPos, facts: &dyn PredicateContext) -> Result<bool> {
        if !facts.loaded(pos)? || !self.matches_state(facts.block(pos)?, facts)? {
            return Ok(false);
        }
        if !self.needs_block_entity() {
            return Ok(true);
        }
        let nbt = if self.0.nbt.is_some() {
            facts.block_entity_nbt(pos)?
        } else {
            None
        };
        let components =
            if self.0.components.exact.is_empty() && self.0.components.partial.0.is_empty() {
                None
            } else {
                facts.block_entity_components(pos)?
            };
        self.matches_entity(nbt.as_ref(), components.as_ref(), facts)
    }
}
/// Fluid state facts use the registry identifier and typed property values.
#[derive(Clone, Debug)]
pub struct FluidState {
    pub fluid: Identifier,
    pub properties: Vec<(String, String)>,
}
/// Port of FluidPredicate. Fluid property matching awaits the level fluid adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct FluidPredicate {
    pub fluids: Option<HolderSet>,
    pub state: Option<component::StatePropertiesPredicate>,
}
impl FluidPredicate {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self {
            fluids: codec::optional(v, "fluids", |v| Ok(HolderSet::from_json(v)?))?,
            state: codec::optional(v, "state", |v| {
                Ok(component::StatePropertiesPredicate::from_json(v)?)
            })?,
        })
    }
    pub fn matches(&self, pos: BlockPos, facts: &dyn PredicateContext) -> Result<bool> {
        if !facts.loaded(pos)? {
            return Ok(false);
        }
        let fluid = facts.fluid(pos)?;
        if let Some(set) = &self.fluids {
            if !super::item::matches_set(set, &fluid.fluid, super::PredicateRegistry::Fluid, facts)?
            {
                return Ok(false);
            }
        }
        if self.state.is_some() {
            return Err(LootError::Unsupported("fluid.property_definition".into()));
        }
        Ok(true)
    }
}
