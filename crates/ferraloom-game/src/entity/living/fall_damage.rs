//! LivingEntity's ordered landing effects before and after the Entity parent.
use crate::{
    block::UnportedOperation,
    entity::{
        Entity, EntityData, EntityMethod, EntityRef, LivingEntity, movement,
        player::{ServerPlayer, equipment::SLOTS, inventory::component},
    },
    level::{Level, LevelError},
};
use ferraloom_data::{
    BlockState,
    registries::{attribute::Id as Attribute, data_component_type::Id as ComponentType},
};
use ferraloom_protocol::{component::DataComponent, entity_data::Particle};
use ferraloom_util::{BlockPos, Vec3, java, mth};

/// Port of LivingEntity#calculateFallDamage. Keep the double arithmetic and
/// native floor after the float modifier is widened, including NaN/infinity.
pub fn calculate_fall_damage(
    entity: EntityRef,
    distance: f64,
    modifier: f32,
    level: &Level,
) -> Result<i32, LevelError> {
    if level.entity_type_tags()?.contains_key(
        ferraloom_data::tags::EntityTypeTags::FALL_DAMAGE_IMMUNE,
        level.entity::<EntityData>(entity)?.entity_type,
    ) {
        return Ok(0);
    }
    let power = distance + 1.0e-6 - super::attribute(entity, Attribute::SAFE_FALL_DISTANCE, level)?;
    Ok(mth::floor(
        power
            * f64::from(modifier)
            * super::attribute(entity, Attribute::FALL_DAMAGE_MULTIPLIER, level)?,
    ))
}

/// Port of LivingEntity#playBlockFallSound. Its explicit Silent check also
/// applies to Player, whose ordinary playSound override bypasses silence.
pub fn play_block_fall_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if *base.entity_data.get(crate::entity::synched_data::SILENT) {
        return Ok(());
    }
    let position = base.position;
    let state = level.get_block_state(BlockPos::new(
        mth::floor(position.x),
        mth::floor(position.y - 0.20000000298023224),
        mth::floor(position.z),
    ))?;
    if !state.is_air() {
        let sound = state.data().sound;
        behavior.play_sound(
            entity,
            sound.fall_sound,
            sound.volume * 0.5,
            sound.pitch * 0.75,
            level,
        )?;
    }
    Ok(())
}

/// Port of LivingEntity#causeFallDamage. The explicit Entity parent keeps
/// passenger virtual dispatch; subsequent calls use this same derived receiver.
pub fn cause_fall_damage<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    distance: f64,
    modifier: f32,
    source: &crate::entity::DamageSource,
) -> Result<bool, LevelError> {
    let living = level.entity_mut::<LivingEntity>(entity)?;
    let effective = if let Some(impact) = living.current_impulse_impact_pos() {
        let effective = java::min(distance, impact.y - living.position.y);
        if effective <= 0.0 {
            living.reset_current_impulse_context();
        } else {
            living.try_reset_current_impulse_context();
        }
        effective
    } else {
        distance
    };
    let damaged = movement::base_cause_fall_damage(entity, level, effective, modifier, source)?;
    let damage = behavior.calculate_fall_damage(entity, effective, modifier, level)?;
    if damage <= 0 {
        return Ok(damaged);
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .reset_current_impulse_context();
    let sounds = behavior.fall_sounds();
    behavior.play_sound(
        entity,
        if damage > 4 { sounds.big } else { sounds.small },
        1.0,
        1.0,
        level,
    )?;
    behavior.play_block_fall_sound(entity, level)?;
    behavior.hurt_server(entity, level, source.clone(), damage as f32)?;
    Ok(true)
}

/// Port of Player#causeFallDamage. mayfly is an independent ability, and the
/// fall-distance stat precedes impulse adjustment in the LivingEntity parent.
pub fn player_cause_fall_damage<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    distance: f64,
    modifier: f32,
    source: &crate::entity::DamageSource,
) -> Result<bool, LevelError> {
    if level.entity::<ServerPlayer>(entity)?.abilities.mayfly {
        return Ok(false);
    }
    if distance >= 2.0 {
        // This branch is nonnegative: Java Math.round(double) returns a long,
        // then the native cast to int preserves only its low 32 bits.
        let centimeters = (distance * 100.0).round() as i64 as i32;
        behavior.award_stat(
            entity,
            crate::stats::Stat::Custom(crate::stats::custom::Id::FALL_ONE_CM),
            centimeters,
            level,
        )?;
    }
    cause_fall_damage(behavior, entity, level, distance, modifier, source)
}

