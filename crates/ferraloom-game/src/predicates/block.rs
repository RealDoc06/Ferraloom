use super::{PredicateContext, match_components};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::loot::{Identifier, LootError, Result, codec};
use ferraloom_data::BlockState;
use ferraloom_nbt::{Tag, compare_nbt};
use ferraloom_protocol::{
    codec::{Either, HolderSet},
    component,
};
use ferraloom_util::BlockPos;

/// Port of StatePropertiesPredicate. Values compare in the property's typed order.
#[derive(Clone, Debug, PartialEq)]
pub struct StatePropertiesPredicate(pub component::StatePropertiesPredicate);
impl StatePropertiesPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self(component::StatePropertiesPredicate::from_input(v)?))
    }
    pub fn matches(&self, state: BlockState) -> bool {
        self.0.properties.iter().all(|p| {
            let Some(values) = state.property_values(&p.name) else {
                return false;
            };
            let Some((_, actual)) = state.properties().find(|(name, _)| *name == p.name) else {
                return false;
            };
            let numeric = state.has_integer_property(&p.name);
            let boolean = values.iter().all(|v| matches!(*v, "true" | "false"));
            let index = |value: &str| -> Option<i32> {
                if numeric {
                    let value = java_integer(value)?;
                    values
                        .iter()
                        .any(|v| v.parse::<i32>().ok() == Some(value))
                        .then_some(value)
                } else if boolean {
                    match value {
                        "false" => Some(0),
                        "true" => Some(1),
                        _ => None,
                    }
                } else {
                    values.iter().position(|v| *v == value).map(|i| i as i32)
                }
            };
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
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self(component::NbtPredicate::from_input(v)?.0.0))
    }
    pub fn matches(&self, actual: Option<&Tag>) -> bool {
        compare_nbt(Some(&self.0), actual, true)
    }
}
/// Port of BlockPredicate, including loaded-position and block-entity gates.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPredicate(pub component::BlockPredicate);
impl BlockPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let value = component::BlockPredicate::from_input(v)?;
        if let Some(set) = &value.blocks {
            super::validate_static_holders(&set.0, "block", |id| {
                ferraloom_data::Block::from_name(id).is_some()
            })?;
        }
        Ok(Self(value))
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
        if let Some(test) = &self.0.nbt
            && !compare_nbt(Some(&test.0.0), nbt, true)
        {
            return Ok(false);
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
/// Port of FluidPredicate over the five built-in fluid types and their properties.
#[derive(Clone, Debug, PartialEq)]
pub struct FluidPredicate {
    pub fluids: Option<HolderSet>,
    pub state: Option<component::StatePropertiesPredicate>,
}
impl FluidPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            fluids: codec::optional(v, "fluids", |v| {
                let set = HolderSet::from_input(v)?;
                super::validate_static_holders(&set, "fluid", |id| {
                    ferraloom_data::registries::fluid::from_name(id).is_some()
                })?;
                Ok(set)
            })?,
            state: codec::optional(v, "state", |v| {
                Ok(component::StatePropertiesPredicate::from_input(v)?)
            })?,
        })
    }
    pub fn matches(&self, pos: BlockPos, facts: &dyn PredicateContext) -> Result<bool> {
        if !facts.loaded(pos)? {
            return Ok(false);
        }
        let fluid = facts.fluid(pos)?;
        if let Some(set) = &self.fluids
            && !super::item::matches_set(set, &fluid.fluid, super::PredicateRegistry::Fluid, facts)?
        {
            return Ok(false);
        }
        if let Some(test) = &self.state {
            for property in &test.properties {
                let Some((_, actual)) = fluid
                    .properties
                    .iter()
                    .find(|(name, _)| *name == property.name)
                else {
                    return Ok(false);
                };
                let falling = property.name == "falling"
                    && matches!(
                        fluid.fluid.as_str(),
                        "minecraft:water"
                            | "minecraft:flowing_water"
                            | "minecraft:lava"
                            | "minecraft:flowing_lava"
                    );
                let level = property.name == "level"
                    && matches!(
                        fluid.fluid.as_str(),
                        "minecraft:flowing_water" | "minecraft:flowing_lava"
                    );
                let index = |value: &str| -> Option<i32> {
                    if falling {
                        match value {
                            "false" => Some(0),
                            "true" => Some(1),
                            _ => None,
                        }
                    } else if level {
                        java_integer(value).filter(|n| (1..=8).contains(n))
                    } else {
                        None
                    }
                };
                let Some(actual) = index(actual) else {
                    return Ok(false);
                };
                let matches = match &property.matcher {
                    Either::Left(value) => index(value) == Some(actual),
                    Either::Right(range) => {
                        range
                            .min
                            .as_ref()
                            .is_none_or(|v| index(v).is_some_and(|v| actual >= v))
                            && range
                                .max
                                .as_ref()
                                .is_none_or(|v| index(v).is_some_and(|v| actual <= v))
                    }
                };
                if !matches {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

/// IntegerProperty uses Integer.parseInt, including BMP decimal digits and '+';
/// supplementary digits are rejected because Java traverses UTF-16 chars.
pub(crate) fn java_integer(input: &str) -> Option<i32> {
    let (negative, digits) = if let Some(s) = input.strip_prefix('-') {
        (true, s)
    } else {
        (false, input.strip_prefix('+').unwrap_or(input))
    };
    if digits.is_empty() {
        return None;
    }
    let mut result = 0_i32;
    const ZEROES: &[u32] = &[
        48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664,
        3792, 3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528,
        43216, 43264, 43472, 43504, 43600, 44016, 65296,
    ];
    for character in digits.chars() {
        let code = character as u32;
        let digit = ZEROES
            .iter()
            .find_map(|zero| code.checked_sub(*zero).filter(|v| *v < 10))?
            as i32;
        result = result.checked_mul(10)?.checked_sub(digit)?;
    }
    if negative {
        Some(result)
    } else {
        result.checked_neg()
    }
}
