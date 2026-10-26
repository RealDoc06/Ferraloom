//! LivingEntity's catalyst-facing reads and ordered death prefix. Death loot,
//! player death and kill producers remain required until the full death slice.
use super::{LivingEntity, damage_state};
use crate::{
    entity::{
        DamageSource, Entity, EntityData, EntityMethod, EntityRef,
        player::{ServerPlayer, equipment::SLOTS},
        synched_data,
    },
    level::{Level, LevelError},
};
use ferraloom_data::registries::{
    enchantment_effect_component_type::Id as EffectComponent, game_event,
};

impl LivingEntity {
    /// Port of LivingEntity#wasExperienceConsumed. This is transient state,
    /// intentionally absent from NBT and unchanged by an in-place NBT load.
    pub fn was_experience_consumed(&self) -> bool {
        self.skip_drop_experience
    }
    /// Port of LivingEntity#skipDropExperience, shared by every catalyst listener.
    pub fn skip_drop_experience(&mut self) {
        self.skip_drop_experience = true;
    }
    pub fn is_dead(&self) -> bool {
        self.dead
    }
}
fn required(entity: EntityRef, method: EntityMethod, level: &Level) -> LevelError {
    match level.entity_state(entity) {
        Ok(state) => LevelError::UnsupportedEntityMethod(state.entity_type(), method),
        Err(error) => error,
    }
}
/// Port of LivingEntity#getKillCredit: an existing player reference takes
/// precedence even when its lookup returns None; it never falls back to a mob.
pub fn kill_credit(entity: EntityRef, level: &mut Level) -> Result<Option<EntityRef>, LevelError> {
    if level
        .entity::<LivingEntity>(entity)?
        .last_hurt_by_player
        .is_some()
    {
        damage_state::last_hurt_by_player(entity, level)
    } else {
        damage_state::last_hurt_by_mob(entity, level)
    }
}
/// Port of LivingEntity#getLastDamageSource's native forty-tick read.
pub fn last_damage_source(
    entity: EntityRef,
    level: &Level,
) -> Result<Option<DamageSource>, LevelError> {
    Ok(level
        .entity::<LivingEntity>(entity)?
        .last_damage_source(level, 40)
        .cloned())
}
/// Port of LivingEntity#getExperienceReward. Base reward (including Animal RNG)
/// is evaluated before killer equipment. A nonliving killer has no enchantments.
pub fn experience_reward<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    killer: Option<EntityRef>,
    level: &mut Level,
) -> Result<i32, LevelError> {
    let amount = behavior.base_experience_reward(entity, level)?;
    if let Some(killer) = killer
        && level.entity_state(killer)?.as_living().is_some()
    {
        let killer_behavior = level.entity_behavior(killer)?;
        for slot in SLOTS {
            if let Some(stack) = killer_behavior.item_by_slot(killer, slot, level)?
                && let Some(ferraloom_protocol::component::DataComponent::Enchantments(entries)) =
                    crate::entity::player::inventory::component(
                        stack,
                        ferraloom_protocol::component::ComponentTypeId::ENCHANTMENTS,
                    )
                && !entries.0.is_empty()
            {
                let registry = level.context().enchantments()?;
                for (id, _) in &entries.0 {
                    let enchantment =
                        registry
                            .get(&id.0)
                            .ok_or_else(|| LevelError::EnchantmentEffect {
                                enchantment: id.0.clone(),
                                source: "unknown enchantment registry holder".into(),
                            })?;
                    if enchantment.definition.matching_slot(slot)
                        && let Some(effect) = enchantment
                            .value_effect(EffectComponent::MOB_EXPERIENCE)
                            .map_err(|error| LevelError::EnchantmentEffect {
                                enchantment: id.0.clone(),
                                source: error.to_string(),
                            })?
                        && effect
                            .conditional_effects()
                            .is_some_and(|effects| !effects.is_empty())
                    {
                        // The native ENCHANTED_ENTITY conditional context and
                        // effect execution remain required for a nonempty list.
                        return Err(required(entity, EntityMethod::ProcessMobExperience, level));
                    }
                }
            }
        }
        // MutableFloat narrows and widens even when no enchantment applies.
        return Ok((amount as f32) as i32);
    }
    Ok(amount)
}
/// Port of LivingEntity#die through dropAllDeathLoot. Every predecessor finishes
/// before synchronous ENTITY_DIE; a catalyst can consume XP before the required
/// loot boundary. A repeated call observes dead=true and performs no work.
pub fn die_parent<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &mut Level,
) -> Result<(), LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    if living.removed || living.dead {
        return Ok(());
    }
    let causing = source.causing_entity();
    if let Some(killer) = kill_credit(entity, level)? {
        level
            .entity_behavior(killer)?
            .award_kill_score(killer, entity, source, level)?;
    }
    if level
        .entity::<LivingEntity>(entity)?
        .entity_data
        .get(synched_data::SLEEPING_POS)
        .is_some()
    {
        return Err(required(entity, EntityMethod::StopSleeping, level));
    }
    super::stop_using_item(entity, level)?;
    if level
        .entity::<EntityData>(entity)?
        .entity_data
        .get(synched_data::CUSTOM_NAME)
        .is_some()
    {
        // Native logs CombatTracker#getDeathMessage before handleKillingBlow.
        return Err(required(entity, EntityMethod::DeathMessage, level));
    }
    behavior.handle_killing_blow(entity, level)?;
    let killed = match causing {
        None => true,
        Some(cause) => level
            .entity_behavior(cause)?
            .killed_entity(cause, entity, source, level)?,
    };
    if killed {
        behavior.game_event(entity, game_event::Id::ENTITY_DIE, level)?;
        behavior.drop_all_death_loot(entity, source, level)?;
    }
    // createWitherRose, byte3 and DYING pose belong after successful loot (or
    // a false killedEntity result). They remain required in the full death slice.
    Err(required(entity, EntityMethod::Die, level))
}
/// Player#getBaseExperienceReward's native world-rule and spectator gates.
pub fn player_base_experience_reward(entity: EntityRef, level: &Level) -> Result<i32, LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    Ok(
        if !level.rules().keep_inventory
            && !level.entity_behavior(entity)?.is_spectator(entity, level)?
        {
            player.experience_level.wrapping_mul(7).min(100)
        } else {
            0
        },
    )
}

#[cfg(test)]
mod tests;
