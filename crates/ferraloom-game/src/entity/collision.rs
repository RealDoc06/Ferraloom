//! Entity#collide and CollisionGetter/BlockCollisions, using concrete Level reads.
use super::{EntityData, EntityRef};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    Aabb, Axis, BlockPos, Direction, Vec3, mth,
    shapes::{self, BooleanOp, VoxelShape},
};

/// Block behavior owns all static and contextual collision rules.
pub fn block_collision_shape(
    state: BlockState,
    pos: BlockPos,
    source: Option<EntityRef>,
    level: &Level,
) -> Result<VoxelShape, LevelError> {
    let context = collision_context(source, level, false)?;
    level.collision_shape(state, pos, &context)
}
fn collision_context(
    source: Option<EntityRef>,
    level: &Level,
    liquids: bool,
) -> Result<crate::block::BlockCollisionContext<'static>, LevelError> {
    Ok(match source {
        Some(id) => crate::block::BlockCollisionContext::Entity(
            super::collision_context::EntityCollisionView::capture(id, level, liquids)?,
        ),
        None => crate::block::BlockCollisionContext::empty_with_fluid_collisions(liquids),
    })
}
fn has_large_collision_shape(state: BlockState) -> bool {
    state.has_large_collision_shape().unwrap_or(true)
}

/// Port of BlockCollisions#computeNext. Cursor3D visits X, then Y, then Z.
pub fn block_collisions(
    level: &Level,
    source: Option<EntityRef>,
    bb: Aabb,
) -> Result<Vec<(BlockPos, VoxelShape)>, LevelError> {
    let context = collision_context(source, level, false)?;
    block_collisions_from_context(level, &context, bb)
}
pub fn block_collisions_from_context(
    level: &Level,
    context: &crate::block::BlockCollisionContext<'_>,
    bb: Aabb,
) -> Result<Vec<(BlockPos, VoxelShape)>, LevelError> {
    let mut result = Vec::new();
    let _ = visit_block_collisions(
        &crate::block::LevelReaderRef::Server(level),
        context,
        bb,
        |pos, shape| {
            result.push((pos, shape));
            std::ops::ControlFlow::Continue(())
        },
    )?;
    Ok(result)
}

/// Keep BlockCollisions lazy: noCollision stops at the first occupied shape,
/// before any later block or ownership query can have an observable result.
pub(crate) fn visit_block_collisions(
    reader: &crate::block::LevelReaderRef<'_, '_>,
    context: &crate::block::BlockCollisionContext<'_>,
    bb: Aabb,
    mut visitor: impl FnMut(BlockPos, VoxelShape) -> std::ops::ControlFlow<()>,
) -> Result<std::ops::ControlFlow<()>, LevelError> {
    let min = BlockPos::new(
        mth::floor(bb.min_x - 1e-7) - 1,
        mth::floor(bb.min_y - 1e-7) - 1,
        mth::floor(bb.min_z - 1e-7) - 1,
    );
    let max = BlockPos::new(
        mth::floor(bb.max_x + 1e-7) + 1,
        mth::floor(bb.max_y + 1e-7) + 1,
        mth::floor(bb.max_z + 1e-7) + 1,
    );
    let entity_shape = shapes::create_aabb(bb);
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let faces = u8::from(x == min.x || x == max.x)
                    + u8::from(y == min.y || y == max.y)
                    + u8::from(z == min.z || z == max.z);
                if faces == 3 {
                    continue;
                }
                let pos = BlockPos::new(x, y, z);
                let state = reader.block_state_for_collisions(pos)?;
                if (faces == 1 && !has_large_collision_shape(state))
                    || (faces == 2 && state.block() != Block::MOVING_PISTON)
                {
                    continue;
                }
                let shape = reader.collision_shape(state, pos, context)?.move_by(
                    f64::from(x),
                    f64::from(y),
                    f64::from(z),
                );
                if !shape.is_empty()
                    && shapes::join_is_not_empty(&shape, &entity_shape, BooleanOp::And)
                    && visitor(pos, shape).is_break()
                {
                    return Ok(std::ops::ControlFlow::Break(()));
                }
            }
        }
    }
    Ok(std::ops::ControlFlow::Continue(()))
}

