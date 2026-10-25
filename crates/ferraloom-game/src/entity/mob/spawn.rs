//! The currently supported Cow -> AgeableMob -> Mob spawn-finalization chain.
use crate::{
    entity::{
        EntityData, EntityMethod, EntityRef, EntitySpawnReason, LivingEntity, animal::Cow,
        synched_data,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_protocol::{
    Identifier,
    component::{AttributeModifier, AttributeOperation, CowSoundVariant, CowVariant, JavaF64},
};
use ferraloom_util::random::RandomSource;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Port of AgeableMob.AgeableMobGroupData, shared across one spawn group.
#[derive(Clone, Copy, Debug)]
pub struct AgeableMobGroupData {
    group_size: i32,
    should_spawn_baby: bool,
    baby_spawn_chance: f32,
}
impl Default for AgeableMobGroupData {
    fn default() -> Self {
        Self::new(true, 0.05)
    }
}
impl AgeableMobGroupData {
    pub fn new(should_spawn_baby: bool, baby_spawn_chance: f32) -> Self {
        Self {
            group_size: 0,
            should_spawn_baby,
            baby_spawn_chance,
        }
    }
    pub fn group_size(&self) -> i32 {
        self.group_size
    }
    pub fn should_spawn_baby(&self) -> bool {
        self.should_spawn_baby
    }
    pub fn baby_spawn_chance(&self) -> f32 {
        self.baby_spawn_chance
    }
    pub fn increase_group_size_by_one(&mut self) {
        self.group_size = self.group_size.wrapping_add(1);
    }
}

/// Port of Cow#finalizeSpawn, then AgeableMob and Mob. Their native
/// DifficultyInstance argument is unused in all three bodies; add that required
/// input when a supported mob first reads it. Every draw below is Level.random.
pub fn finalize_spawn(
    entity: EntityRef,
    _reason: EntitySpawnReason,
    group: Option<AgeableMobGroupData>,
    level: &mut Level,
) -> Result<AgeableMobGroupData, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if base.entity_type != EntityType::COW {
        return Err(LevelError::UnsupportedEntityMethod(
            base.entity_type,
            EntityMethod::FinalizeSpawn,
        ));
    }
    let pos = base.block_position;
    let variants = level.context().mob_variants()?.clone();
    let candidates = variants.cow.variants.candidates(pos, level)?;
    if !candidates.is_empty() {
        let variant =
            candidates[level.random().next_int_bound(candidates.len() as i32) as usize].clone();
        level
            .entity_mut::<LivingEntity>(entity)?
            .entity_data
            .set(synched_data::COW_VARIANT, CowVariant(variant));
    }
    let sounds = variants.cow.sound_variants();
    if sounds.is_empty() {
        return Err(LevelError::MissingNetworkRegistryEntry {
            registry: "minecraft:cow_sound_variant"
                .parse()
                .expect("native registry"),
            entry: level.entity::<Cow>(entity)?.sound_variant().0.clone(),
        });
    }
    let sound = sounds[level.random().next_int_bound(sounds.len() as i32) as usize].clone();
    level
        .entity_mut::<LivingEntity>(entity)?
        .entity_data
        .set(synched_data::COW_SOUND_VARIANT, CowSoundVariant(sound));

    let mut group = group.unwrap_or_default();
    if group.should_spawn_baby
        && group.group_size > 0
        && level.random().next_float() <= group.baby_spawn_chance
    {
        let changed = level.entity_mut::<Cow>(entity)?.set_age(-24000);
        if changed {
            crate::entity::dimensions::refresh_dimensions(
                level.entity_behavior(entity)?,
                entity,
                level,
            )?;
        }
    }
    group.increase_group_size_by_one();

    let bonus: Identifier = "minecraft:random_spawn_bonus"
        .parse()
        .expect("native modifier id");
    let exists = level
        .entity_mut::<LivingEntity>(entity)?
        .attributes
        .get_instance(Attribute::FOLLOW_RANGE)
        .expect("Mob supplier includes FOLLOW_RANGE")
        .modifier(&bonus)
        .is_some();
    if !exists {
        let amount = level.random().triangle(0.0, 0.11485000000000001);
        level
            .entity_mut::<LivingEntity>(entity)?
            .attributes
            .get_instance(Attribute::FOLLOW_RANGE)
            .expect("Mob supplier includes FOLLOW_RANGE")
            .add_permanent_modifier(Arc::new(AttributeModifier {
                id: bonus,
                amount: JavaF64(amount),
                operation: AttributeOperation::AddMultipliedBase,
            }))
            .expect("modifier presence checked without callbacks");
    }
    let left_handed = level.random().next_float() < 0.05;
    let living = level.entity_mut::<LivingEntity>(entity)?;
    let flags = *living.entity_data.get(synched_data::MOB_FLAGS);
    living.entity_data.set(
        synched_data::MOB_FLAGS,
        (flags & !2) | if left_handed { 2 } else { 0 },
    );
    Ok(group)
}
