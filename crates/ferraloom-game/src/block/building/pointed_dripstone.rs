//! PointedDripstoneBlock fluid transfer and inherited speleothem growth.
use super::speleothem::{tip, valid, with_direction, with_tip};
use crate::{
    block::{BlockBehavior, LevelReaderRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockState, properties as p,
    registries::{fluid::Id as Fluid, game_event::Id as GameEvent},
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{
    BlockPos, Direction, Vec3,
    random::RandomSource,
    shapes::{self, BooleanOp},
};

pub(super) struct PointedDripstone;
pub(super) static POINTED_DRIPSTONE: PointedDripstone = PointedDripstone;

/// Port of SpeleothemBlock#findBlockVertical. Test the target before the build
/// height and path gates, and exclude maxSteps itself from the search.
fn find_vertical(
    level: &Level,
    pos: BlockPos,
    direction: Direction,
    max_steps: i32,
    mut path: impl FnMut(BlockPos, BlockState, &Level) -> Result<bool, LevelError>,
    mut target: impl FnMut(BlockState, &Level) -> Result<bool, LevelError>,
) -> Result<Option<BlockPos>, LevelError> {
    let mut at = pos;
    for _ in 1..max_steps {
        at = at.relative(direction, 1);
        let state = level.get_block_state(at)?;
        if target(state, level)? {
            return Ok(Some(at));
        }
        if at.y < level.min_build_height()
            || at.y > level.max_build_height()
            || !path(at, state, level)?
        {
            return Ok(None);
        }
    }
    Ok(None)
}
fn is_tip(state: BlockState, merged: bool, level: &Level) -> Result<bool, LevelError> {
    if !level
        .block_tags()?
        .contains_key(BlockTags::SPELEOTHEMS, state.block())
    {
        return Ok(false);
    }
    let thickness = state
        .get(p::THICKNESS)
        .expect("tagged speleothem thickness");
    Ok(thickness == p::Thickness::Tip || merged && thickness == p::Thickness::TipMerge)
}

fn free_hanging(state: BlockState, level: &Level) -> Result<bool, LevelError> {
    Ok(
        with_direction(state, Direction::Down, &LevelReaderRef::Server(level))?
            && state.get(p::THICKNESS) == Some(p::Thickness::Tip)
            && state.get(p::WATERLOGGED) == Some(false),
    )
}
fn start(state: BlockState, pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    Ok(
        with_direction(state, Direction::Down, &LevelReaderRef::Server(level))?
            && level.get_block_state(pos.above(1))?.block() != state.block(),
    )
}
fn find_tip(
    state: BlockState,
    pos: BlockPos,
    max_steps: i32,
    level: &Level,
) -> Result<Option<BlockPos>, LevelError> {
    if is_tip(state, false, level)? {
        return Ok(Some(pos));
    }
    let direction = tip(state);
    find_vertical(
        level,
        pos,
        direction,
        max_steps,
        |_, next, _| Ok(next.block() == state.block() && tip(next) == direction),
        |next, level| is_tip(next, false, level),
    )
}

#[derive(Clone, Copy)]
struct FluidInfo {
    pos: BlockPos,
    fluid: Fluid,
    source: BlockState,
}
/// Port of PointedDripstoneBlock#getFluidAboveStalactite, including the mud
/// environment read at the captured source position, not at the tip.
fn fluid_above(
    state: BlockState,
    pos: BlockPos,
    level: &Level,
) -> Result<Option<FluidInfo>, LevelError> {
    if !with_direction(state, Direction::Down, &LevelReaderRef::Server(level))? {
        return Ok(None);
    }
    let direction = tip(state);
    let Some(root) = find_vertical(
        level,
        pos,
        direction.opposite(),
        11,
        |_, next, _| Ok(next.block() == state.block() && tip(next) == direction),
        |next, _| Ok(next.block() != state.block()),
    )?
    else {
        return Ok(None);
    };
    let at = root.above(1);
    let source = level.get_block_state(at)?;
    let fluid = if source.block() == Block::MUD && !level.water_evaporates(at)? {
        Fluid::WATER
    } else {
        LevelReaderRef::Server(level)
            .get_fluid_state(at)?
            .fluid_type
    };
    Ok(Some(FluidInfo {
        pos: at,
        fluid,
        source,
    }))
}
/// Port of PointedDripstoneBlock#canDripThrough. The four-pixel central column
/// uses the actual contextual collision shape, after the native cheap gates.
fn can_drip_through(pos: BlockPos, state: BlockState, level: &Level) -> Result<bool, LevelError> {
    if state.is_air() {
        return Ok(true);
    }
    if state.data().solid_render || !state.data().fluid.empty {
        return Ok(false);
    }
    let shape = LevelReaderRef::Server(level).collision_shape_without_context(state, pos)?;
    Ok(!shapes::join_is_not_empty(
        &shapes::block_box(6.0, 0.0, 6.0, 10.0, 16.0, 10.0),
        &shape,
        BooleanOp::And,
    ))
}
/// Native CauldronBlock accepts either fluid; LayeredCauldronBlock accepts
/// water only for RAIN. Full water cauldrons still qualify for the scheduled tick.
fn can_receive_drip(state: BlockState, fluid: Fluid) -> bool {
    state.block() == Block::CAULDRON
        || state.block() == Block::WATER_CAULDRON && fluid == Fluid::WATER
}
/// Port of PointedDripstoneBlock#maybeTransferFluid. A transfer's event happens
/// before reading the cauldron receiver for scheduling. Mud captures its old
/// state, installs clay, then pushes entities, in that order.
pub(super) fn maybe_transfer_fluid(
    state: BlockState,
    pos: BlockPos,
    value: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if value > 0.17578125_f32 || !start(state, pos, level)? {
        return Ok(());
    }
    let Some(source) = fluid_above(state, pos, level)? else {
        return Ok(());
    };
    let probability = match source.fluid {
        Fluid::WATER => 0.17578125_f32,
        Fluid::LAVA => 0.05859375_f32,
        _ => return Ok(()),
    };
    if value >= probability {
        return Ok(());
    }
    let Some(tip_pos) = find_tip(state, pos, 11, level)? else {
        return Ok(());
    };
    if source.source.block() == Block::MUD && source.fluid == Fluid::WATER {
        let clay = Block::CLAY.default_state();
        level.set_block_and_update(source.pos, clay)?;
        level.push_entities_up(source.source, clay, source.pos)?;
        level.game_event(
            GameEvent::BLOCK_CHANGE,
            Vec3::at_center_of(source.pos.into()),
            None,
            Some(clay),
        )?;
        level.level_event(None, 1504, tip_pos, 0)?;
    } else if let Some(cauldron) = find_vertical(
        level,
        tip_pos,
        Direction::Down,
        11,
        can_drip_through,
        |state, _| Ok(can_receive_drip(state, source.fluid)),
    )? {
        level.level_event(None, 1504, tip_pos, 0)?;
        let delay = 50 + tip_pos.y - cauldron.y;
        let block = level.get_block_state(cauldron)?.block();
        level.schedule_block_tick_after(cauldron, block, delay)?;
    }
    Ok(())
}
fn unmerged_tip(
    state: BlockState,
    direction: Direction,
    block: Block,
    level: &Level,
) -> Result<bool, LevelError> {
    Ok(is_tip(state, false, level)? && tip(state) == direction && state.block() == block)
}
fn can_tip_grow(
    state: BlockState,
    pos: BlockPos,
    block: Block,
    level: &Level,
) -> Result<bool, LevelError> {
    let direction = tip(state);
    let next = level.get_block_state(pos.relative(direction, 1))?;
    if !next.data().fluid.empty {
        return Ok(false);
    }
    Ok(next.is_air() || unmerged_tip(next, direction.opposite(), block, level)?)
}
fn create(
    block: Block,
    pos: BlockPos,
    direction: Direction,
    thickness: p::Thickness,
    level: &mut Level,
) -> Result<(), LevelError> {
    let state = with_tip(block.default_state(), direction)
        .with(p::THICKNESS, thickness)
        .unwrap()
        .with(
            p::WATERLOGGED,
            LevelReaderRef::Server(level)
                .get_fluid_state(pos)?
                .fluid_type
                == Fluid::WATER,
        )
        .unwrap();
    level.set_block_and_update(pos, state)?;
    Ok(())
}
fn grow(
    block: Block,
    pos: BlockPos,
    direction: Direction,
    level: &mut Level,
) -> Result<(), LevelError> {
    let target = pos.relative(direction, 1);
    let state = level.get_block_state(target)?;
    if unmerged_tip(state, direction.opposite(), block, level)? {
        let stalactite = if tip(state) == Direction::Up {
            target.above(1)
        } else {
            target
        };
        create(
            block,
            stalactite,
            Direction::Down,
            p::Thickness::TipMerge,
            level,
        )?;
        create(
            block,
            stalactite.below(1),
            Direction::Up,
            p::Thickness::TipMerge,
            level,
        )?;
    } else if state.is_air() || state.block() == Block::WATER {
        create(block, target, direction, p::Thickness::Tip, level)?;
    }
    Ok(())
}
fn grow_stalagmite(block: Block, tip_pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    let mut at = tip_pos;
    for _ in 0..10 {
        at = at.below(1);
        let state = level.get_block_state(at)?;
        if !state.data().fluid.empty {
            return Ok(());
        }
        if unmerged_tip(state, Direction::Up, block, level)?
            && can_tip_grow(state, at, block, level)?
        {
            return grow(block, at, Direction::Up, level);
        }
        if valid(block, at, Direction::Up, &LevelReaderRef::Server(level))?
            && !level.fluid_tags()?.contains_key(
                FluidTags::WATER,
                LevelReaderRef::Server(level)
                    .get_fluid_state(at.below(1))?
                    .fluid_type,
            )
        {
            return grow(block, at.below(1), Direction::Up, level);
        }
        if !can_drip_through(at, state, level)? {
            return Ok(());
        }
    }
    Ok(())
}
/// Port of PointedDripstoneBlock#canGrow and
/// SpeleothemBlock#growStalactiteOrStalagmiteIfPossible. Blocks#register supplies
/// DRIPSTONE_BLOCK as this receiver's substrate; its inherited search limit is 7.
pub(super) fn grow_if_possible(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    let water = level.get_block_state(pos.above(2))?.data().fluid;
    if level.get_block_state(pos.above(1))?.block() != Block::DRIPSTONE_BLOCK
        || water.fluid_type != Fluid::WATER
        || !water.source
    {
        return Ok(());
    }
    let Some(at) = find_tip(state, pos, 7, level)? else {
        return Ok(());
    };
    let current = level.get_block_state(at)?;
    if free_hanging(current, level)? && can_tip_grow(current, at, state.block(), level)? {
        if level.random().next_boolean() {
            grow(state.block(), at, Direction::Down, level)?;
        } else {
            grow_stalagmite(state.block(), at, level)?;
        }
    }
    Ok(())
}
impl BlockBehavior for PointedDripstone {
    /// Port of PointedDripstoneBlock#randomTick then SpeleothemBlock#randomTick.
    /// The growth draw is unconditional after successful fluid-transfer handling.
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let transfer_roll = level.random().next_float();
        maybe_transfer_fluid(state, pos, transfer_roll, level)?;
        if level.random().next_float() < 0.011377778_f32 && start(state, pos, level)? {
            grow_if_possible(state, pos, level)?;
        }
        Ok(())
    }
}
