//! Port of Entity's movement-path block contacts and ordered inside effects.
use super::{Entity, EntityData, EntityRef, Movement, movement, synched_data};
use crate::{
    block::{LevelReaderRef, fluid::FlowingFluid},
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_protocol::codec::VarInt;
use ferraloom_util::{Aabb, BlockPos, Direction, Vec3, random::RandomSource, shapes};

/// Declaration order is InsideBlockEffectType's observable application order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum InsideBlockEffectType {
    Freeze,
    ClearFreeze,
    FireIgnite,
    LavaIgnite,
    Extinguish,
}
const ORDER: [InsideBlockEffectType; 5] = [
    InsideBlockEffectType::Freeze,
    InsideBlockEffectType::ClearFreeze,
    InsideBlockEffectType::FireIgnite,
    InsideBlockEffectType::LavaIgnite,
    InsideBlockEffectType::Extinguish,
];
/// Typed counterparts of vanilla's deferred entity consumers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InsideBlockAction {
    Effect(InsideBlockEffectType),
    LavaHurt,
    InFireHurt(f32),
    /// PowderSnowBlock#entityInside captures pos in runBefore(EXTINGUISH).
    PowderSnowExtinguish(BlockPos),
}

/// Port of InsideBlockEffectApplier.StepBasedCollector. Adjacent visits to the
/// same traversal step coalesce effects, while before/after actions retain order.
#[derive(Debug)]
pub struct StepBasedCollector {
    active: [bool; 5],
    before: [Vec<InsideBlockAction>; 5],
    after: [Vec<InsideBlockAction>; 5],
    final_effects: Vec<InsideBlockAction>,
    last_step: i32,
}
impl Default for StepBasedCollector {
    fn default() -> Self {
        Self {
            active: [false; 5],
            before: std::array::from_fn(|_| Vec::new()),
            after: std::array::from_fn(|_| Vec::new()),
            final_effects: Vec::new(),
            last_step: -1,
        }
    }
}
impl StepBasedCollector {
    pub fn advance_step(&mut self, step: i32) {
        if self.last_step != step {
            self.last_step = step;
            self.flush_step();
        }
    }
    pub fn apply(&mut self, effect: InsideBlockEffectType) {
        self.active[effect as usize] = true;
    }
    pub fn run_before(&mut self, effect: InsideBlockEffectType, action: InsideBlockAction) {
        self.before[effect as usize].push(action);
    }
    pub fn run_after(&mut self, effect: InsideBlockEffectType, action: InsideBlockAction) {
        self.after[effect as usize].push(action);
    }
    fn flush_step(&mut self) {
        for effect in ORDER {
            let i = effect as usize;
            self.final_effects.append(&mut self.before[i]);
            if std::mem::take(&mut self.active[i]) {
                self.final_effects.push(InsideBlockAction::Effect(effect));
            }
            self.final_effects.append(&mut self.after[i]);
        }
    }
    fn drain(&mut self) -> Vec<InsideBlockAction> {
        self.flush_step();
        self.last_step = -1;
        std::mem::take(&mut self.final_effects)
    }
}

fn is_alive(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let state = level.entity_state(entity)?;
    Ok(!state.as_entity().removed && state.as_living().is_none_or(|e| e.health() > 0.0))
}
fn affected(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    Ok(!base.removed && !base.no_physics)
}
pub fn is_on_fire(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    Ok(
        !level.entity_behavior(entity)?.fire_immune(entity, level)?
            && base.remaining_fire_ticks > 0,
    )
}
pub fn ignite_for_ticks(
    entity: EntityRef,
    ticks: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_behavior(entity)?
        .ignite_for_ticks(entity, ticks, level)
}
pub fn clear_fire(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    base.remaining_fire_ticks = base.remaining_fire_ticks.min(0);
    Ok(())
}

