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
    movement::base_check_fall_damage(entity, dy, on_ground, state, pos, level)?;
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
