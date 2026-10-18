//! Ports of living coral's water scan, delayed death and dead-state conversion.
use super::super::{BlockBehavior, BlockPlaceContext, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p, tags::FluidTags};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};
pub struct Coral;
pub static CORAL: Coral = Coral;
fn scan_water(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    if state.get(p::WATERLOGGED) == Some(true) {
        return Ok(true);
    }
    for d in Direction::VALUES {
        if level.fluid_tags()?.contains_key(
            FluidTags::WATER,
            level.get_fluid_state(pos.relative(d, 1))?.fluid_type,
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn dead(block: Block) -> Block {
    match block {
        Block::TUBE_CORAL => Block::DEAD_TUBE_CORAL,
        Block::BRAIN_CORAL => Block::DEAD_BRAIN_CORAL,
        Block::BUBBLE_CORAL => Block::DEAD_BUBBLE_CORAL,
        Block::FIRE_CORAL => Block::DEAD_FIRE_CORAL,
        Block::HORN_CORAL => Block::DEAD_HORN_CORAL,
        Block::TUBE_CORAL_FAN => Block::DEAD_TUBE_CORAL_FAN,
        Block::BRAIN_CORAL_FAN => Block::DEAD_BRAIN_CORAL_FAN,
        Block::BUBBLE_CORAL_FAN => Block::DEAD_BUBBLE_CORAL_FAN,
        Block::FIRE_CORAL_FAN => Block::DEAD_FIRE_CORAL_FAN,
        Block::HORN_CORAL_FAN => Block::DEAD_HORN_CORAL_FAN,
        Block::TUBE_CORAL_WALL_FAN => Block::DEAD_TUBE_CORAL_WALL_FAN,
        Block::BRAIN_CORAL_WALL_FAN => Block::DEAD_BRAIN_CORAL_WALL_FAN,
        Block::BUBBLE_CORAL_WALL_FAN => Block::DEAD_BUBBLE_CORAL_WALL_FAN,
        Block::FIRE_CORAL_WALL_FAN => Block::DEAD_FIRE_CORAL_WALL_FAN,
        Block::HORN_CORAL_WALL_FAN => Block::DEAD_HORN_CORAL_WALL_FAN,
        Block::TUBE_CORAL_BLOCK => Block::DEAD_TUBE_CORAL_BLOCK,
        Block::BRAIN_CORAL_BLOCK => Block::DEAD_BRAIN_CORAL_BLOCK,
        Block::BUBBLE_CORAL_BLOCK => Block::DEAD_BUBBLE_CORAL_BLOCK,
        Block::FIRE_CORAL_BLOCK => Block::DEAD_FIRE_CORAL_BLOCK,
        Block::HORN_CORAL_BLOCK => Block::DEAD_HORN_CORAL_BLOCK,
        _ => unreachable!("registered living coral"),
    }
}
fn schedule_die(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if !scan_water(state, pos, &level.as_reader())? {
        let delay = 60 + level.random()?.next_int_bound(40);
        level.schedule_block_tick(pos, state.block(), delay)?;
    }
    Ok(())
}
impl BlockBehavior for Coral {
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let state = block.default_state();
        schedule_die(state, context.clicked_pos(), level)?;
        Ok(Some(state))
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        schedule_die(state, pos, &mut LevelAccessRef::Server(level))
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !scan_water(state, pos, &LevelReaderRef::Server(level))? {
            LevelAccessRef::Server(level).set_block(
                pos,
                dead(state.block())
                    .default_state()
                    .with_properties_of(state),
                UpdateFlags::CLIENTS,
            )?;
        }
        Ok(())
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
        let class = state.block().class();
        if class == BlockClass::CoralBlock {
            schedule_die(state, pos, level)?;
            return Ok(state);
        }
        let toward_support = if class == BlockClass::CoralWallFanBlock {
            super::survival::facing(state).opposite()
        } else {
            Direction::Down
        };
        if direction == toward_support
            && !super::survival::PLANT.can_survive(state, pos, &level.as_reader())?
        {
            return Ok(Block::AIR.default_state());
        }
        if class == BlockClass::CoralWallFanBlock {
            super::survival::schedule_water(state, pos, level)?;
            schedule_die(state, pos, level)?;
        } else {
            schedule_die(state, pos, level)?;
            super::survival::schedule_water(state, pos, level)?;
        }
        super::survival::PLANT.update_shape(state, pos, direction, neighbor_pos, neighbor, level)
    }
}
