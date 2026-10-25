//! Typed value-component payloads of EnchantmentEffectComponents.
use super::{ConditionalEffect, EnchantmentValueEffect};
use crate::loot::{ContextKeySet, LootData, Result, codec};
use ferraloom_data::registries::enchantment_effect_component_type::Id as ComponentType;
use serde_json::Value;

/// Value payloads have distinct variants so a registry component cannot carry
/// another component's context or an unconditional effect in place of a list.
#[derive(Clone, Debug, PartialEq)]
pub enum EnchantmentValueEffectComponent {
    DamageProtection(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    Damage(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    SmashDamagePerFallenBlock(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    Knockback(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    ArmorEffectiveness(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    ItemDamage(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    AmmoUse(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    ProjectilePiercing(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    BlockExperience(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    RepairWithXp(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    ProjectileSpread(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    ProjectileCount(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    TridentReturnAcceleration(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    FishingTimeReduction(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    FishingLuckBonus(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    MobExperience(Vec<ConditionalEffect<EnchantmentValueEffect>>),
    CrossbowChargeTime(EnchantmentValueEffect),
    TridentSpinAttackStrength(EnchantmentValueEffect),
}
impl EnchantmentValueEffectComponent {
    /// Port of the value-effect component codecs. None identifies a component
    /// with a different native payload; its owning codec must decode that value.
    pub fn decode(component: ComponentType, value: &Value) -> Result<Option<Self>> {
        let list = || {
            codec::list(value, |value| {
                ConditionalEffect::decode(value, EnchantmentValueEffect::decode)
            })
        };
        Ok(Some(match component {
            ComponentType::DAMAGE_PROTECTION => Self::DamageProtection(list()?),
            ComponentType::DAMAGE => Self::Damage(list()?),
            ComponentType::SMASH_DAMAGE_PER_FALLEN_BLOCK => {
                Self::SmashDamagePerFallenBlock(list()?)
            }
            ComponentType::KNOCKBACK => Self::Knockback(list()?),
            ComponentType::ARMOR_EFFECTIVENESS => Self::ArmorEffectiveness(list()?),
            ComponentType::ITEM_DAMAGE => Self::ItemDamage(list()?),
            ComponentType::AMMO_USE => Self::AmmoUse(list()?),
            ComponentType::PROJECTILE_PIERCING => Self::ProjectilePiercing(list()?),
            ComponentType::BLOCK_EXPERIENCE => Self::BlockExperience(list()?),
            ComponentType::REPAIR_WITH_XP => Self::RepairWithXp(list()?),
            ComponentType::PROJECTILE_SPREAD => Self::ProjectileSpread(list()?),
            ComponentType::PROJECTILE_COUNT => Self::ProjectileCount(list()?),
            ComponentType::TRIDENT_RETURN_ACCELERATION => Self::TridentReturnAcceleration(list()?),
            ComponentType::FISHING_TIME_REDUCTION => Self::FishingTimeReduction(list()?),
            ComponentType::FISHING_LUCK_BONUS => Self::FishingLuckBonus(list()?),
            ComponentType::MOB_EXPERIENCE => Self::MobExperience(list()?),
            ComponentType::CROSSBOW_CHARGE_TIME => {
                Self::CrossbowChargeTime(EnchantmentValueEffect::decode(value)?)
            }
            ComponentType::TRIDENT_SPIN_ATTACK_STRENGTH => {
                Self::TridentSpinAttackStrength(EnchantmentValueEffect::decode(value)?)
            }
            _ => return Ok(None),
        }))
    }
    pub fn component_type(&self) -> ComponentType {
        match self {
            Self::DamageProtection(..) => ComponentType::DAMAGE_PROTECTION,
            Self::Damage(..) => ComponentType::DAMAGE,
            Self::SmashDamagePerFallenBlock(..) => ComponentType::SMASH_DAMAGE_PER_FALLEN_BLOCK,
            Self::Knockback(..) => ComponentType::KNOCKBACK,
            Self::ArmorEffectiveness(..) => ComponentType::ARMOR_EFFECTIVENESS,
            Self::ItemDamage(..) => ComponentType::ITEM_DAMAGE,
            Self::AmmoUse(..) => ComponentType::AMMO_USE,
            Self::ProjectilePiercing(..) => ComponentType::PROJECTILE_PIERCING,
            Self::BlockExperience(..) => ComponentType::BLOCK_EXPERIENCE,
            Self::RepairWithXp(..) => ComponentType::REPAIR_WITH_XP,
            Self::ProjectileSpread(..) => ComponentType::PROJECTILE_SPREAD,
            Self::ProjectileCount(..) => ComponentType::PROJECTILE_COUNT,
            Self::TridentReturnAcceleration(..) => ComponentType::TRIDENT_RETURN_ACCELERATION,
            Self::FishingTimeReduction(..) => ComponentType::FISHING_TIME_REDUCTION,
            Self::FishingLuckBonus(..) => ComponentType::FISHING_LUCK_BONUS,
            Self::MobExperience(..) => ComponentType::MOB_EXPERIENCE,
            Self::CrossbowChargeTime(..) => ComponentType::CROSSBOW_CHARGE_TIME,
            Self::TridentSpinAttackStrength(..) => ComponentType::TRIDENT_SPIN_ATTACK_STRENGTH,
        }
    }
    /// Port of validatedListCodec. Named predicates resolve against the same
    /// canonical LootData used at runtime, before publishing the registry.
    pub fn validate(&self, data: &LootData) -> Result<()> {
        let (effects, context) = match self {
            Self::DamageProtection(effects)
            | Self::Damage(effects)
            | Self::SmashDamagePerFallenBlock(effects)
            | Self::Knockback(effects)
            | Self::ArmorEffectiveness(effects) => (effects, "enchanted_damage"),
            Self::ItemDamage(effects)
            | Self::AmmoUse(effects)
            | Self::ProjectilePiercing(effects)
            | Self::BlockExperience(effects)
            | Self::RepairWithXp(effects) => (effects, "enchanted_item"),
            Self::ProjectileSpread(effects)
            | Self::ProjectileCount(effects)
            | Self::TridentReturnAcceleration(effects)
            | Self::FishingTimeReduction(effects)
            | Self::FishingLuckBonus(effects)
            | Self::MobExperience(effects) => (effects, "enchanted_entity"),
            Self::CrossbowChargeTime(..) | Self::TridentSpinAttackStrength(..) => return Ok(()),
        };
        let context =
            ContextKeySet::vanilla(&context.parse()?).expect("native enchantment parameter set");
        for effect in effects {
            effect.validate(data, &context)?;
        }
        Ok(())
    }
    pub fn conditional_effects(&self) -> Option<&[ConditionalEffect<EnchantmentValueEffect>]> {
        match self {
            Self::DamageProtection(effects)
            | Self::Damage(effects)
            | Self::SmashDamagePerFallenBlock(effects)
            | Self::Knockback(effects)
            | Self::ArmorEffectiveness(effects)
            | Self::ItemDamage(effects)
            | Self::AmmoUse(effects)
            | Self::ProjectilePiercing(effects)
            | Self::BlockExperience(effects)
            | Self::RepairWithXp(effects)
            | Self::ProjectileSpread(effects)
            | Self::ProjectileCount(effects)
            | Self::TridentReturnAcceleration(effects)
            | Self::FishingTimeReduction(effects)
            | Self::FishingLuckBonus(effects)
            | Self::MobExperience(effects) => Some(effects),
            Self::CrossbowChargeTime(..) | Self::TridentSpinAttackStrength(..) => None,
        }
    }
    pub fn unconditional_effect(&self) -> Option<&EnchantmentValueEffect> {
        match self {
            Self::CrossbowChargeTime(effect) | Self::TridentSpinAttackStrength(effect) => {
                Some(effect)
            }
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests;