pub fn apply_collected<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let actions = level
        .entity_mut::<EntityData>(entity)?
        .inside_effect_collector
        .drain();
    for action in actions {
        if !is_alive(entity, level)? {
            break;
        }
        match action {
            InsideBlockAction::Effect(effect) => match effect {
                InsideBlockEffectType::Freeze => {
                    level.entity_mut::<EntityData>(entity)?.is_in_powder_snow = true;
                    if behavior.can_freeze(entity, level)? {
                        let base = level.entity_mut::<EntityData>(entity)?;
                        let frozen = base.ticks_frozen().wrapping_add(1).min(140);
                        base.entity_data
                            .set(synched_data::TICKS_FROZEN, VarInt(frozen));
                    }
                }
                InsideBlockEffectType::ClearFreeze => {
                    level
                        .entity_mut::<EntityData>(entity)?
                        .entity_data
                        .set(synched_data::TICKS_FROZEN, VarInt(0));
                }
                InsideBlockEffectType::Extinguish => clear_fire(entity, level)?,
                InsideBlockEffectType::LavaIgnite => {
                    if !behavior.fire_immune(entity, level)? {
                        behavior.ignite_for_ticks(entity, 300, level)?;
                    }
                }
                InsideBlockEffectType::FireIgnite => {
                    if !behavior.fire_immune(entity, level)? {
                        let base = level.entity::<EntityData>(entity)?;
                        let remaining = base.remaining_fire_ticks;
                        let player = base.entity_type == EntityType::PLAYER;
                        let add = if remaining < 0 {
                            1
                        } else if player {
                            level.random().next_int_between_inclusive(1, 2)
                        } else {
                            0
                        };
                        level.entity_mut::<EntityData>(entity)?.remaining_fire_ticks =
                            remaining.wrapping_add(add);
                        if level.entity::<EntityData>(entity)?.remaining_fire_ticks >= 0 {
                            behavior.ignite_for_ticks(entity, 160, level)?;
                        }
                    }
                }
            },
            InsideBlockAction::LavaHurt => {
                if !level
                    .entity::<EntityData>(entity)?
                    .entity_type
                    .data()
                    .fire_immune
                {
                    let source = level.damage_sources().lava();
                    let hurt = behavior.hurt_server(entity, level, source, 4.0)?;
                    if hurt
                        && behavior.should_play_lava_hurt_sound(entity, level)?
                        && !*level
                            .entity::<EntityData>(entity)?
                            .entity_data
                            .get(synched_data::SILENT)
                    {
                        let pitch =
                            2.0 + level.entity_mut::<EntityData>(entity)?.random.next_float() * 0.4;
                        level.play_sound(
                            None,
                            level.entity::<EntityData>(entity)?.position,
                            ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_BURN,
                            behavior.sound_source(),
                            0.4,
                            pitch,
                        )?;
                    }
                }
            }
            InsideBlockAction::PowderSnowExtinguish(pos) => {
                crate::block::plants::powder_snow_before_extinguish(entity, pos, level)?;
            }
            InsideBlockAction::InFireHurt(damage) => {
                let source = level.damage_sources().in_fire();
                behavior.hurt_server(entity, level, source, damage)?;
            }
        }
    }
    Ok(())
}

