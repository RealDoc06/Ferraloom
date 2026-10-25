//! Port of CombatRules' float armor and magic-protection arithmetic.
use super::DamageSource;
use crate::{
    entity::{EntityMethod, EntityRef, LivingEntity, player::inventory::component},
    level::{Level, LevelError},
};
use ferraloom_data::registries::data_component_type::Id as ComponentType;
use ferraloom_protocol::component::DataComponent;
use ferraloom_util::java;

pub const MAX_ARMOR: f32 = 20.0;
pub const ARMOR_PROTECTION_DIVIDER: f32 = 25.0;
pub const BASE_ARMOR_TOUGHNESS: f32 = 2.0;
pub const MIN_ARMOR_RATIO: f32 = 0.2;

/// Port of CombatRules#getDamageAfterAbsorb. Enchantment evaluation is a
/// required value query; Warn cannot substitute a fraction for missing effects.
pub fn get_damage_after_absorb(
    victim: EntityRef,
    damage: f32,
    source: &DamageSource,
    total_armor: f32,
    armor_toughness: f32,
    level: &mut Level,
) -> Result<f32, LevelError> {
    let toughness = BASE_ARMOR_TOUGHNESS + armor_toughness / 4.0;
    let lower = total_armor * MIN_ARMOR_RATIO;
    let armor = total_armor - damage / toughness;
    let real_armor = if armor < lower {
        lower
    } else {
        java::min_f32(armor, MAX_ARMOR)
    };
    let armor_fraction = real_armor / ARMOR_PROTECTION_DIVIDER;
    let modified_fraction = match source.weapon_item(level)? {
        None => armor_fraction,
        Some(weapon) => {
            let enchanted = matches!(component(weapon, ComponentType::ENCHANTMENTS),
                Some(DataComponent::Enchantments(enchantments)) if !enchantments.0.is_empty());
            let kind = level.entity::<LivingEntity>(victim)?.entity_type;
            if enchanted {
                return Err(LevelError::UnsupportedEntityMethod(
                    kind,
                    EntityMethod::ModifyArmorEffectiveness,
                ));
            }
            // EnchantmentHelper's absent/empty component iteration is an identity.
            if armor_fraction < 0.0 {
                0.0
            } else {
                java::min_f32(armor_fraction, 1.0)
            }
        }
    };
    Ok(damage * (1.0 - modified_fraction))
}

/// Port of CombatRules#getDamageAfterMagicAbsorb and Mth#clamp(float).
pub fn get_damage_after_magic_absorb(damage: f32, total_magic_armor: f32) -> f32 {
    let real_armor = if total_magic_armor < 0.0 {
        0.0
    } else {
        java::min_f32(total_magic_armor, MAX_ARMOR)
    };
    damage * (1.0 - real_armor / ARMOR_PROTECTION_DIVIDER)
}

#[cfg(test)]
mod tests;