pub fn entity_collisions(
    level: &Level,
    source: Option<EntityRef>,
    bb: Aabb,
) -> Result<Vec<VoxelShape>, LevelError> {
    if bb.size() < 1e-7 {
        return Ok(Vec::new());
    }
    let mut colliders = Vec::new();
    for id in level.entities_in(bb.inflate_uniform(1e-7), |id, state| {
        Some(id) != source && !state.as_entity().removed
    }) {
        let behavior = level.entity_behavior(id)?;
        if behavior.can_be_collided_with(id, source, level)? {
            if let Some(source) = source
                && same_vehicle(source, id, level)?
            {
                continue;
            }
            colliders.push(shapes::create_aabb(
                level.entity::<EntityData>(id)?.bounding_box,
            ));
        }
    }
    Ok(colliders)
}
pub(crate) fn same_vehicle(a: EntityRef, b: EntityRef, level: &Level) -> Result<bool, LevelError> {
    fn root(mut id: EntityRef, level: &Level) -> Result<EntityRef, LevelError> {
        while let Some(vehicle) = level.entity::<EntityData>(id)?.vehicle.get() {
            id = vehicle;
        }
        Ok(id)
    }
    Ok(root(a, level)? == root(b, level)?)
}

pub fn collect_colliders(
    level: &Level,
    source: Option<EntityRef>,
    bb: Aabb,
    entities: &[VoxelShape],
) -> Result<Vec<VoxelShape>, LevelError> {
    let mut result = entities.to_vec();
    if let Some(id) = source {
        let border = level.entity_world_border();
        if border.is_inside_close_to_border(level.entity::<EntityData>(id)?.position, bb) {
            result.push(border.collision_shape());
        }
    }
    result.extend(
        block_collisions(level, source, bb)?
            .into_iter()
            .map(|(_, shape)| shape),
    );
    Ok(result)
}
pub fn no_collision(
    level: &Level,
    source: Option<EntityRef>,
    bb: Aabb,
) -> Result<bool, LevelError> {
    level.no_collision(source, bb, false)
}

/// Port of Entity#collideWithShapes. Axis order depends on the original movement.
pub fn collide_with_shapes(movement: Vec3, bb: Aabb, colliders: &[VoxelShape]) -> Vec3 {
    if colliders.is_empty() {
        return movement;
    }
    let mut resolved = Vec3::ZERO;
    for axis in Direction::axis_step_order(movement) {
        let distance = movement.get(axis);
        if distance != 0.0 {
            resolved = resolved.with(
                axis,
                shapes::collide(axis, bb.move_vec(resolved), colliders, distance),
            );
        }
    }
    resolved
}

/// Port of Entity#collectCandidateStepUpHeights, including the float conversion.
pub fn candidate_step_heights(bb: Aabb, colliders: &[VoxelShape], max: f32, skip: f32) -> Vec<f32> {
    let mut candidates = Vec::new();
    for shape in colliders {
        let coords = shape.coords(Axis::Y);
        for i in 0..coords.len() {
            let relative = (coords.get(i) - bb.min_y) as f32;
            if relative < 0.0 || relative == skip {
                continue;
            }
            if relative > max {
                break;
            }
            if !candidates.contains(&relative) {
                candidates.push(relative);
            }
        }
    }
    candidates.sort_unstable_by(f32::total_cmp);
    candidates
}

