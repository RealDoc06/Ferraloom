//! Building block virtuals, registered per method. Unported overrides remain
//! required errors even when another method of the same block is implemented.
use super::{BlockBehavior, BlockMethod, LevelAccessRef};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p, registries::fluid::Id as Fluid};
use ferraloom_util::{BlockPos, Direction};

mod orientation;
#[cfg(test)]
mod tests;

pub(crate) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    if orientation::supports(block, method) {
        return Some(&orientation::ORIENTATION);
    }
    None
}

fn with_facing(state: BlockState, direction: Direction) -> BlockState {
    let facing = match direction {
        Direction::Down => p::Facing::Down,
        Direction::Up => p::Facing::Up,
        Direction::North => p::Facing::North,
        Direction::South => p::Facing::South,
        Direction::West => p::Facing::West,
        Direction::East => p::Facing::East,
    };
    state
        .with(p::FACING, facing)
        .expect("building facing property")
}

/// Port of the waterlogged building blocks' updateShape scheduling. Water's
/// native tick delay is five in every dimension, including worldgen access.
fn schedule_water(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if state.get(p::WATERLOGGED) == Some(true) {
        level.schedule_fluid_tick(pos, Fluid::WATER, 5)?;
    }
    Ok(())
}
