//! Port of Entity#move. The derived dispatcher remains selected across parent helpers.
use super::{Entity, EntityData, EntityRef, Movement, collision};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockClass, BlockState, tags::BlockTags};
use ferraloom_util::{Aabb, Axis, BlockPos, Vec3, mth};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoverType {
    SelfMovement,
    Player,
    Piston,
    ShulkerBox,
    Shulker,
}
impl MoverType {
    pub fn is_server_and_client_simulated(self) -> bool {
        self != Self::SelfMovement
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementEmission {
    None,
    Sounds,
    Events,
    All,
}
impl MovementEmission {
    const fn from_flags(sound: bool, event: bool) -> Self {
        match (sound, event) {
            (false, false) => Self::None,
            (true, false) => Self::Sounds,
            (false, true) => Self::Events,
            (true, true) => Self::All,
        }
    }
    pub fn emits_sounds(self) -> bool {
        matches!(self, Self::Sounds | Self::All)
    }
    pub fn emits_events(self) -> bool {
        matches!(self, Self::Events | Self::All)
    }
}

/// Port of Entity#getInputVector: normalize only inputs longer than one, then
/// apply the float speed and lookup-table yaw before double vector arithmetic.
pub fn input_vector(input: Vec3, speed: f32, yaw: f32) -> Vec3 {
    let length = input.length_sqr();
    if length < 1.0e-7 {
        return Vec3::ZERO;
    }
    let movement = if length > 1.0 {
        input.normalize()
    } else {
        input
    } * f64::from(speed);
    let angle = f64::from(yaw * mth::DEG_TO_RAD);
    let sin = f64::from(mth::sin(angle));
    let cos = f64::from(mth::cos(angle));
    Vec3::new(
        movement.x * cos - movement.z * sin,
        movement.y,
        movement.z * cos + movement.x * sin,
    )
}

/// Port of Entity#calculateViewVector. Products round as floats before widening.
pub fn calculate_view_vector(pitch: f32, yaw: f32) -> Vec3 {
    let pitch = f64::from(pitch * mth::DEG_TO_RAD);
    let yaw = f64::from(-yaw * mth::DEG_TO_RAD);
    let y_cos = mth::cos(yaw);
    let y_sin = mth::sin(yaw);
    let x_cos = mth::cos(pitch);
    let x_sin = mth::sin(pitch);
    Vec3::new(
        f64::from(y_sin * x_cos),
        f64::from(-x_sin),
        f64::from(y_cos * x_cos),
    )
}

/// Port of Entity#limitPistonMovement, preserving first-nonzero-axis selection.
pub fn limit_piston_movement(base: &mut EntityData, movement: Vec3, game_time: i64) -> Vec3 {
    if movement.length_sqr() <= 1e-7 {
        return movement;
    }
    if base.piston_deltas_game_time != game_time {
        base.piston_deltas = [0.0; 3];
        base.piston_deltas_game_time = game_time;
    }
    for (index, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
        let amount = movement.get(axis);
        if amount != 0.0 {
            let limited = mth::clamp(amount + base.piston_deltas[index], -0.51, 0.51);
            let delta = limited - base.piston_deltas[index];
            base.piston_deltas[index] = limited;
            return if delta.abs() <= f64::from(1e-5_f32) {
                Vec3::ZERO
            } else {
                Vec3::ZERO.with(axis, delta)
            };
        }
    }
    Vec3::ZERO
}

pub fn get_on_pos(entity: EntityRef, offset: f32, level: &Level) -> Result<BlockPos, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let pos = base.position;
    if let Some(support) = base.supporting_block_pos {
        if offset <= 1e-5_f32 || offset.is_nan() {
            return Ok(support);
        }
        let state = level.get_block_state(support)?;
        let fence = offset <= 0.5
            && level
                .block_tags()?
                .contains_key(BlockTags::FENCES, state.block());
        if fence
            || level
                .block_tags()?
                .contains_key(BlockTags::WALLS, state.block())
            || state.block().is_instance_of(BlockClass::FenceGateBlock)
        {
            return Ok(support);
        }
        return Ok(BlockPos::new(
            support.x,
            mth::floor(pos.y - f64::from(offset)),
            support.z,
        ));
    }
    Ok(BlockPos::containing(
        pos.x,
        pos.y - f64::from(offset),
        pos.z,
    ))
}
pub fn block_speed_factor(entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
    let state = level.get_block_state(level.entity::<EntityData>(entity)?.block_position)?;
    let factor = state.block().data().speed_factor;
    if factor != 1.0 || matches!(state.block(), Block::WATER | Block::BUBBLE_COLUMN) {
        return Ok(factor);
    }
    Ok(level
        .get_block_state(
            level
                .entity_behavior(entity)?
                .block_pos_below_that_affects_my_movement(entity, level)?,
        )?
        .block()
        .data()
        .speed_factor)
}
pub fn block_jump_factor(entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
    let here = level
        .get_block_state(level.entity::<EntityData>(entity)?.block_position)?
        .block()
        .data()
        .jump_factor;
    let below = level
        .get_block_state(
            level
                .entity_behavior(entity)?
                .block_pos_below_that_affects_my_movement(entity, level)?,
        )?
        .block()
        .data()
        .jump_factor;
    Ok(if here == 1.0 { below } else { here })
}

/// Port of Entity#checkSupportingBlock, including the previous-horizontal-position retry.
pub fn check_supporting_block(
    entity: EntityRef,
    on_ground: bool,
    movement: Option<Vec3>,
    level: &mut Level,
) -> Result<(), LevelError> {
    if !on_ground {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.on_ground_no_blocks = false;
        base.supporting_block_pos = None;
        return Ok(());
    }
    let base = level.entity::<EntityData>(entity)?;
    let bb = base.bounding_box;
    let no_blocks = base.on_ground_no_blocks;
    let test = Aabb::new(
        bb.min_x,
        bb.min_y - 1e-6,
        bb.min_z,
        bb.max_x,
        bb.min_y,
        bb.max_z,
    );
    let mut support = collision::find_supporting_block(entity, test, level)?;
    if support.is_some() || no_blocks {
        level.entity_mut::<EntityData>(entity)?.supporting_block_pos = support;
    } else if let Some(movement) = movement {
        support = collision::find_supporting_block(
            entity,
            test.move_by(-movement.x, 0.0, -movement.z),
            level,
        )?;
        level.entity_mut::<EntityData>(entity)?.supporting_block_pos = support;
    }
    level.entity_mut::<EntityData>(entity)?.on_ground_no_blocks = support.is_none();
    Ok(())
}
pub fn set_on_ground(
    entity: EntityRef,
    on_ground: bool,
    movement: Option<Vec3>,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity_mut::<EntityData>(entity)?.on_ground = on_ground;
    check_supporting_block(entity, on_ground, movement, level)
}

pub fn move_entity<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    mover: MoverType,
    mut delta: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<EntityData>(entity)?.no_physics {
        let position = level.entity::<EntityData>(entity)?.position + delta;
        level.set_entity_pos(entity, position)?;
        let base = level.entity_mut::<EntityData>(entity)?;
        base.horizontal_collision = false;
        base.vertical_collision = false;
        base.vertical_collision_below = false;
        base.minor_horizontal_collision = false;
        return Ok(());
    }
    if mover == MoverType::Piston {
        let time = level.game_time();
        delta = limit_piston_movement(level.entity_mut::<EntityData>(entity)?, delta, time);
        if delta == Vec3::ZERO {
            return Ok(());
        }
    }
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        if base.stuck_speed_multiplier.length_sqr() > 1e-7 {
            let multiplier = base.stuck_speed_multiplier;
            if mover != MoverType::Piston {
                delta = delta.multiply(multiplier.x, multiplier.y, multiplier.z);
            }
            base.stuck_speed_multiplier = Vec3::ZERO;
            base.velocity = Vec3::ZERO;
        }
    }
    delta = behavior.maybe_back_off_from_edge(entity, level, delta, mover)?;
    let max_step = behavior.max_up_step(entity, level)?;
    let movement = collision::collide(entity, delta, max_step, level)?;
    let length = movement.length_sqr();
    if length > 1e-7 || delta.length_sqr() - length < 1e-7 {
        if level.entity::<EntityData>(entity)?.fall_distance != 0.0
            && length >= 1.0
            && super::trace::fall_distance_resetting_hit(entity, movement, level)?
        {
            level.entity_mut::<EntityData>(entity)?.fall_distance = 0.0;
        }
        let position = level.entity::<EntityData>(entity)?.position;
        level
            .entity_mut::<EntityData>(entity)?
            .add_movement(Movement {
                from: position,
                to: position + movement,
                axis_dependent_original_movement: Some(delta),
            });
        level.set_entity_pos(entity, position + movement)?;
    }
    let x_collision = !mth::equal(delta.x, movement.x);
    let z_collision = !mth::equal(delta.z, movement.z);
    let horizontal = x_collision || z_collision;
    let moved_vertically = delta.y.abs() > 0.0;
    let authoritative = behavior.is_local_instance_authoritative(entity, level)?;
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.horizontal_collision = horizontal;
        if moved_vertically || authoritative {
            base.vertical_collision = delta.y != movement.y;
            base.vertical_collision_below = base.vertical_collision && delta.y < 0.0;
        }
        base.minor_horizontal_collision = false;
    }
    if moved_vertically || authoritative {
        let on_ground = level.entity::<EntityData>(entity)?.vertical_collision_below;
        set_on_ground(entity, on_ground, Some(movement), level)?;
    }
    if horizontal {
        let minor = behavior.is_horizontal_collision_minor(entity, level, movement)?;
        level
            .entity_mut::<EntityData>(entity)?
            .minor_horizontal_collision = minor;
    }
    let effect_pos = get_on_pos(entity, 0.2_f32, level)?;
    let effect_state = level.get_block_state(effect_pos)?;
    if authoritative {
        check_fall_damage(
            entity,
            movement.y,
            level.entity::<EntityData>(entity)?.on_ground,
            effect_state,
            effect_pos,
            level,
        )?;
    }
    if level.entity::<EntityData>(entity)?.removed {
        return Ok(());
    }
    if (moved_vertically && level.entity::<EntityData>(entity)?.vertical_collision) || horizontal {
        restitute(
            behavior,
            entity,
            effect_state,
            x_collision,
            z_collision,
            movement,
            level,
        )?;
    }
    let emission = behavior.movement_emission();
    if emission != MovementEmission::None
        && level.entity::<EntityData>(entity)?.vehicle.get().is_none()
    {
        apply_movement_emission(
            behavior,
            entity,
            emission,
            movement,
            effect_pos,
            effect_state,
            level,
        )?;
    }
    let factor = f64::from(behavior.block_speed_factor(entity, level)?);
    let base = level.entity_mut::<EntityData>(entity)?;
    base.velocity = base.velocity.multiply(factor, 1.0, factor);
    Ok(())
}

