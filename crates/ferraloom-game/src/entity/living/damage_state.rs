//! LivingEntity's canonical damage memory, absorption and save-data references.
use super::LivingEntity;
use crate::{
    entity::{
        DamageSource, EntityDataView, EntityRef, EntityReference, player::ServerPlayer,
        synched_data,
    },
    level::{Level, LevelError},
};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{Uuid, java};

impl LivingEntity {
    /// Port of LivingEntity#isDeadOrDying.
    pub fn is_dead_or_dying(&self) -> bool {
        self.health() <= 0.0
    }
    pub fn no_action_time(&self) -> i32 {
        self.no_action_time
    }
    pub fn set_no_action_time(&mut self, ticks: i32) {
        self.no_action_time = ticks;
    }
    /// Port of LivingEntity#getAbsorptionAmount's parent body.
    pub fn absorption_amount(&self) -> f32 {
        self.absorption_amount
    }
    pub fn max_absorption(&self) -> f32 {
        self.attributes
            .value(Attribute::MAX_ABSORPTION)
            .expect("living supplier defines MAX_ABSORPTION") as f32
    }
    /// Port of LivingEntity#internalSetAbsorptionAmount, used by cold base loading.
    pub(crate) fn internal_set_absorption_amount(&mut self, amount: f32) {
        self.absorption_amount = amount;
    }
    /// Port of LivingEntity#setAbsorptionAmount's explicit parent setter.
    pub fn set_absorption_amount(&mut self, amount: f32) {
        self.internal_set_absorption_amount(clamp_absorption(amount, self.max_absorption()));
    }
    pub fn last_hurt_by_mob_timestamp(&self) -> i32 {
        self.last_hurt_by_mob_timestamp
    }
    pub fn last_hurt_mob(&self) -> Option<EntityRef> {
        self.last_hurt_mob
    }
    pub fn last_hurt_mob_timestamp(&self) -> i32 {
        self.last_hurt_mob_timestamp
    }
    pub fn combat_tracker(&self) -> &crate::entity::damage::combat_tracker::CombatTracker {
        &self.combat_tracker
    }
    pub fn last_hurt_by_player_memory_time(&self) -> i32 {
        self.last_hurt_by_player_memory_time
    }
    /// Port of LivingEntity#getLastDamageSource. The source remains retained even
    /// when its timeout makes this query return None.
    pub fn last_damage_source(&self, level: &Level, timeout: i32) -> Option<&DamageSource> {
        (level.game_time().wrapping_sub(self.last_damage_stamp) <= i64::from(timeout))
            .then_some(self.last_damage_source.as_ref())
            .flatten()
    }
    /// Every newly added canonical damage reference is traced in this parent.
    pub(crate) fn trace_damage_entity_refs(&self, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(&self.parent, references);
        references.extend(
            self.last_hurt_by_player
                .and_then(EntityReference::cached_entity),
        );
        references.extend(
            self.last_hurt_by_mob
                .and_then(EntityReference::cached_entity),
        );
        references.extend(self.last_hurt_mob);
        if let Some(source) = &self.last_damage_source {
            source.trace_entity_refs(references);
        }
        self.combat_tracker.trace_entity_refs(references);
    }
}
impl ServerPlayer {
    /// Port of Player#getAbsorptionAmount; the inherited base field stays unused.
    pub fn absorption_amount(&self) -> f32 {
        player_absorption_amount(&self.parent.parent)
    }
    pub fn set_absorption_amount(&mut self, amount: f32) {
        let amount = clamp_absorption(amount, self.max_absorption());
        player_internal_set_absorption_amount(&mut self.parent.parent, amount);
    }
}
fn clamp_absorption(value: f32, maximum: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else {
        java::min_f32(value, maximum)
    }
}
pub(crate) fn player_absorption_amount(living: &LivingEntity) -> f32 {
    *living.entity_data.get(synched_data::PLAYER_ABSORPTION)
}
pub(crate) fn player_internal_set_absorption_amount(living: &mut LivingEntity, amount: f32) {
    living
        .entity_data
        .set(synched_data::PLAYER_ABSORPTION, amount);
}

