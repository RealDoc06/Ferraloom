//! Pointed dripstone and sulfur spike placement, thickness and support updates.
use super::schedule_water;
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
    tags::BlockTags,
};
use ferraloom_util::{BlockPos, Direction, VoxelShape, mth, shapes};
pub(super) struct Speleothem;
pub(super) static SPELEOTHEM: Speleothem = Speleothem;
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(
        block.class(),
        C::PointedDripstoneBlock | C::SulfurSpikeBlock
    ) && matches!(
        method,
        BlockMethod::GetStateForPlacement
            | BlockMethod::CanSurvive
            | BlockMethod::UpdateShape
            | BlockMethod::UpdateFromNeighbourShapes
            | BlockMethod::OutlineShape
            | BlockMethod::IsCollisionShapeFullBlock
    )
}
fn tip(state: BlockState) -> Direction {
    if state.get(p::VERTICAL_DIRECTION) == Some(p::VerticalDirection::Up) {
        Direction::Up
    } else {
        Direction::Down
    }
}
fn with_tip(state: BlockState, direction: Direction) -> BlockState {
    state
        .with(
            p::VERTICAL_DIRECTION,
            if direction == Direction::Up {
                p::VerticalDirection::Up
            } else {
                p::VerticalDirection::Down
            },
        )
        .unwrap()
}
fn with_direction(
    state: BlockState,
    direction: Direction,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    Ok(level
        .block_tags()?
        .contains_key(BlockTags::SPELEOTHEMS, state.block())
        && tip(state) == direction)
}
fn valid(
    block: Block,
    pos: BlockPos,
    direction: Direction,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let at = pos.relative(direction.opposite(), 1);
    let behind = level.get_block_state(at)?;
    Ok(level.is_face_sturdy(behind, at, direction)?
        || with_direction(behind, direction, level)? && behind.block() == block)
}
/// Port of SpeleothemBlock#calculateSpeleothemThickness. Opposing tips require
/// the same concrete block; the same-direction branch uses the live shared tag.
fn thickness(
    block: Block,
    pos: BlockPos,
    direction: Direction,
    merge: bool,
    level: &LevelReaderRef<'_, '_>,
) -> Result<p::Thickness, LevelError> {
    let front = level.get_block_state(pos.relative(direction, 1))?;
    if with_direction(front, direction.opposite(), level)? && front.block() == block {
        return Ok(
            if !merge && front.get(p::THICKNESS) != Some(p::Thickness::TipMerge) {
                p::Thickness::Tip
            } else {
                p::Thickness::TipMerge
            },
        );
    }
    if !with_direction(front, direction, level)? {
        return Ok(p::Thickness::Tip);
    }
    if matches!(
        front.get(p::THICKNESS),
        Some(p::Thickness::Tip | p::Thickness::TipMerge)
    ) {
        return Ok(p::Thickness::Frustum);
    }
    Ok(
        if with_direction(
            level.get_block_state(pos.relative(direction.opposite(), 1))?,
            direction,
            level,
        )? {
            p::Thickness::Middle
        } else {
            p::Thickness::Base
        },
    )
}
fn scheduled(
    block: Block,
    pos: BlockPos,
    level: &LevelAccessRef<'_, '_>,
) -> Result<bool, LevelError> {
    match level {
        LevelAccessRef::Server(level) => Ok(level.has_scheduled_block_tick(pos, block)),
        LevelAccessRef::Worldgen(region) => Ok(region
            .chunk(
                [pos.x >> 4, pos.z >> 4],
                ferraloom_worldgen::region::ChunkStatus::Empty,
            )
            .map_err(|_| LevelError::UnsupportedWorldgenEnvironment)?
            .block_ticks
            .has_scheduled_tick([pos.x, pos.y, pos.z], &block)),
    }
}
impl BlockBehavior for Speleothem {
    /// Port of SpeleothemBlock#getShape and its XZ offset function. Its native
    /// max horizontal offset is SHAPE_BASE.min(X), exactly one eighth.
    fn shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        let (width, min_y, max_y) = match state.get(p::THICKNESS).unwrap() {
            p::Thickness::TipMerge => (6.0, 0.0, 16.0),
            p::Thickness::Tip if tip(state) == Direction::Down => (6.0, 5.0, 16.0),
            p::Thickness::Tip => (6.0, 0.0, 11.0),
            p::Thickness::Frustum => (8.0, 0.0, 16.0),
            p::Thickness::Middle => (10.0, 0.0, 16.0),
            p::Thickness::Base => (12.0, 0.0, 16.0),
        };
        let inset = (16.0 - width) / 32.0;
        let seed = mth::get_seed(pos.x, 0, pos.z);
        let x = ((f64::from((seed & 15) as f32 / 15.0) - 0.5) * 0.5).clamp(-0.125, 0.125);
        let z = ((f64::from(((seed >> 8) & 15) as f32 / 15.0) - 0.5) * 0.5).clamp(-0.125, 0.125);
        Ok(shapes::box_shape(
            inset,
            min_y / 16.0,
            inset,
            1.0 - inset,
            max_y / 16.0,
            1.0 - inset,
        )
        .move_by(x, 0.0, z))
    }
    fn is_collision_shape_full_block(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let preferred = context
            .nearest_looking_vertical_direction(&reader)?
            .opposite();
        let direction = if valid(block, pos, preferred, &reader)? {
            preferred
        } else if valid(block, pos, preferred.opposite(), &reader)? {
            preferred.opposite()
        } else {
            return Ok(None);
        };
        let thickness = thickness(
            block,
            pos,
            direction,
            !context.is_secondary_use_active(&reader)?,
            &reader,
        )?;
        Ok(Some(
            with_tip(block.default_state(), direction)
                .with(p::THICKNESS, thickness)
                .unwrap()
                .with(
                    p::WATERLOGGED,
                    reader.get_fluid_state(pos)?.fluid_type == Fluid::WATER,
                )
                .unwrap(),
        ))
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        valid(state.block(), pos, tip(state), level)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        schedule_water(state, pos, level)?;
        if direction.axis().is_horizontal() {
            return Ok(state);
        }
        let tip = tip(state);
        if tip == Direction::Down && scheduled(state.block(), pos, level)? {
            return Ok(state);
        }
        if direction == tip.opposite() && !self.can_survive(state, pos, &level.as_reader())? {
            level.schedule_block_tick(
                pos,
                state.block(),
                if tip == Direction::Down { 2 } else { 1 },
            )?;
            return Ok(state);
        }
        Ok(state
            .with(
                p::THICKNESS,
                thickness(
                    state.block(),
                    pos,
                    tip,
                    state.get(p::THICKNESS) == Some(p::Thickness::TipMerge),
                    &level.as_reader(),
                )?,
            )
            .unwrap())
    }
}
