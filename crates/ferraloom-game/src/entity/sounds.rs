//! Entity sound selection and sampling, at the native synchronous call sites.
use super::{Entity, EntityData, EntityRef, synched_data};
use crate::level::{Level, LevelError};
use ferraloom_data::{
    BlockState, RenderShape, registries::sound_event::Id as Sound, tags::BlockTags,
};
use ferraloom_protocol::entity_data::Particle;
use ferraloom_util::{BlockPos, Vec3, mth, random::RandomSource};

/// Port of Entity#playSound. Player's override deliberately bypasses silence.
pub fn play_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    sound: Sound,
    volume: f32,
    pitch: f32,
    except_self: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if except_self || !*base.entity_data.get(synched_data::SILENT) {
        level.play_sound(
            except_self.then_some(entity),
            base.position,
            sound,
            behavior.sound_source(),
            volume,
            pitch,
        )?;
    }
    Ok(())
}

/// Port of Entity#playEntityOnFireExtinguishedSound. This direct level call
/// includes the source player and deliberately does not check Silent.
pub fn play_fire_extinguished_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let random = &mut level.entity_mut::<EntityData>(entity)?.random;
    let pitch = 1.6 + (random.next_float() - random.next_float()) * 0.4;
    level.play_sound(
        None,
        level.entity::<EntityData>(entity)?.position,
        Sound::ENTITY_GENERIC_EXTINGUISH_FIRE,
        behavior.sound_source(),
        0.7,
        pitch,
    )
}

/// Port of Entity#playStepSound's base block sound.
pub fn play_step_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    state: BlockState,
    level: &mut Level,
) -> Result<(), LevelError> {
    let sound = state.data().sound;
    behavior.play_sound(
        entity,
        sound.step_sound,
        sound.volume * 0.15,
        sound.pitch,
        level,
    )
}

fn play_muffled_step_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    state: BlockState,
    level: &mut Level,
) -> Result<(), LevelError> {
    let sound = state.data().sound;
    behavior.play_sound(
        entity,
        sound.step_sound,
        sound.volume * 0.05,
        sound.pitch * 0.8,
        level,
    )
}

/// Port of Player#playStepSound, including above-block and combination sounds.
pub fn player_step_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    pos: BlockPos,
    state: BlockState,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<EntityData>(entity)?.was_touching_water {
        water_swim_sound(behavior, entity, level)?;
        return play_muffled_step_sound(behavior, entity, state, level);
    }
    let above = pos.above(1);
    let above_state = level.get_block_state(above)?;
    let tags = level.block_tags()?;
    if tags.contains_key(BlockTags::INSIDE_STEP_SOUND_BLOCKS, above_state.block())
        || tags.contains_key(
            BlockTags::COMBINATION_STEP_SOUND_BLOCKS,
            above_state.block(),
        )
    {
        // The native override reads the selected primary state a second time.
        let primary = level.get_block_state(above)?;
        let combination = level
            .block_tags()?
            .contains_key(BlockTags::COMBINATION_STEP_SOUND_BLOCKS, primary.block());
        play_step_sound(behavior, entity, primary, level)?;
        if combination {
            play_muffled_step_sound(behavior, entity, state, level)?;
        }
    } else {
        play_step_sound(behavior, entity, state, level)?;
    }
    Ok(())
}

/// Port of Entity#walkingStepSound and its amethyst intensity/interval state.
pub fn walking_step_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    pos: BlockPos,
    state: BlockState,
    level: &mut Level,
) -> Result<(), LevelError> {
    behavior.play_step_sound(entity, pos, state, level)?;
    if level
        .block_tags()?
        .contains_key(BlockTags::CRYSTAL_SOUND_BLOCKS, state.block())
    {
        let base = level.entity::<EntityData>(entity)?;
        if base.tick_count >= base.last_crystal_sound_play_tick.wrapping_add(20) {
            let (pitch, volume) = {
                let base = level.entity_mut::<EntityData>(entity)?;
                base.crystal_sound_intensity *= 0.997_f64.powf(f64::from(
                    base.tick_count
                        .wrapping_sub(base.last_crystal_sound_play_tick),
                )) as f32;
                let intensity = base.crystal_sound_intensity + 0.07;
                base.crystal_sound_intensity = if intensity > 1.0 { 1.0 } else { intensity };
                let pitch = 0.5 + base.crystal_sound_intensity * base.random.next_float() * 1.2;
                let volume = 0.1 + base.crystal_sound_intensity * 1.2;
                (pitch, volume)
            };
            behavior.play_sound(
                entity,
                Sound::BLOCK_AMETHYST_BLOCK_CHIME,
                volume,
                pitch,
                level,
            )?;
            let tick = level.entity::<EntityData>(entity)?.tick_count;
            level
                .entity_mut::<EntityData>(entity)?
                .last_crystal_sound_play_tick = tick;
        }
    }
    Ok(())
}