/// Port of Entity#collide, with the first improving sorted step candidate winning.
pub fn collide(
    entity: EntityRef,
    movement: Vec3,
    max_up_step: f32,
    level: &Level,
) -> Result<Vec3, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let bb = base.bounding_box;
    let grounded = base.on_ground;
    let entities = entity_collisions(
        level,
        Some(entity),
        bb.expand_towards(movement.x, movement.y, movement.z)
            .expand_towards(0.0, f64::from(max_up_step), 0.0),
    )?;
    let clipped = if movement.length_sqr() == 0.0 {
        movement
    } else {
        collide_with_shapes(
            movement,
            bb,
            &collect_colliders(
                level,
                Some(entity),
                bb.expand_towards(movement.x, movement.y, movement.z),
                &entities,
            )?,
        )
    };
    let landing = movement.y != clipped.y && movement.y < 0.0;
    if max_up_step > 0.0
        && (landing || grounded)
        && (movement.x != clipped.x || movement.z != clipped.z)
    {
        let grounded_box = if landing {
            bb.move_by(0.0, clipped.y, 0.0)
        } else {
            bb
        };
        let mut step_box =
            grounded_box.expand_towards(movement.x, f64::from(max_up_step), movement.z);
        if !landing {
            step_box = step_box.expand_towards(0.0, -f64::from(1e-5_f32), 0.0);
        }
        let colliders = collect_colliders(level, Some(entity), step_box, &entities)?;
        for height in
            candidate_step_heights(grounded_box, &colliders, max_up_step, clipped.y as f32)
        {
            let step = collide_with_shapes(
                Vec3::new(movement.x, f64::from(height), movement.z),
                grounded_box,
                &colliders,
            );
            if step.horizontal_distance_sqr() > clipped.horizontal_distance_sqr() {
                return Ok(step.subtract_xyz(0.0, bb.min_y - grounded_box.min_y, 0.0));
            }
        }
    }
    Ok(clipped)
}

/// Port of CollisionGetter#findSupportingBlock: equal distances prefer larger Y/Z/X.
pub fn find_supporting_block(
    entity: EntityRef,
    bb: Aabb,
    level: &Level,
) -> Result<Option<BlockPos>, LevelError> {
    let position = level.entity::<EntityData>(entity)?.position;
    let mut best = None;
    let mut best_distance = f64::MAX;
    for (pos, _) in block_collisions(level, Some(entity), bb)? {
        let distance = position.distance_to_sqr(Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        ));
        if distance < best_distance
            || (distance == best_distance
                && best.is_none_or(|old: BlockPos| (old.y, old.z, old.x) < (pos.y, pos.z, pos.x)))
        {
            best = Some(pos);
            best_distance = distance;
        }
    }
    Ok(best)
}

impl Level {
    /// Port of CollisionGetter#noCollision. Required shape/ownership failures
    /// propagate even when optional unported block behavior is configured Warn.
    pub fn no_collision(
        &self,
        source: Option<EntityRef>,
        bb: Aabb,
        include_liquids: bool,
    ) -> Result<bool, LevelError> {
        let context = collision_context(source, self, include_liquids)?;
        if visit_block_collisions(
            &crate::block::LevelReaderRef::Server(self),
            &context,
            bb,
            |_, _| std::ops::ControlFlow::Break(()),
        )?
        .is_break()
            || !entity_collisions(self, source, bb)?.is_empty()
        {
            return Ok(false);
        }
        if let Some(id) = source {
            let border = self.entity_world_border();
            if border.is_inside_close_to_border(self.entity::<EntityData>(id)?.position, bb)
                && shapes::join_is_not_empty(
                    &border.collision_shape(),
                    &shapes::create_aabb(bb),
                    BooleanOp::And,
                )
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Port of CollisionGetter#findFreePosition, with the constructor's captured
/// context available before the new entity enters world lookup/tracking.
pub(crate) fn find_free_position_from_context(
    level: &Level,
    context: &crate::block::BlockCollisionContext<'_>,
    allowed: &VoxelShape,
    preferred: Vec3,
    size: Vec3,
) -> Result<Option<Vec3>, LevelError> {
    if allowed.is_empty() {
        return Ok(None);
    }
    let search = allowed.bounds().inflate(size.x, size.y, size.z);
    let mut expanded = shapes::empty();
    for (_, shape) in block_collisions_from_context(level, context, search)? {
        if level.entity_world_border().is_within_aabb(shape.bounds()) {
            for bounds in shape.to_aabbs() {
                expanded = shapes::or(
                    &expanded,
                    &shapes::create_aabb(bounds.inflate(size.x / 2.0, size.y / 2.0, size.z / 2.0)),
                );
            }
        }
    }
    Ok(shapes::join(allowed, &expanded, BooleanOp::OnlyFirst).closest_point_to(preferred))
}