/// Port of Entity#checkFallDamage. fallOn runs before reset and can reenter.
pub fn check_fall_damage(
    entity: EntityRef,
    dy: f64,
    on_ground: bool,
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_mut::<EntityData>(entity)?
        .accumulate_fall_distance(dy);
    let base = level.entity::<EntityData>(entity)?;
    if !on_ground {
        return Ok(());
    }
    let distance = base.fall_distance;
    if distance > 0.0 {
        level.call_fall_on(state, pos, entity, distance)?;
        let base = level.entity::<EntityData>(entity)?;
        let position = base.position;
        let support = base.supporting_block_pos;
        let support_state = if let Some(support) = support {
            level.get_block_state(support)?
        } else {
            state
        };
        level.game_event(
            ferraloom_data::registries::game_event::Id::HIT_GROUND,
            position,
            Some(entity),
            Some(support_state),
        )?;
    }
    level.entity_mut::<EntityData>(entity)?.fall_distance = 0.0;
    Ok(())
}
fn restitute<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    state: BlockState,
    x: bool,
    z: bool,
    movement: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let suppressed = level.entity::<EntityData>(entity)?.shared_flag(1);
    let mut restitution = if suppressed {
        0.0
    } else {
        behavior.bounciness(entity, level)?
    };
    let current = level.entity::<EntityData>(entity)?.velocity;
    let mut after = current;
    if x {
        after = after.with(Axis::X, -current.x * restitution);
    }
    if z {
        after = after.with(Axis::Z, -current.z * restitution);
    }
    let mut bounced = restitution > 0.0 && (x || z);
    let base = level.entity::<EntityData>(entity)?;
    if base.vertical_collision {
        let gravity = behavior.effective_gravity(entity, level)?;
        if base.vertical_collision_below {
            restitution = if -current.y > gravity
                && !suppressed
                && !level
                    .block_tags()?
                    .contains_key(BlockTags::SUPPRESSES_BOUNCE, state.block())
            {
                let mut block = state.block().data().bounce_restitution;
                if level.entity_state(entity)?.as_living().is_none() {
                    block *= 0.8_f32;
                }
                restitution.max(f64::from(block))
            } else {
                0.0
            };
        }
        let (compensation, drag) = if restitution > 0.0 {
            let portion = movement.y / current.y;
            bounced = true;
            (
                portion * gravity,
                mth::lerp(portion, 1.0, f64::from(behavior.air_drag(entity, level)?)),
            )
        } else {
            (0.0, 1.0)
        };
        after = after.with(Axis::Y, (compensation - current.y) * drag * restitution);
    }
    if bounced {
        let pos = behavior.block_pos_below_that_affects_my_movement(entity, level)?;
        let distance = level.entity::<EntityData>(entity)?.fall_distance;
        level.call_bounce_on(state, pos, entity, distance)?;
        level.entity_mut::<EntityData>(entity)?.sync_position = true;
    }
    level.entity_mut::<EntityData>(entity)?.velocity = after;
    Ok(())
}
fn climbable(state: BlockState, level: &Level) -> Result<bool, LevelError> {
    Ok(state.block() == Block::POWDER_SNOW
        || level
            .block_tags()?
            .contains_key(BlockTags::CLIMBABLE, state.block()))
}
fn apply_movement_emission<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    emission: MovementEmission,
    movement: Vec3,
    effect_pos: BlockPos,
    effect_state: BlockState,
    level: &mut Level,
) -> Result<(), LevelError> {
    let distance = (movement.length() * f64::from(0.6_f32)) as f32;
    let horizontal = (movement.horizontal_distance() * f64::from(0.6_f32)) as f32;
    let pos = get_on_pos(entity, 1e-5_f32, level)?;
    let state = level.get_block_state(pos)?;
    let climbing = climbable(state, level)?;
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.move_dist += if climbing { distance } else { horizontal };
        base.fly_dist += distance;
    }
    let base = level.entity::<EntityData>(entity)?;
    if base.move_dist <= base.next_step || state.is_air() {
        return Ok(());
    }
    let same = pos == effect_pos;
    let mut effects = emit_step(
        behavior,
        entity,
        effect_pos,
        effect_state,
        MovementEmission::from_flags(emission.emits_sounds(), same),
        movement,
        level,
    )?;
    if !same {
        effects |= emit_step(
            behavior,
            entity,
            pos,
            state,
            MovementEmission::from_flags(false, emission.emits_events()),
            movement,
            level,
        )?;
    }
    if effects || level.entity::<EntityData>(entity)?.was_touching_water {
        if !effects {
            if emission.emits_sounds() {
                super::sounds::water_swim_sound(behavior, entity, level)?;
            }
            if emission.emits_events() {
                level.game_event(
                    ferraloom_data::registries::game_event::Id::SWIM,
                    level.entity::<EntityData>(entity)?.position,
                    Some(entity),
                    None,
                )?;
            }
        }
        let base = level.entity_mut::<EntityData>(entity)?;
        base.next_step = base.move_dist as i32 as f32 + 1.0;
    }
    Ok(())
}
fn emit_step<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    pos: BlockPos,
    state: BlockState,
    emission: MovementEmission,
    movement: Vec3,
    level: &mut Level,
) -> Result<bool, LevelError> {
    if state.is_air() {
        return Ok(false);
    }
    let is_climbable = climbable(state, level)?;
    let base = level.entity::<EntityData>(entity)?;
    if (base.on_ground
        || is_climbable
        || base.pose() == ferraloom_protocol::entity_data::Pose::Crouching && movement.y == 0.0)
        && !behavior.is_swimming(entity, level)?
    {
        if emission.emits_sounds() {
            super::sounds::walking_step_sound(behavior, entity, pos, state, level)?;
        }
        if emission.emits_events() {
            level.game_event(
                ferraloom_data::registries::game_event::Id::STEP,
                level.entity::<EntityData>(entity)?.position,
                Some(entity),
                Some(state),
            )?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Port of Player#maybeBackOffFromEdge, including separate X/Z/diagonal loops.
pub fn player_back_off_from_edge(
    entity: EntityRef,
    level: &Level,
    delta: Vec3,
    mover: MoverType,
    max_step: f32,
    flying: bool,
) -> Result<Vec3, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if flying
        || delta.y > 0.0
        || !matches!(mover, MoverType::SelfMovement | MoverType::Player)
        || !base.shared_flag(1)
    {
        return Ok(delta);
    }
    let grounded = base.on_ground
        || base.fall_distance < f64::from(max_step)
            && !can_fall_at_least(
                entity,
                level,
                0.0,
                0.0,
                f64::from(max_step) - base.fall_distance,
            )?;
    if !grounded {
        return Ok(delta);
    }
    let (mut x, mut z) = (delta.x, delta.z);
    let (step_x, step_z) = (x.signum() * 0.05, z.signum() * 0.05);
    while x != 0.0 && can_fall_at_least(entity, level, x, 0.0, f64::from(max_step))? {
        if x.abs() <= 0.05 {
            x = 0.0;
            break;
        }
        x -= step_x;
    }
    while z != 0.0 && can_fall_at_least(entity, level, 0.0, z, f64::from(max_step))? {
        if z.abs() <= 0.05 {
            z = 0.0;
            break;
        }
        z -= step_z;
    }
    while x != 0.0 && z != 0.0 && can_fall_at_least(entity, level, x, z, f64::from(max_step))? {
        x = if x.abs() <= 0.05 { 0.0 } else { x - step_x };
        z = if z.abs() <= 0.05 { 0.0 } else { z - step_z };
    }
    Ok(Vec3::new(x, delta.y, z))
}
fn can_fall_at_least(
    entity: EntityRef,
    level: &Level,
    x: f64,
    z: f64,
    min_height: f64,
) -> Result<bool, LevelError> {
    let bb = level.entity::<EntityData>(entity)?.bounding_box;
    level.no_collision(
        Some(entity),
        Aabb::new(
            bb.min_x + 1e-7 + x,
            bb.min_y - min_height - 1e-7,
            bb.min_z + 1e-7 + z,
            bb.max_x - 1e-7 + x,
            bb.min_y,
            bb.max_z - 1e-7 + z,
        ),
        false,
    )
}
