//! Building block virtuals, registered per method. Unported overrides remain
//! required errors even when another method of the same block is implemented.
use super::{BlockBehavior, BlockMethod, LevelAccessRef};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p, registries::fluid::Id as Fluid};
use ferraloom_util::{BlockPos, Direction};

#[cfg(test)]
mod attachment_tests;
mod attachments;
#[cfg(test)]
mod connected_tests;
mod connections;
mod decorative;
#[cfg(test)]
mod decorative_tests;
mod fire_powder;
#[cfg(test)]
mod fire_powder_tests;
mod orientation;
mod scaffolding;
mod slab;
#[cfg(test)]
mod stair_slab_tests;
mod stairs;
#[cfg(test)]
mod tests;

pub(crate) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    if attachments::supports(block, method) {
        return Some(&attachments::ATTACHMENTS);
    }
    if decorative::supports(block, method) {
        return Some(&decorative::DECORATIVE);
    }
    if fire_powder::supports(block, method) {
        return Some(&fire_powder::FIRE_POWDER);
    }
    if scaffolding::supports(block, method) {
        return Some(&scaffolding::SCAFFOLDING);
    }
    if orientation::supports(block, method) {
        return Some(&orientation::ORIENTATION);
    }
    if stairs::supports(block, method) {
        return Some(&stairs::STAIRS);
    }
    if slab::supports(block, method) {
        return Some(&slab::SLAB);
    }
    if connections::supports(block, method) {
        return Some(&connections::CONNECTIONS);
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

fn facing(state: BlockState) -> Direction {
    match state.get(p::FACING).expect("building facing property") {
        p::Facing::Down => Direction::Down,
        p::Facing::Up => Direction::Up,
        p::Facing::North => Direction::North,
        p::Facing::South => Direction::South,
        p::Facing::West => Direction::West,
        p::Facing::East => Direction::East,
    }
}

/// StairBlock/SlabBlock's click-height rule, preserving the native greater-than
/// comparison for NaN and the exact half-block boundary.
fn top_half(context: &super::BlockPlaceContext<'_>) -> bool {
    let face = context.clicked_face();
    face == Direction::Down
        || (face != Direction::Up
            && context.click_location().y - f64::from(context.clicked_pos().y) > 0.5)
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
