//! Vanilla BlockGetter traversal used by movement traces and inside-block effects.
use super::{EntityData, EntityRef};
use crate::{
    block::fluid::FlowingFluid,
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, EntityType,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{
    Aabb, Axis, BlockPos, Direction, Vec3, collections::LongOpenHashSet, mth, shapes,
};

/// Port of BlockGetter#traverseBlocks. Ties advance Z before Y before X.
pub fn traverse_blocks(
    from: Vec3,
    to: Vec3,
    mut visit: impl FnMut(BlockPos) -> Result<bool, LevelError>,
) -> Result<bool, LevelError> {
    if from == to {
        return Ok(false);
    }
    let end = Vec3::new(
        mth::lerp(-1e-7, to.x, from.x),
        mth::lerp(-1e-7, to.y, from.y),
        mth::lerp(-1e-7, to.z, from.z),
    );
    let begin = Vec3::new(
        mth::lerp(-1e-7, from.x, to.x),
        mth::lerp(-1e-7, from.y, to.y),
        mth::lerp(-1e-7, from.z, to.z),
    );
    let delta = end - begin;
    let mut cell = [
        mth::floor(begin.x),
        mth::floor(begin.y),
        mth::floor(begin.z),
    ];
    if visit(BlockPos::new(cell[0], cell[1], cell[2]))? {
        return Ok(true);
    }
    let (sign, step, mut next) = traversal_times(begin, delta);
    while next.iter().any(|&value| value <= 1.0) {
        let axis = next_axis(next);
        cell[axis] += sign[axis];
        next[axis] += step[axis];
        if visit(BlockPos::new(cell[0], cell[1], cell[2]))? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn traversal_times(from: Vec3, delta: Vec3) -> ([i32; 3], [f64; 3], [f64; 3]) {
    let starts = [from.x, from.y, from.z];
    let values = [delta.x, delta.y, delta.z];
    let sign = values.map(mth::sign);
    let step = std::array::from_fn(|i| {
        if sign[i] == 0 {
            f64::MAX
        } else {
            f64::from(sign[i]) / values[i]
        }
    });
    let next = std::array::from_fn(|i| {
        step[i]
            * if sign[i] > 0 {
                1.0 - mth::frac(starts[i])
            } else {
                mth::frac(starts[i])
            }
    });
    (sign, step, next)
}
fn next_axis(next: [f64; 3]) -> usize {
    if next[0] < next[1] {
        if next[0] < next[2] { 0 } else { 2 }
    } else if next[1] < next[2] {
        1
    } else {
        2
    }
}

pub fn fall_distance_resetting_hit(
    entity: EntityRef,
    movement: Vec3,
    level: &Level,
) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let from = base.position;
    let to = from + movement.normalize().scale(movement.length().min(8.0));
    let player = base.entity_type == EntityType::PLAYER;
    traverse_blocks(from, to, |pos| {
        let state = level.get_block_state(pos)?;
        let resets = level
            .block_tags()?
            .contains_key(BlockTags::FALL_DAMAGE_RESETTING, state.block())
            || player && matches!(state.block(), Block::END_GATEWAY | Block::END_PORTAL);
        if resets && shapes::block().clip(from, to, pos).is_some() {
            return Ok(true);
        }
        let fluid = state.data().fluid;
        if !fluid.empty
            && level
                .fluid_tags()?
                .contains_key(FluidTags::WATER, fluid.fluid_type)
        {
            let kind = FlowingFluid::from_id(fluid.fluid_type).expect("vanilla water fluid");
            let height = kind.height(fluid, level, pos)?;
            if shapes::box_shape(0.0, 0.0, 0.0, 1.0, f64::from(height), 1.0)
                .clip(from, to, pos)
                .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

/// Port of BlockGetter#forEachBlockIntersectedBetween. Visitor false stops the
/// traversal immediately, before any subsequent world reads or callbacks.
pub fn for_each_intersected_block(
    from: Vec3,
    to: Vec3,
    bb: Aabb,
    mut visit: impl FnMut(BlockPos, i32) -> Result<bool, LevelError>,
) -> Result<(), LevelError> {
    let travel = to - from;
    if travel.length_sqr() < f64::from(1e-5_f32 * 1e-5_f32) {
        let min = BlockPos::containing(bb.min_x, bb.min_y, bb.min_z);
        let max = BlockPos::containing(bb.max_x, bb.max_y, bb.max_z);
        for pos in BlockPos::between_closed(min, max) {
            if !visit(pos, 0)? {
                break;
            }
        }
        return Ok(());
    }
    let mut visited = LongOpenHashSet::new();
    let start = bb.move_vec(-travel);
    if !between_in_direction(start.min_position(), start.max_position(), travel, |pos| {
        if !visit(pos, 0)? {
            return Ok(false);
        }
        visited.insert(pos.as_long());
        Ok(true)
    })? {
        return Ok(());
    }
    let size = Vec3::new(bb.x_size(), bb.y_size(), bb.z_size());
    let corner = furthest_corner(travel);
    let center = bb.center();
    let end = Vec3::new(
        center.x + size.x * 0.5 * corner[0],
        center.y + size.y * 0.5 * corner[1],
        center.z + size.z * 0.5 * corner[2],
    );
    let begin = end - travel;
    let mut cell = [
        mth::floor(begin.x),
        mth::floor(begin.y),
        mth::floor(begin.z),
    ];
    let (sign, step, mut next) = traversal_times(begin, travel);
    let mut iterations = 0;
    while next.iter().any(|&value| value <= 1.0) {
        let axis = next_axis(next);
        cell[axis] += sign[axis];
        next[axis] += step[axis];
        let pos = BlockPos::new(cell[0], cell[1], cell[2]);
        if let Some(hit) = Aabb::from_block(pos).clip(begin, end) {
            iterations += 1;
            let epsilon = f64::from(1e-5_f32);
            let hit = Vec3::new(
                mth::clamp(
                    hit.x,
                    f64::from(cell[0]) + epsilon,
                    f64::from(cell[0]) + 1.0 - epsilon,
                ),
                mth::clamp(
                    hit.y,
                    f64::from(cell[1]) + epsilon,
                    f64::from(cell[1]) + 1.0 - epsilon,
                ),
                mth::clamp(
                    hit.z,
                    f64::from(cell[2]) + epsilon,
                    f64::from(cell[2]) + 1.0 - epsilon,
                ),
            );
            let opposite = Vec3::new(
                f64::from(mth::floor(hit.x - size.x * corner[0])),
                f64::from(mth::floor(hit.y - size.y * corner[1])),
                f64::from(mth::floor(hit.z - size.z * corner[2])),
            );
            if !between_in_direction(
                Vec3::new(f64::from(cell[0]), f64::from(cell[1]), f64::from(cell[2])),
                opposite,
                travel,
                |pos| {
                    if visited.insert(pos.as_long()) {
                        visit(pos, iterations)
                    } else {
                        Ok(true)
                    }
                },
            )? {
                return Ok(());
            }
        }
    }
    between_in_direction(bb.min_position(), bb.max_position(), travel, |pos| {
        if visited.insert(pos.as_long()) {
            visit(pos, iterations + 1)
        } else {
            Ok(true)
        }
    })?;
    Ok(())
}
fn furthest_corner(v: Vec3) -> [f64; 3] {
    let sx = if v.x >= 0.0 { 1.0 } else { -1.0 };
    let sy = if v.y >= 0.0 { 1.0 } else { -1.0 };
    let sz = if v.z >= 0.0 { 1.0 } else { -1.0 };
    if v.x.abs() <= v.y.abs() && v.x.abs() <= v.z.abs() {
        [-sx, -sz, sy]
    } else if v.y.abs() <= v.z.abs() {
        [sz, -sy, -sx]
    } else {
        [-sy, sx, -sz]
    }
}
fn between_in_direction(
    first: Vec3,
    second: Vec3,
    direction: Vec3,
    mut visit: impl FnMut(BlockPos) -> Result<bool, LevelError>,
) -> Result<bool, LevelError> {
    let a = [
        mth::floor(first.x),
        mth::floor(first.y),
        mth::floor(first.z),
    ];
    let b = [
        mth::floor(second.x),
        mth::floor(second.y),
        mth::floor(second.z),
    ];
    let min = std::array::from_fn::<_, 3, _>(|i| a[i].min(b[i]));
    let max = std::array::from_fn::<_, 3, _>(|i| a[i].max(b[i]));
    let values = [direction.x, direction.y, direction.z];
    let start = std::array::from_fn::<_, 3, _>(|i| if values[i] >= 0.0 { min[i] } else { max[i] });
    let axes = Direction::axis_step_order(direction).map(|axis| match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    });
    for first in 0..=max[axes[0]] - min[axes[0]] {
        for second in 0..=max[axes[1]] - min[axes[1]] {
            for third in 0..=max[axes[2]] - min[axes[2]] {
                let mut pos = start;
                for (axis, index) in axes.into_iter().zip([first, second, third]) {
                    pos[axis] += if values[axis] >= 0.0 { index } else { -index };
                }
                if !visit(BlockPos::new(pos[0], pos[1], pos[2]))? {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
