//! Port of EntityFluidInteraction and Entity#updateFluidInteraction (26.3).
use super::{Entity, EntityData, EntityRef, EntityState};
use crate::{
    block::{LevelReaderRef, fluid::FlowingFluid},
    level::{Level, LevelError},
};
use ferraloom_data::{
    FluidState,
    registries::fluid::Id as Fluid,
    tags::{FluidTags, TagKey},
};
use ferraloom_util::{Aabb, BlockPos, ChunkPos, Direction, Vec3};

#[derive(Clone, Debug)]
struct Tracker {
    fluid: Fluid,
    height: f64,
    eyes_inside: bool,
}
#[derive(Clone, Copy, Debug, Default)]
struct Current {
    height: f64,
    accumulated: Vec3,
    count: i32,
}
/// Canonical fluid trackers are distinct per concrete fluid, including flowing
/// variants; tag heights aggregate these exactly as EntityFluidInteraction does.
#[derive(Debug, Default)]
pub struct EntityFluidInteraction {
    trackers: Vec<Tracker>,
    currents: [Current; 2],
}
impl EntityFluidInteraction {
    fn reset(&mut self) {
        self.trackers.retain(|t| t.height != 0.0 || t.eyes_inside);
        for tracker in &mut self.trackers {
            tracker.height = 0.0;
            tracker.eyes_inside = false;
        }
        self.currents = [Current::default(); 2];
    }
    pub fn height(&self, tag: TagKey<Fluid>, level: &Level) -> Result<f64, LevelError> {
        let mut height = 0.0;
        for tracker in &self.trackers {
            if tracker.height > height && level.fluid_tags()?.contains_key(tag, tracker.fluid) {
                height = tracker.height;
            }
        }
        Ok(height)
    }
    pub fn eyes_inside(&self, tag: TagKey<Fluid>, level: &Level) -> Result<bool, LevelError> {
        for tracker in &self.trackers {
            if tracker.eyes_inside && level.fluid_tags()?.contains_key(tag, tracker.fluid) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn has_fluid_and_loaded(level: &Level, bb: Aabb) -> bool {
    let min = BlockPos::containing(
        bb.min_x.floor() - 1.0,
        bb.min_y.floor(),
        bb.min_z.floor() - 1.0,
    );
    let max = BlockPos::new(
        bb.max_x.ceil() as i32,
        bb.max_y.ceil() as i32 - 1,
        bb.max_z.ceil() as i32,
    );
    let mut fluid = false;
    for z in min.z >> 4..=max.z >> 4 {
        for x in min.x >> 4..=max.x >> 4 {
            let Some(chunk) = level.chunk(ChunkPos::new(x, z)) else {
                return false;
            };
            for section in min.y >> 4..=max.y >> 4 {
                if let Some(section) = chunk.section(section) {
                    fluid |= section.has_fluid();
                }
            }
        }
    }
    fluid
}
fn camera_height(
    kind: FlowingFluid,
    fluid: FluidState,
    level: &Level,
    pos: BlockPos,
) -> Result<f32, LevelError> {
    if fluid.source {
        let above = pos.above(1);
        let state = level.get_block_state(above)?;
        if LevelReaderRef::Server(level).is_face_sturdy(state, above, Direction::Down)? {
            return Ok(1.0);
        }
    }
    kind.height(fluid, level, pos)
}

pub fn update_fluid_interaction<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<bool, LevelError> {
    level
        .entity_mut::<EntityData>(entity)?
        .fluid_interaction
        .reset();
    let Some(bb) = behavior.fluid_interaction_box(entity, level)? else {
        return finish(behavior, entity, false, level);
    };
    if !has_fluid_and_loaded(level, bb) {
        return finish(behavior, entity, false, level);
    }
    let base = level.entity::<EntityData>(entity)?;
    let entity_y = base.bounding_box.min_y;
    let eye_y = base.position.y + f64::from(base.eye_height);
    let eye_x = base.block_position.x;
    let eye_z = base.block_position.z;
    let ignore_current = !behavior.is_pushed_by_fluid(entity, level)?;
    let min = BlockPos::containing(bb.min_x, bb.min_y, bb.min_z);
    let max = BlockPos::new(
        bb.max_x.ceil() as i32 - 1,
        bb.max_y.ceil() as i32 - 1,
        bb.max_z.ceil() as i32 - 1,
    );
    let mut any = false;
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                let pos = BlockPos::new(x, y, z);
                let state = level.get_block_state(pos)?;
                let fluid = state.data().fluid;
                if fluid.empty {
                    continue;
                }
                let kind = FlowingFluid::from_id(fluid.fluid_type)
                    .ok_or(LevelError::UnsupportedBlock(state.block()))?;
                let top = f64::from(y) + f64::from(kind.height(fluid, level, pos)?);
                if top < bb.min_y {
                    continue;
                }
                any = true;
                let eyes_inside = x == eye_x
                    && z == eye_z
                    && eye_y >= f64::from(y)
                    && eye_y <= f64::from(y) + f64::from(camera_height(kind, fluid, level, pos)?);
                let current_index = if ignore_current {
                    None
                } else if level
                    .fluid_tags()?
                    .contains_key(FluidTags::WATER, fluid.fluid_type)
                {
                    Some(0)
                } else if level
                    .fluid_tags()?
                    .contains_key(FluidTags::LAVA, fluid.fluid_type)
                {
                    Some(1)
                } else {
                    None
                };
                let flow = if current_index.is_some() {
                    Some(kind.get_flow(fluid, level, pos)?)
                } else {
                    None
                };
                let interaction = &mut level.entity_mut::<EntityData>(entity)?.fluid_interaction;
                let tracker_index = if let Some(index) = interaction
                    .trackers
                    .iter()
                    .position(|t| t.fluid == fluid.fluid_type)
                {
                    index
                } else {
                    interaction.trackers.push(Tracker {
                        fluid: fluid.fluid_type,
                        height: 0.0,
                        eyes_inside: false,
                    });
                    interaction.trackers.len() - 1
                };
                let tracker = &mut interaction.trackers[tracker_index];
                tracker.eyes_inside |= eyes_inside;
                tracker.height = (top - entity_y).max(tracker.height);
                if let Some(index) = current_index {
                    let current = &mut interaction.currents[index];
                    current.height = tracker.height.max(current.height);
                    let mut flow = flow.unwrap();
                    if current.height < 0.4 {
                        flow = flow.scale(current.height);
                    }
                    current.accumulated = current.accumulated + flow;
                    current.count += 1;
                }
            }
        }
    }
    finish(behavior, entity, any, level)
}
fn finish<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    any: bool,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let interaction = &level.entity::<EntityData>(entity)?.fluid_interaction;
    let water = interaction.height(FluidTags::WATER, level)? > 0.0;
    let lava = interaction.height(FluidTags::LAVA, level)? > 0.0;
    if water {
        behavior.reset_fall_distance(entity, level)?;
        let base = level.entity::<EntityData>(entity)?;
        if !base.was_touching_water && !base.first_tick {
            behavior.do_water_splash_effect(entity, level)?;
        }
    }
    level.entity_mut::<EntityData>(entity)?.was_touching_water = water;
    if behavior.is_pushed_by_fluid(entity, level)? {
        if water {
            let current = level
                .entity::<EntityData>(entity)?
                .fluid_interaction
                .currents[0];
            apply_current(entity, current, 0.014, level)?;
        }
        if lava {
            let current = level
                .entity::<EntityData>(entity)?
                .fluid_interaction
                .currents[1];
            apply_current(
                entity,
                current,
                if level.fast_lava()? {
                    0.007
                } else {
                    0.002_333_333_333_333_333_5
                },
                level,
            )?;
        }
    }
    Ok(any)
}
fn apply_current(
    entity: EntityRef,
    current: Current,
    scale: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    if current.count == 0 || current.accumulated.length_sqr() < f64::from(1e-5_f32) {
        return Ok(());
    }
    let mut impulse = if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
        current.accumulated.scale(1.0 / f64::from(current.count))
    } else {
        current.accumulated.normalize()
    };
    impulse = impulse.scale(scale);
    let base = level.entity_mut::<EntityData>(entity)?;
    if base.velocity.x.abs() < 0.003
        && base.velocity.z.abs() < 0.003
        && impulse.length() < 0.004_500_000_000_000_000_5
    {
        impulse = impulse.normalize().scale(0.004_500_000_000_000_000_5);
    }
    base.velocity = base.velocity + impulse;
    Ok(())
}