/// Port of Entity#checkInsideBlocks. The per-entity visited set spans all paths.
pub fn check_inside_blocks<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    movements: &[Movement],
    level: &mut Level,
) -> Result<(), LevelError> {
    if !affected(entity, level)? {
        return Ok(());
    }
    for movement in movements {
        let delta = movement.to - movement.from;
        let mut budget = 16;
        if let Some(original) = movement
            .axis_dependent_original_movement
            .filter(|_| delta.length_sqr() > 0.0)
        {
            let mut from = movement.from;
            for axis in Direction::axis_step_order(original) {
                let distance = delta.get(axis);
                if distance != 0.0 {
                    let to = match axis {
                        ferraloom_util::Axis::X => from.add_xyz(distance, 0.0, 0.0),
                        ferraloom_util::Axis::Y => from.add_xyz(0.0, distance, 0.0),
                        ferraloom_util::Axis::Z => from.add_xyz(0.0, 0.0, distance),
                    };
                    budget -= check_segment(behavior, entity, from, to, budget, level)?;
                    from = to;
                }
            }
        } else {
            budget -= check_segment(behavior, entity, movement.from, movement.to, 16, level)?;
        }
        if budget <= 0 {
            check_segment(behavior, entity, movement.to, movement.to, 1, level)?;
        }
    }
    level
        .entity_mut::<EntityData>(entity)?
        .visited_blocks
        .clear();
    Ok(())
}
fn check_segment<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    from: Vec3,
    to: Vec3,
    budget: i32,
    level: &mut Level,
) -> Result<i32, LevelError> {
    let dimensions = &level.entity::<EntityData>(entity)?.dimensions;
    let bb = dimensions.make_bounding_box(to).deflate(
        f64::from(1e-5_f32),
        f64::from(1e-5_f32),
        f64::from(1e-5_f32),
    );
    let from_box = dimensions.make_bounding_box(from);
    let travel = to - from;
    let moved_far = from.distance_to_sqr(to) > 0.9999900000002526_f64.powi(2);
    let mut iterations = 0;
    super::trace::for_each_intersected_block(from, to, bb, |pos, iteration| {
        if !is_alive(entity, level)? || iteration >= budget {
            return Ok(false);
        }
        iterations = iteration;
        let state = level.get_block_state(pos)?;
        if state.is_air() {
            return Ok(true);
        }
        let shape =
            LevelReaderRef::Server(level).entity_inside_collision_shape(state, pos, entity)?;
        let inside_block = shape.ptr_eq(&shapes::block())
            || from_box.collided_along_vector(
                travel,
                &shape
                    .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z))
                    .to_aabbs(),
            );
        let fluid = state.data().fluid;
        let inside_fluid = if fluid.empty {
            false
        } else {
            let kind = FlowingFluid::from_id(fluid.fluid_type)
                .ok_or(LevelError::UnsupportedBlock(state.block()))?;
            let height = kind.height(fluid, level, pos)?;
            from_box.collided_along_vector(
                travel,
                &[Aabb::new(
                    f64::from(pos.x),
                    f64::from(pos.y),
                    f64::from(pos.z),
                    f64::from(pos.x + 1),
                    f64::from(pos.y) + f64::from(height),
                    f64::from(pos.z + 1),
                )],
            )
        };
        if (inside_block || inside_fluid)
            && level
                .entity_mut::<EntityData>(entity)?
                .visited_blocks
                .insert(pos.as_long())
        {
            if inside_block {
                let base = level.entity_mut::<EntityData>(entity)?;
                base.inside_effect_collector.advance_step(iteration);
                base.inside_block_is_precise = moved_far || bb.intersects(Aabb::from_block(pos));
                level.call_entity_inside(state, pos, entity)?;
                behavior.on_inside_block(entity, state, level)?;
            }
            if inside_fluid {
                let kind = FlowingFluid::from_id(fluid.fluid_type)
                    .ok_or(LevelError::UnsupportedBlock(state.block()))?;
                let collector = &mut level
                    .entity_mut::<EntityData>(entity)?
                    .inside_effect_collector;
                collector.advance_step(iteration);
                match kind {
                    FlowingFluid::Water => collector.apply(InsideBlockEffectType::Extinguish),
                    FlowingFluid::Lava => {
                        collector.apply(InsideBlockEffectType::ClearFreeze);
                        collector.apply(InsideBlockEffectType::LavaIgnite);
                        collector.run_after(
                            InsideBlockEffectType::LavaIgnite,
                            InsideBlockAction::LavaHurt,
                        );
                    }
                }
            }
        }
        Ok(true)
    })?;
    Ok(iterations + 1)
}

/// Capture Entity#applyEffectsFromBlocks' movement history before callbacks.
pub fn collect_movements(
    entity: EntityRef,
    level: &mut Level,
) -> Result<Vec<Movement>, LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    base.final_movements_this_tick.clear();
    base.final_movements_this_tick
        .extend(base.movement_this_tick.drain(..));
    if let Some(last) = base.final_movements_this_tick.last() {
        if last.to.distance_to_sqr(base.position) > f64::from(1e-5_f32 * 1e-5_f32) {
            base.final_movements_this_tick.push(Movement {
                from: last.to,
                to: base.position,
                axis_dependent_original_movement: None,
            });
        }
    } else {
        base.final_movements_this_tick.push(Movement {
            from: base.old_position,
            to: base.position,
            axis_dependent_original_movement: None,
        });
    }
    Ok(base.final_movements_this_tick.clone())
}

