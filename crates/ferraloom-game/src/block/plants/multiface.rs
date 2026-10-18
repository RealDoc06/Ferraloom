//! Port of MultifaceBlock's attachment queries and state transitions.
use super::super::{
    BlockBehavior, BlockMethod, LevelAccessRef, LevelReaderRef, fluid::SimpleWaterloggedBlock,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, FluidState, Item, properties as p, registries::fluid::Id};
use ferraloom_util::{BlockPos, Direction, shapes};

pub struct Multiface;
pub static MULTIFACE: Multiface = Multiface;
pub fn has_face(state: BlockState, face: Direction) -> bool {
    match face {
        Direction::Down => state.get(p::DOWN) == Some(true),
        Direction::Up => state.get(p::UP) == Some(true),
        Direction::North => state.get(p::NORTH) == Some(p::North::True),
        Direction::South => state.get(p::SOUTH) == Some(p::South::True),
        Direction::West => state.get(p::WEST) == Some(p::West::True),
        Direction::East => state.get(p::EAST) == Some(p::East::True),
    }
}
pub fn with_face(state: BlockState, face: Direction, value: bool) -> Option<BlockState> {
    match face {
        Direction::Down => state.with(p::DOWN, value),
        Direction::Up => state.with(p::UP, value),
        Direction::North => state.with(
            p::NORTH,
            if value {
                p::North::True
            } else {
                p::North::False
            },
        ),
        Direction::South => state.with(
            p::SOUTH,
            if value {
                p::South::True
            } else {
                p::South::False
            },
        ),
        Direction::West => state.with(p::WEST, if value { p::West::True } else { p::West::False }),
        Direction::East => state.with(p::EAST, if value { p::East::True } else { p::East::False }),
    }
}
pub fn has_any_face(state: BlockState) -> bool {
    Direction::VALUES
        .into_iter()
        .any(|face| has_face(state, face))
}
/// Port of MultifaceBlock#canAttachTo, using support and collision independently.
pub fn can_attach_to(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    toward: Direction,
) -> Result<bool, LevelError> {
    let neighbor_pos = pos.relative(toward, 1);
    let neighbor = level.get_block_state(neighbor_pos)?;
    can_attach_to_neighbor(level, neighbor, neighbor_pos, toward)
}
pub fn can_attach_to_neighbor(
    level: &LevelReaderRef<'_, '_>,
    neighbor: BlockState,
    pos: BlockPos,
    toward: Direction,
) -> Result<bool, LevelError> {
    let support = level
        .required_behavior(neighbor.block(), BlockMethod::IsFaceSturdy)?
        .support_shape(neighbor, pos, level)?;
    Ok(shapes::is_face_full(&support, toward.opposite())
        || shapes::is_face_full(
            &level.collision_shape_without_context(neighbor, pos)?,
            toward.opposite(),
        ))
}
/// Port of MultifaceBlock#getStateForPlacement(BlockState,BlockGetter,BlockPos,Direction).
pub fn state_for_placement(
    level: &LevelReaderRef<'_, '_>,
    block: Block,
    old: BlockState,
    pos: BlockPos,
    face: Direction,
) -> Result<Option<BlockState>, LevelError> {
    if with_face(block.default_state(), face, true).is_none()
        || (old.block() == block && has_face(old, face))
        || !can_attach_to(level, pos, face)?
    {
        return Ok(None);
    }
    let state = if old.block() == block {
        old
    } else {
        block
            .default_state()
            .with(p::WATERLOGGED, super::source_water(old.data().fluid))
            .expect("multiface waterlogging")
    };
    Ok(with_face(state, face, true))
}
impl BlockBehavior for Multiface {
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let mut any = false;
        for d in Direction::VALUES {
            if has_face(state, d) {
                if !can_attach_to(level, pos, d)? {
                    return Ok(false);
                }
                any = true;
            }
        }
        Ok(any)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        super::survival::schedule_water(state, pos, level)?;
        if !has_any_face(state) {
            return Ok(Block::AIR.default_state());
        }
        if has_face(state, direction)
            && !can_attach_to_neighbor(&level.as_reader(), neighbor, neighbor_pos, direction)?
        {
            let next = with_face(state, direction, false).unwrap();
            return Ok(if has_any_face(next) {
                next
            } else {
                Block::AIR.default_state()
            });
        }
        Ok(state)
    }
    fn can_place_liquid(&self, _state: BlockState, fluid: Id) -> bool {
        fluid == Id::WATER
    }
    fn place_liquid(
        &self,
        state: BlockState,
        pos: BlockPos,
        fluid: FluidState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        SimpleWaterloggedBlock::place_liquid(state, pos, fluid, level)
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        SimpleWaterloggedBlock::pickup_block(state, pos, level)
    }
}
