//! PistonMovingBlockEntity's canonical progress and persistent state.
use crate::block::entity::{
    BlockEntity, BlockEntityData, BlockEntityDataView, BlockEntityRef, BlockEntityState,
};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{BlockPos, Direction, mth};

/// Port of PistonMovingBlockEntity. Persisted progress is progressO, including
/// the half-tick visual/movement state at the moment a chunk is saved.
#[derive(Clone, Debug)]
pub struct PistonMovingBlockEntity {
    pub parent: BlockEntityData,
    pub moved_state: BlockState,
    pub direction: Direction,
    pub extending: bool,
    pub is_source_piston: bool,
    pub progress: f32,
    pub progress_o: f32,
    pub last_ticked: i64,
    pub death_ticks: i32,
}
impl PistonMovingBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            moved_state: Block::AIR.default_state(),
            direction: Direction::Down,
            extending: false,
            is_source_piston: false,
            progress: 0.0,
            progress_o: 0.0,
            last_ticked: 0,
            death_ticks: 0,
        }
    }
    pub fn moving(
        parent: BlockEntityData,
        moved_state: BlockState,
        direction: Direction,
        extending: bool,
        is_source_piston: bool,
    ) -> Self {
        Self {
            moved_state,
            direction,
            extending,
            is_source_piston,
            ..Self::new(parent)
        }
    }
    pub fn get_progress(&self, partial: f32) -> f32 {
        mth::lerp_f32(
            if partial > 1.0 { 1.0 } else { partial },
            self.progress_o,
            self.progress,
        )
    }
    pub fn extended_progress(&self, progress: f32) -> f32 {
        if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        }
    }
    pub fn movement_direction(&self) -> Direction {
        if self.extending {
            self.direction
        } else {
            self.direction.opposite()
        }
    }
    pub fn offset(&self, partial: f32) -> ferraloom_util::Vec3 {
        let progress = self.extended_progress(self.get_progress(partial));
        let step = self.direction.step();
        ferraloom_util::Vec3::new(
            f64::from(step.x as f32 * progress),
            f64::from(step.y as f32 * progress),
            f64::from(step.z as f32 * progress),
        )
    }
    pub fn position(&self) -> BlockPos {
        self.parent.pos
    }
}

pub struct PistonMovingEntityBehavior;
pub static MOVING_PISTON_ENTITY: PistonMovingEntityBehavior = PistonMovingEntityBehavior;
impl BlockEntity for PistonMovingEntityBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        if let Some(e) = PistonMovingBlockEntity::view_mut(state) {
            e.moved_state = tag
                .get("blockState")
                .and_then(crate::entity::persistence::load_block_state)
                .unwrap_or(Block::AIR.default_state());
            e.direction = Direction::from_3d_data_value(tag.get_int_or("facing", 0));
            e.progress = tag.get_float_or("progress", 0.0);
            e.progress_o = e.progress;
            e.extending = tag.get_boolean_or("extending", false);
            e.is_source_piston = tag.get_boolean_or("source", false);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let e = PistonMovingBlockEntity::view(state).expect("moving piston dispatcher");
        let mut tag = CompoundTag::new();
        tag.insert(
            "blockState",
            crate::entity::persistence::save_block_state(e.moved_state),
        );
        tag.insert("facing", Tag::Int(e.direction as i32));
        tag.insert("progress", Tag::Float(e.progress_o));
        tag.insert("extending", Tag::Byte(i8::from(e.extending)));
        tag.insert("source", Tag::Byte(i8::from(e.is_source_piston)));
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, entity: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        level.with_block_entity_callback(entity, |level| tick_moving(level, entity))
    }
    fn pre_remove_side_effects(
        &self,
        entity: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        final_tick(level, entity)
    }
}

use super::{
    initial_orientation,
    state::{facing, with_facing},
};
use crate::{
    block::{
        BlockBehavior, BlockCollisionContext, InteractionResult, LevelReaderRef, UPDATE_LIMIT,
        UpdateFlags, UseContext,
    },
    entity::{EntityData, EntityRef, movement::MoverType},
};
use ferraloom_data::{BlockClass, PushReaction, properties as p};
use ferraloom_util::{Aabb, Axis, Vec3, VoxelShape, shapes};