fn water_sound_movement<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    own_volume: f32,
    passenger_volume: f32,
    level: &Level,
) -> Result<(Vec3, f32), LevelError> {
    let controller = behavior
        .controlling_passenger(entity, level)?
        .unwrap_or(entity);
    let movement = level.entity::<EntityData>(controller)?.velocity;
    let modifier = if controller == entity {
        own_volume
    } else {
        passenger_volume
    };
    let speed = ((movement.x * movement.x * f64::from(0.2_f32)
        + movement.y * movement.y
        + movement.z * movement.z * f64::from(0.2_f32))
    .sqrt() as f32)
        * modifier;
    Ok((movement, if speed > 1.0 { 1.0 } else { speed }))
}

/// Port of Entity#waterSwimSound and Entity#playSwimSound.
pub fn water_swim_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let (_, volume) = water_sound_movement(behavior, entity, 0.35, 0.4, level)?;
    let random = &mut level.entity_mut::<EntityData>(entity)?.random;
    let pitch = 1.0 + (random.next_float() - random.next_float()) * 0.4;
    behavior.play_sound(entity, behavior.swim_sound(), volume, pitch, level)
}

/// Port of Entity#doWaterSplashEffect. Server addParticle is a no-op, but its
/// arguments still consume the entity random stream in native loop order.
pub fn water_splash_effect<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let (movement, volume) = water_sound_movement(behavior, entity, 0.2, 0.9, level)?;
    let random = &mut level.entity_mut::<EntityData>(entity)?.random;
    let pitch = 1.0 + (random.next_float() - random.next_float()) * 0.4;
    let sound = if volume < 0.25 {
        behavior.swim_splash_sound()
    } else {
        behavior.swim_high_speed_splash_sound()
    };
    behavior.play_sound(entity, sound, volume, pitch, level)?;
    let base = level.entity::<EntityData>(entity)?;
    let position = base.position;
    let width = base.dimensions.width;
    let y = f64::from(mth::floor(position.y) as f32 + 1.0);
    for bubble in [true, false] {
        let mut index = 0;
        while (index as f32) < 1.0 + width * 20.0 {
            let random = &mut level.entity_mut::<EntityData>(entity)?.random;
            let x = (random.next_double() * 2.0 - 1.0) * f64::from(width);
            let z = (random.next_double() * 2.0 - 1.0) * f64::from(width);
            let velocity_y = if bubble {
                movement.y - random.next_double() * f64::from(0.2_f32)
            } else {
                movement.y
            };
            level.add_particle(
                if bubble {
                    Particle::Bubble
                } else {
                    Particle::Splash
                },
                Vec3::new(position.x + x, y, position.z + z),
                Vec3::new(movement.x, velocity_y, movement.z),
            );
            index += 1;
        }
    }
    level.game_event(
        ferraloom_data::registries::game_event::Id::SPLASH,
        level.entity::<EntityData>(entity)?.position,
        Some(entity),
        None,
    )?;
    Ok(())
}

/// Port of Entity#canSpawnSprintParticle and Entity#spawnSprintParticle.
pub fn maybe_spawn_sprint_particle<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if !base.shared_flag(3)
        || base.was_touching_water
        || behavior.is_spectator(entity, level)?
        || base.pose() == ferraloom_protocol::entity_data::Pose::Crouching
        || base.is_in_lava(level)?
        || !behavior.is_alive(entity, level)?
    {
        return Ok(());
    }
    let pos = super::movement::get_on_pos(entity, 0.2, level)?;
    let state = level.get_block_state(pos)?;
    if state.data().render_shape == RenderShape::Invisible {
        return Ok(());
    }
    let (position, velocity) = {
        let base = level.entity_mut::<EntityData>(entity)?;
        let mut x =
            base.position.x + (base.random.next_double() - 0.5) * f64::from(base.dimensions.width);
        let mut z =
            base.position.z + (base.random.next_double() - 0.5) * f64::from(base.dimensions.width);
        if base.block_position.x != pos.x {
            x = mth::clamp(x, f64::from(pos.x), f64::from(pos.x) + 1.0);
        }
        if base.block_position.z != pos.z {
            z = mth::clamp(z, f64::from(pos.z), f64::from(pos.z) + 1.0);
        }
        (
            Vec3::new(x, base.position.y + 0.1, z),
            Vec3::new(base.velocity.x * -4.0, 1.5, base.velocity.z * -4.0),
        )
    };
    level.add_particle(Particle::Block(state), position, velocity);
    Ok(())
}