/// Port of Entity#applyEffectsFromBlocks(List). Rain remains a required world
/// query; the Level implementation short-circuits dry weather before precipitation.
pub fn apply_effects<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    movements: &[Movement],
    level: &mut Level,
) -> Result<(), LevelError> {
    if !affected(entity, level)? {
        return Ok(());
    }
    if level.entity::<EntityData>(entity)?.on_ground {
        let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
        let state = level.get_block_state(pos)?;
        level.call_step_on(state, pos, entity)?;
    }
    let was_on_fire = is_on_fire(entity, level)?;
    let base = level.entity::<EntityData>(entity)?;
    let was_freezing = base.ticks_frozen() > 0;
    let previous_fire_ticks = base.remaining_fire_ticks;
    check_inside_blocks(behavior, entity, movements, level)?;
    apply_collected(behavior, entity, level)?;
    let base = level.entity::<EntityData>(entity)?;
    let pos = base.block_position;
    let top = BlockPos::containing(base.position.x, base.bounding_box.max_y, base.position.z);
    if level.is_raining_at(pos)? || level.is_raining_at(top)? {
        clear_fire(entity, level)?;
    }
    if was_on_fire && !is_on_fire(entity, level)?
        || was_freezing && level.entity::<EntityData>(entity)?.ticks_frozen() <= 0
    {
        behavior.play_fire_extinguished_sound(entity, level)?;
    }
    let ignited = level.entity::<EntityData>(entity)?.remaining_fire_ticks > previous_fire_ticks;
    if !is_on_fire(entity, level)? && !ignited {
        level.entity_mut::<EntityData>(entity)?.remaining_fire_ticks =
            -behavior.fire_immune_ticks();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ignition_uses_item_fire_immunity_and_effective_empty_components() {
        use crate::{
            Behaviors,
            block::UnportedBehavior,
            entity::{
                EntityState,
                item::{ItemEntity, ItemEntityBehavior},
            },
            level::{LevelContext, Region},
        };
        use ferraloom_data::Item;
        use ferraloom_protocol::component::ItemStack;
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut checked = 0;
        for row in include_str!("../../tests/java/entity/inside_fire/vectors.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
            level.set_unported_behavior(UnportedBehavior::Strict);
            level.random().set_seed(98);
            let mut item = ItemEntity::new(1, 0);
            item.set_item(ItemStack::new(
                Item::from_name(fields[1]).unwrap(),
                fields[2].parse().unwrap(),
                Default::default(),
            ));
            item.parent.remaining_fire_ticks = fields[4].parse().unwrap();
            item.parent
                .entity_data
                .set(synched_data::TICKS_FROZEN, VarInt(20));
            item.parent.inside_effect_collector.apply(match fields[3] {
                "FIRE_IGNITE" => InsideBlockEffectType::FireIgnite,
                "LAVA_IGNITE" => InsideBlockEffectType::LavaIgnite,
                _ => panic!("unexpected effect {row}"),
            });
            let entity = level.next_entity_ref();
            level
                .add_entity(entity, EntityState::Item(Box::new(item)))
                .unwrap();
            apply_collected(&ItemEntityBehavior, entity, &mut level).unwrap();
            let base = level.entity::<EntityData>(entity).unwrap();
            assert_eq!(
                base.remaining_fire_ticks.to_string(),
                fields[5],
                "fire {row}"
            );
            assert_eq!(base.ticks_frozen().to_string(), fields[6], "freeze {row}");
            assert_eq!(
                ItemEntityBehavior
                    .fire_immune(entity, &level)
                    .unwrap()
                    .to_string(),
                fields[7],
                "immunity {row}"
            );
            assert_eq!(
                is_on_fire(entity, &level).unwrap().to_string(),
                fields[8],
                "onFire {row}"
            );
            assert_eq!(
                format!("{:x}", level.random().next_long() as u64),
                fields[9],
                "rng {row}"
            );
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0, "{row}");
            checked += 1;
        }
        assert_eq!(checked, 48);
    }

    #[test]
    fn effects_coalesce_per_step_and_keep_before_after_declaration_order() {
        let mut c = StepBasedCollector::default();
        c.advance_step(0);
        c.apply(InsideBlockEffectType::Extinguish);
        c.run_after(
            InsideBlockEffectType::LavaIgnite,
            InsideBlockAction::LavaHurt,
        );
        c.apply(InsideBlockEffectType::LavaIgnite);
        c.apply(InsideBlockEffectType::LavaIgnite);
        c.run_before(
            InsideBlockEffectType::LavaIgnite,
            InsideBlockAction::InFireHurt(1.0),
        );
        c.advance_step(1);
        c.apply(InsideBlockEffectType::Freeze);
        assert_eq!(
            c.drain(),
            vec![
                InsideBlockAction::InFireHurt(1.0),
                InsideBlockAction::Effect(InsideBlockEffectType::LavaIgnite),
                InsideBlockAction::LavaHurt,
                InsideBlockAction::Effect(InsideBlockEffectType::Extinguish),
                InsideBlockAction::Effect(InsideBlockEffectType::Freeze)
            ]
        );
        assert!(c.drain().is_empty());
    }
}