fn collision_related_state(e: &PistonMovingBlockEntity) -> BlockState {
    if !e.extending
        && e.is_source_piston
        && e.moved_state
            .block()
            .is_instance_of(BlockClass::PistonBaseBlock)
    {
        with_facing(
            Block::PISTON_HEAD
                .default_state()
                .with(p::SHORT, e.progress > 0.25)
                .unwrap()
                .with(
                    p::TYPE_,
                    if e.moved_state.block() == Block::STICKY_PISTON {
                        p::Type::Sticky
                    } else {
                        p::Type::Normal
                    },
                )
                .unwrap(),
            facing(e.moved_state),
        )
    } else {
        e.moved_state
    }
}
fn move_by_position_and_progress(pos: BlockPos, aabb: Aabb, e: &PistonMovingBlockEntity) -> Aabb {
    let offset = f64::from(e.extended_progress(e.progress));
    let step = e.direction.step();
    aabb.move_by(
        f64::from(pos.x) + offset * f64::from(step.x),
        f64::from(pos.y) + offset * f64::from(step.y),
        f64::from(pos.z) + offset * f64::from(step.z),
    )
}
/// Port of PistonMath#getMovementArea. This is only the swept leading slab,
/// rather than an expanded full bounding box.
pub fn movement_area(aabb: Aabb, direction: Direction, amount: f64) -> Aabb {
    let delta = amount * f64::from(direction.axis_direction().step());
    let min = ferraloom_util::java::min(delta, 0.0);
    let max = ferraloom_util::java::max(delta, 0.0);
    match direction {
        Direction::West => Aabb::new(
            aabb.min_x + min,
            aabb.min_y,
            aabb.min_z,
            aabb.min_x + max,
            aabb.max_y,
            aabb.max_z,
        ),
        Direction::East => Aabb::new(
            aabb.max_x + min,
            aabb.min_y,
            aabb.min_z,
            aabb.max_x + max,
            aabb.max_y,
            aabb.max_z,
        ),
        Direction::Down => Aabb::new(
            aabb.min_x,
            aabb.min_y + min,
            aabb.min_z,
            aabb.max_x,
            aabb.min_y + max,
            aabb.max_z,
        ),
        Direction::Up => Aabb::new(
            aabb.min_x,
            aabb.max_y + min,
            aabb.min_z,
            aabb.max_x,
            aabb.max_y + max,
            aabb.max_z,
        ),
        Direction::North => Aabb::new(
            aabb.min_x,
            aabb.min_y,
            aabb.min_z + min,
            aabb.max_x,
            aabb.max_y,
            aabb.min_z + max,
        ),
        Direction::South => Aabb::new(
            aabb.min_x,
            aabb.min_y,
            aabb.max_z + min,
            aabb.max_x,
            aabb.max_y,
            aabb.max_z + max,
        ),
    }
}
fn get_movement(outside: Aabb, direction: Direction, entity: Aabb) -> f64 {
    match direction {
        Direction::East => outside.max_x - entity.min_x,
        Direction::West => entity.max_x - outside.min_x,
        Direction::Up => outside.max_y - entity.min_y,
        Direction::Down => entity.max_y - outside.min_y,
        Direction::South => outside.max_z - entity.min_z,
        Direction::North => entity.max_z - outside.min_z,
    }
}
fn move_entity_by_piston(
    level: &mut Level,
    piston_direction: Direction,
    mut entity: EntityRef,
    delta: f64,
    movement: Direction,
) -> Result<(), LevelError> {
    level.region_mut().redstone.piston_noclip = Some(piston_direction);
    let result = (|| {
        let previous = level.entity::<EntityData>(entity)?.position;
        let step = movement.step();
        entity.move_by(
            level,
            MoverType::Piston,
            Vec3::new(
                delta * f64::from(step.x),
                delta * f64::from(step.y),
                delta * f64::from(step.z),
            ),
        )?;
        let current = level.entity::<EntityData>(entity)?.position;
        let behavior = level.entity_behavior(entity)?;
        behavior.apply_effects_from_blocks_between(entity, previous, current, level)?;
        level
            .entity_mut::<EntityData>(entity)?
            .remove_latest_movement_recording();
        Ok(())
    })();
    level.region_mut().redstone.piston_noclip = None;
    result
}
fn fix_entity_within_piston_base(
    level: &mut Level,
    pos: BlockPos,
    entity: EntityRef,
    direction: Direction,
    delta_progress: f64,
) -> Result<(), LevelError> {
    let entity_box = level.entity::<EntityData>(entity)?.bounding_box;
    let base =
        shapes::block()
            .bounds()
            .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    if entity_box.intersects(base) {
        let opposite = direction.opposite();
        let delta = get_movement(base, opposite, entity_box) + 0.01;
        let intersected = get_movement(base, opposite, entity_box.intersect(base)) + 0.01;
        if (delta - intersected).abs() < 0.01 {
            move_entity_by_piston(
                level,
                direction,
                entity,
                ferraloom_util::java::min(delta, delta_progress) + 0.01,
                opposite,
            )?;
        }
    }
    Ok(())
}
fn move_collided_entities(
    level: &mut Level,
    handle: BlockEntityRef,
    new_progress: f32,
) -> Result<(), LevelError> {
    let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
    let pos = handle.pos;
    let movement = e.movement_direction();
    let delta_progress = f64::from(new_progress - e.progress);
    let shape = LevelReaderRef::Server(level)
        .collision_shape_without_context(collision_related_state(e), pos)?;
    if shape.is_empty() {
        return Ok(());
    }
    let moved = move_by_position_and_progress(
        pos,
        shape.bounds(),
        level.block_entity::<PistonMovingBlockEntity>(handle)?,
    );
    let candidates = level.entities_in(
        movement_area(moved, movement, delta_progress).minmax(moved),
        |_, state| !state.as_entity().removed,
    );
    let mut entities = Vec::new();
    for entity in candidates {
        if !level.entity_behavior(entity)?.is_spectator(entity, level)? {
            entities.push(entity);
        }
    }
    if entities.is_empty() {
        return Ok(());
    }
    let shapes = shape.to_aabbs();
    let bounce = level
        .block_entity::<PistonMovingBlockEntity>(handle)?
        .moved_state
        .block()
        == Block::SLIME_BLOCK;
    for entity in entities {
        let behavior = level.entity_behavior(entity)?;
        if behavior.piston_push_reaction(entity, level)? == PushReaction::IgnoreEntity {
            continue;
        }
        if bounce && behavior.can_simulate_movement(entity, level)? {
            let velocity = &mut level.entity_mut::<EntityData>(entity)?.velocity;
            let step = movement.step();
            match movement.axis() {
                Axis::X => velocity.x = f64::from(step.x),
                Axis::Y => velocity.y = f64::from(step.y),
                Axis::Z => velocity.z = f64::from(step.z),
            }
        }
        let mut delta = 0.0_f64;
        for &shape in &shapes {
            let moved = movement_area(
                move_by_position_and_progress(
                    pos,
                    shape,
                    level.block_entity::<PistonMovingBlockEntity>(handle)?,
                ),
                movement,
                delta_progress,
            );
            let entity_box = level.entity::<EntityData>(entity)?.bounding_box;
            if moved.intersects(entity_box) {
                delta = ferraloom_util::java::max(delta, get_movement(moved, movement, entity_box));
                if delta >= delta_progress {
                    break;
                }
            }
        }
        if delta <= 0.0 {
            continue;
        }
        move_entity_by_piston(
            level,
            movement,
            entity,
            ferraloom_util::java::min(delta, delta_progress) + 0.01,
            movement,
        )?;
        let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
        if !e.extending && e.is_source_piston {
            fix_entity_within_piston_base(level, pos, entity, movement, delta_progress)?;
        }
    }
    Ok(())
}
fn move_stuck_entities(
    level: &mut Level,
    handle: BlockEntityRef,
    new_progress: f32,
) -> Result<(), LevelError> {
    let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
    if e.moved_state.block() != Block::HONEY_BLOCK {
        return Ok(());
    }
    let movement = e.movement_direction();
    if movement.axis() == Axis::Y {
        return Ok(());
    }
    let top = LevelReaderRef::Server(level)
        .collision_shape_without_context(e.moved_state, handle.pos)?
        .max(Axis::Y);
    let aabb = move_by_position_and_progress(
        handle.pos,
        Aabb::new(0.0, top, 0.0, 1.0, 1.5000010000000001, 1.0),
        e,
    );
    let delta = f64::from(new_progress - e.progress);
    let candidates = level.entities_in(aabb, |_, _| true);
    let mut matches = Vec::new();
    for entity in candidates {
        let behavior = level.entity_behavior(entity)?;
        let data = level.entity::<EntityData>(entity)?;
        if behavior.piston_push_reaction(entity, level)? == PushReaction::PushPull
            && data.on_ground
            && (data.supporting_block_pos == Some(handle.pos)
                || (data.position.x >= aabb.min_x
                    && data.position.x <= aabb.max_x
                    && data.position.z >= aabb.min_z
                    && data.position.z <= aabb.max_z))
        {
            matches.push(entity);
        }
    }
    for entity in matches {
        move_entity_by_piston(level, movement, entity, delta, movement)?;
    }
    Ok(())
}
/// Port of PistonMovingBlockEntity#finalTick. Pin the original receiver even
/// when replacement callbacks install another generation at this position.
pub fn final_tick(level: &mut Level, handle: BlockEntityRef) -> Result<(), LevelError> {
    level.with_block_entity_callback(handle, |level| {
        let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
        if !e.parent.has_level || e.progress_o >= 1.0 || e.progress_o.is_nan() {
            return Ok(());
        }
        let e = level.block_entity_mut::<PistonMovingBlockEntity>(handle)?;
        e.progress = 1.0;
        e.progress_o = 1.0;
        level.remove_block_entity(handle.pos)?;
        if level.get_block_state(handle.pos)?.block() == Block::MOVING_PISTON {
            let e = level.block_entity::<PistonMovingBlockEntity>(handle)?;
            let new = if e.is_source_piston {
                Block::AIR.default_state()
            } else {
                level.update_from_neighbor_shapes(e.moved_state, handle.pos)?
            };
            level.set_block(handle.pos, new, UpdateFlags::ALL, UPDATE_LIMIT)?;
            let direction = level
                .block_entity::<PistonMovingBlockEntity>(handle)?
                .movement_direction();
            let orientation = initial_orientation(level, Some(direction), None);
            level.neighbor_changed(handle.pos, new.block(), orientation)?;
        }
        Ok(())
    })
}
fn tick_moving(level: &mut Level, handle: BlockEntityRef) -> Result<(), LevelError> {
    let time = level.game_time();
    let e = level.block_entity_mut::<PistonMovingBlockEntity>(handle)?;
    e.last_ticked = time;
    e.progress_o = e.progress;
    if e.progress_o >= 1.0 {
        level.remove_block_entity(handle.pos)?;
        if level.get_block_state(handle.pos)?.block() == Block::MOVING_PISTON {
            let moved = level
                .block_entity::<PistonMovingBlockEntity>(handle)?
                .moved_state;
            let mut new = level.update_from_neighbor_shapes(moved, handle.pos)?;
            if new.is_air() {
                level.set_block(handle.pos, moved, UpdateFlags(340), UPDATE_LIMIT)?;
                level.update_or_destroy(moved, new, handle.pos, UpdateFlags::ALL, UPDATE_LIMIT)?;
            } else {
                if new.get(p::WATERLOGGED) == Some(true) {
                    new = new.with(p::WATERLOGGED, false).unwrap();
                }
                level.set_block(handle.pos, new, UpdateFlags(67), UPDATE_LIMIT)?;
                let direction = level
                    .block_entity::<PistonMovingBlockEntity>(handle)?
                    .movement_direction();
                let orientation = initial_orientation(level, Some(direction), None);
                level.neighbor_changed(handle.pos, new.block(), orientation)?;
            }
        }
    } else {
        let new = e.progress + 0.5;
        move_collided_entities(level, handle, new)?;
        move_stuck_entities(level, handle, new)?;
        level
            .block_entity_mut::<PistonMovingBlockEntity>(handle)?
            .progress = if new >= 1.0 { 1.0 } else { new };
    }
    Ok(())
}
pub(super) fn collision_shape(
    level: &Level,
    handle: BlockEntityRef,
) -> Result<VoxelShape, LevelError> {
    collision_shape_from_reader(&LevelReaderRef::Server(level), handle)
}
fn collision_shape_from_reader(
    reader: &LevelReaderRef<'_, '_>,
    handle: BlockEntityRef,
) -> Result<VoxelShape, LevelError> {
    let e = reader.block_entity::<PistonMovingBlockEntity>(handle)?;
    let base = if !e.extending
        && e.is_source_piston
        && e.moved_state
            .block()
            .is_instance_of(BlockClass::PistonBaseBlock)
    {
        reader.collision_shape_without_context(
            e.moved_state.with(p::EXTENDED, true).unwrap(),
            handle.pos,
        )?
    } else {
        shapes::empty()
    };
    if e.progress < 1.0 && reader.piston_noclip()? == Some(e.movement_direction()) {
        return Ok(base);
    }
    let state = if e.is_source_piston {
        with_facing(
            Block::PISTON_HEAD
                .default_state()
                .with(p::SHORT, e.extending != ((1.0 - e.progress) < 0.25))
                .unwrap(),
            e.direction,
        )
    } else {
        e.moved_state
    };
    let extended = e.extended_progress(e.progress);
    let step = e.direction.step();
    Ok(shapes::or(
        &base,
        &reader
            .collision_shape_without_context(state, handle.pos)?
            .move_by(
                f64::from(step.x as f32 * extended),
                f64::from(step.y as f32 * extended),
                f64::from(step.z as f32 * extended),
            ),
    ))
}
pub struct MovingPistonBlock;
pub static MOVING_PISTON: MovingPistonBlock = MovingPistonBlock;
impl BlockBehavior for MovingPistonBlock {
    fn shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(shapes::empty())
    }
    fn collision_shape(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _context: &BlockCollisionContext<'_>,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        match reader {
            LevelReaderRef::Server(level) => {
                let Some(handle) = level.block_entity_ref(pos)? else {
                    return Ok(shapes::empty());
                };
                if PistonMovingBlockEntity::view(level.block_entity_state(handle)?).is_some() {
                    collision_shape(level, handle)
                } else {
                    Ok(shapes::empty())
                }
            }
            LevelReaderRef::PathNavigation(_) => {
                let Some(handle) = reader.block_entity_ref(pos)? else {
                    return Ok(shapes::empty());
                };
                if PistonMovingBlockEntity::view(reader.block_entity_state(handle)?).is_some() {
                    collision_shape_from_reader(reader, handle)
                } else {
                    Ok(shapes::empty())
                }
            }
            LevelReaderRef::Worldgen(_) => {
                Err(LevelError::UnsupportedBlockEntity(Block::MOVING_PISTON))
            }
        }
    }
    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _reader: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_protocol::component::ItemStack, LevelError> {
        Ok(ferraloom_protocol::component::ItemStack::default())
    }
    fn use_without_item(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        if level.block_entity_ref(pos)?.is_none() {
            level.remove_block(pos, false)?;
            Ok(InteractionResult::CONSUME)
        } else {
            Ok(InteractionResult::Pass)
        }
    }
}