/// EntityReference.read uses UUIDUtil.CODEC, including numeric NBT collections;
/// the lenient optional read skips invalid UUIDs rather than accepting strings.
pub(crate) fn read_reference<T: EntityDataView>(
    tag: &CompoundTag,
    key: &str,
) -> Option<EntityReference<T>> {
    tag.get(key)
        .and_then(|value| Uuid::from_nbt(value).ok())
        .map(EntityReference::unresolved)
}
pub(super) fn save_references(
    living: &LivingEntity,
    tag: &mut CompoundTag,
    level: &Level,
) -> Result<(), LevelError> {
    for key in [
        "last_hurt_by_player",
        "last_hurt_by_player_memory_time",
        "last_hurt_by_mob",
        "ticks_since_last_hurt_by_mob",
    ] {
        tag.remove(key);
    }
    if let Some(reference) = living.last_hurt_by_player {
        tag.insert(
            "last_hurt_by_player",
            Tag::IntArray(reference.uuid(level)?.to_int_array().to_vec()),
        );
        tag.insert(
            "last_hurt_by_player_memory_time",
            Tag::Int(living.last_hurt_by_player_memory_time),
        );
    }
    if let Some(reference) = living.last_hurt_by_mob {
        tag.insert(
            "last_hurt_by_mob",
            Tag::IntArray(reference.uuid(level)?.to_int_array().to_vec()),
        );
        tag.insert(
            "ticks_since_last_hurt_by_mob",
            Tag::Int(
                living
                    .tick_count
                    .wrapping_sub(living.last_hurt_by_mob_timestamp),
            ),
        );
    }
    Ok(())
}

/// Port of LivingEntity#setLastHurtByMob, including the timestamp write for None.
pub fn set_last_hurt_by_mob(
    entity: EntityRef,
    attacker: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let reference = attacker
        .map(|id| EntityReference::of(id, level))
        .transpose()?;
    let living = level.entity_mut::<LivingEntity>(entity)?;
    living.last_hurt_by_mob = reference;
    living.last_hurt_by_mob_timestamp = living.tick_count;
    Ok(())
}
/// Port of LivingEntity#setLastHurtByPlayer's cached-reference overload.
pub fn set_last_hurt_by_player(
    entity: EntityRef,
    attacker: Option<EntityRef>,
    memory: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    let reference = attacker
        .map(|id| EntityReference::of(id, level))
        .transpose()?;
    let living = level.entity_mut::<LivingEntity>(entity)?;
    living.last_hurt_by_player = reference;
    living.last_hurt_by_player_memory_time = memory;
    Ok(())
}
/// Port of LivingEntity#setLastHurtMob. The attack producer owns when to call it;
/// a nonliving target clears the reference while still recording the timestamp.
pub fn set_last_hurt_mob(
    entity: EntityRef,
    target: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let target = target
        .map(|id| {
            level
                .entity_state(id)
                .map(|state| state.as_living().is_some().then_some(id))
        })
        .transpose()?
        .flatten();
    let living = level.entity_mut::<LivingEntity>(entity)?;
    living.last_hurt_mob = target;
    living.last_hurt_mob_timestamp = living.tick_count;
    Ok(())
}
/// LivingEntity#baseTick's damage-memory slots. The native lethal tickDeath
/// slot between the timers and memory belongs to the following death slice.
pub fn base_tick_damage_state(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let server_player = matches!(
        level.entity_state(entity)?,
        crate::entity::EntityState::Player(_)
    );
    let living = level.entity_mut::<LivingEntity>(entity)?;
    if living.hurt_time > 0 {
        living.hurt_time -= 1;
    }
    if living.damage_cooldown_time > 0 && !server_player {
        living.damage_cooldown_time -= 1;
    }
    if living.last_hurt_by_player_memory_time > 0 {
        living.last_hurt_by_player_memory_time -= 1;
    } else {
        living.last_hurt_by_player = None;
    }
    if let Some(target) = living.last_hurt_mob
        && !level.entity_behavior(target)?.is_alive(target, level)?
    {
        level.entity_mut::<LivingEntity>(entity)?.last_hurt_mob = None;
    }
    if let Some(attacker) = last_hurt_by_mob(entity, level)? {
        let alive = level.entity_behavior(attacker)?.is_alive(attacker, level)?;
        let living = level.entity::<LivingEntity>(entity)?;
        if !alive
            || living
                .tick_count
                .wrapping_sub(living.last_hurt_by_mob_timestamp)
                > 100
        {
            set_last_hurt_by_mob(entity, None, level)?;
        }
    }
    Ok(())
}
/// Resolving can invalidate a removed cached object. The copied reference is
/// written back after lookup; lookup holds no state borrow and invokes no callback.
pub fn last_hurt_by_mob(
    entity: EntityRef,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    let mut reference = level.entity::<LivingEntity>(entity)?.last_hurt_by_mob;
    let result = reference
        .as_mut()
        .map(|value| value.resolve(level))
        .transpose()?
        .flatten();
    level.entity_mut::<LivingEntity>(entity)?.last_hurt_by_mob = reference;
    Ok(result)
}
pub fn last_hurt_by_player(
    entity: EntityRef,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    let mut reference = level.entity::<LivingEntity>(entity)?.last_hurt_by_player;
    let result = reference
        .as_mut()
        .map(|value| value.resolve(level))
        .transpose()?
        .flatten();
    level
        .entity_mut::<LivingEntity>(entity)?
        .last_hurt_by_player = reference;
    Ok(result)
}

#[cfg(test)]
mod tests;
