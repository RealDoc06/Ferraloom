//! Typed enchantment wrappers reuse the canonical loot predicates and contexts.
use crate::loot::{
    ContextKeySet, Holder, LootData, LootFacts, LootItemCondition, LootParams, LootRuntime, Result,
    codec,
};
use ferraloom_util::random::RandomSource;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnchantmentTarget {
    Attacker,
    DamagingEntity,
    Victim,
}
impl EnchantmentTarget {
    fn decode(value: &Value) -> Result<Self> {
        Ok(match codec::string(value)? {
            "attacker" => Self::Attacker,
            "damaging_entity" => Self::DamagingEntity,
            "victim" => Self::Victim,
            other => return Err(codec::error(format!("unknown enchantment target {other}"))),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalEffect<T> {
    pub effect: T,
    pub requirements: Option<Holder<LootItemCondition>>,
}
impl<T> ConditionalEffect<T> {
    /// Port of ConditionalEffect#codec. Context validation belongs to the typed
    /// effect component, since each consumer supplies a different parameter set.
    pub fn decode(value: &Value, decode: impl FnOnce(&Value) -> Result<T>) -> Result<Self> {
        Ok(Self {
            effect: decode(codec::field(value, "effect")?)?,
            requirements: requirements(value)?,
        })
    }
    /// Port of EnchantmentEffectComponents#validatedListCodec's predicate check.
    pub fn validate(&self, data: &LootData, params: &ContextKeySet) -> Result<()> {
        validate(self.requirements.as_ref(), data, params)
    }
    pub fn matches(
        &self,
        data: &LootData,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<bool> {
        self.requirements.as_ref().map_or(Ok(true), |condition| {
            data.test_predicate(condition, params, random, facts)
        })
    }
    /// The live adapter retains the canonical selected stream and live facts
    /// across successive conditions; no item or entity snapshot is substituted.
    pub fn matches_with_runtime(
        &self,
        data: &LootData,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<bool> {
        self.requirements.as_ref().map_or(Ok(true), |condition| {
            data.test_predicate_with_runtime(condition, params, runtime)
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetedConditionalEffect<T> {
    pub enchanted: EnchantmentTarget,
    pub affected: EnchantmentTarget,
    pub effect: T,
    pub requirements: Option<Holder<LootItemCondition>>,
}
impl<T> TargetedConditionalEffect<T> {
    /// Port of TargetedConditionalEffect#codec and equipmentDropsCodec. Equipment
    /// drops fixes affected to victim and rejects damaging_entity as enchanted.
    pub fn decode(
        value: &Value,
        equipment_drops: bool,
        decode: impl FnOnce(&Value) -> Result<T>,
    ) -> Result<Self> {
        let enchanted = EnchantmentTarget::decode(codec::field(value, "enchanted")?)?;
        if equipment_drops && enchanted == EnchantmentTarget::DamagingEntity {
            return Err(codec::error(
                "equipment drops cannot enchant damaging_entity",
            ));
        }
        let affected = if equipment_drops {
            EnchantmentTarget::Victim
        } else {
            EnchantmentTarget::decode(codec::field(value, "affected")?)?
        };
        Ok(Self {
            enchanted,
            affected,
            effect: decode(codec::field(value, "effect")?)?,
            requirements: requirements(value)?,
        })
    }
    pub fn validate(&self, data: &LootData, params: &ContextKeySet) -> Result<()> {
        validate(self.requirements.as_ref(), data, params)
    }
    pub fn matches(
        &self,
        data: &LootData,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<bool> {
        self.requirements.as_ref().map_or(Ok(true), |condition| {
            data.test_predicate(condition, params, random, facts)
        })
    }
    pub fn matches_with_runtime(
        &self,
        data: &LootData,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<bool> {
        self.requirements.as_ref().map_or(Ok(true), |condition| {
            data.test_predicate_with_runtime(condition, params, runtime)
        })
    }
}
fn requirements(value: &Value) -> Result<Option<Holder<LootItemCondition>>> {
    codec::optional(value, "requirements", |value| {
        Holder::decode(value, LootItemCondition::decode)
    })
}
fn validate(
    requirement: Option<&Holder<LootItemCondition>>,
    data: &LootData,
    params: &ContextKeySet,
) -> Result<()> {
    if let Some(requirement) = requirement {
        let problems = data.validate_condition(requirement, params);
        if !problems.is_empty() {
            return Err(crate::loot::LootError::Validation(problems));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