/// Port of LivingEntity#onChangedBlock. Unenchanted equipment has no location
/// effects. Enchantment application replaces this counted unit-effect seam.
pub fn on_changed_block(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity_state(entity)?;
    let living = level.entity::<LivingEntity>(entity)?;
    let enchanted = SLOTS.into_iter().any(|slot| {
        let item = match (slot, state) {
            (
                ferraloom_protocol::component::EquipmentSlot::Mainhand,
                crate::entity::EntityState::Player(player),
            ) => player.get_main_hand_item(),
            _ => living.equipment.get(slot),
        };
        matches!(component(item, ComponentType::ENCHANTMENTS),
            Some(DataComponent::Enchantments(enchantments)) if !enchantments.0.is_empty())
    });
    if enchanted {
        let kind = living.entity_type;
        level.unported_operation(UnportedOperation::EntityMethod(
            kind,
            EntityMethod::RunLocationChangedEffects,
        ))?;
    }
    Ok(())
}

/// Port of LivingEntity#checkFallDamage, retaining the derived dispatcher for
/// onChangedBlock and ending fluid/world borrows before each subsequent read.
pub fn check_fall_damage<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    dy: f64,
    on_ground: bool,
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    if !level.entity::<EntityData>(entity)?.was_touching_water {
        crate::entity::fluid::update_fluid_interaction(behavior, entity, level)?;
    }
    if on_ground && level.entity::<EntityData>(entity)?.fall_distance > 0.0 {
        behavior.on_changed_block(entity, pos, level)?;
        let living = level.entity::<LivingEntity>(entity)?;
        let power = mth::floor(
            living.fall_distance + 1.0e-6
                - super::attribute(entity, Attribute::SAFE_FALL_DISTANCE, level)?,
        )
        .max(0);
        if power > 0 && !state.is_air() {
            let mut position = living.position;
            if pos.x != living.block_position.x || pos.z != living.block_position.z {
                let dx = position.x - f64::from(pos.x) - 0.5;
                let dz = position.z - f64::from(pos.z) - 0.5;
                let max = java::max(dx.abs(), dz.abs());
                position.x = f64::from(pos.x) + 0.5 + dx / max * 0.5;
                position.z = f64::from(pos.z) + 0.5 + dz / max * 0.5;
            }
            let scale = java::min(0.20000000298023224 + f64::from(power) / 15.0, 2.5);
            level.send_particles(
                Particle::Block(state),
                position,
                (150.0 * scale) as i32,
                Vec3::ZERO,
                0.15000000596046448,
            )?;
        }
    }
    movement::base_check_fall_damage(behavior, entity, dy, on_ground, state, pos, level)?;
    if on_ground {
        level
            .entity::<LivingEntity>(entity)?
            .last_climbable_pos
            .set(None);
    }
    Ok(())
}

/// Port of ServerPlayer#checkFallDamage's extra-particle branch followed by its
/// explicit LivingEntity parent call with the same ServerPlayer dispatcher.
pub fn player_check_fall_damage<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    dy: f64,
    on_ground: bool,
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    if player.spawn_extra_particles_on_fall && on_ground && player.fall_distance > 0.0 {
        let position = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 1.0,
            f64::from(pos.z) + 0.5,
        );
        let particles = (50.0 * player.fall_distance).clamp(0.0, 200.0) as i32;
        level.send_particles(
            Particle::Block(state),
            position,
            particles,
            Vec3::new(
                0.30000001192092896,
                0.30000001192092896,
                0.30000001192092896,
            ),
            0.15000000596046448,
        )?;
        level
            .entity_mut::<ServerPlayer>(entity)?
            .spawn_extra_particles_on_fall = false;
    }
    check_fall_damage(behavior, entity, dy, on_ground, state, pos, level)
}

#[cfg(test)]
mod tests;
