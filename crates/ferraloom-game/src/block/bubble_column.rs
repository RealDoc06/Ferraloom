//! Port of BubbleColumnBlock and LiquidBlock's shared updateColumn call.
use super::{BlockBehavior, BlockMethod, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::{
    entity::{EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, Item, properties,
    registries::fluid::Id as Fluid,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockPos, Direction};

struct BubbleColumnBlock;
static BUBBLE_COLUMN: BubbleColumnBlock = BubbleColumnBlock;

pub(super) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    (block == Block::BUBBLE_COLUMN
        && matches!(
            method,
            BlockMethod::CanSurvive
                | BlockMethod::EntityInside
                | BlockMethod::PickupBlock
                | BlockMethod::Tick
                | BlockMethod::UpdateShape
                | BlockMethod::UpdateFromNeighbourShapes
        ))
    .then_some(&BUBBLE_COLUMN)
}

/// Port of BubbleColumnBlock#canOccupy. A current column short-circuits all
/// fluid-tag queries; other states must be full source liquid blocks.
fn can_occupy(
    bubble: Block,
    state: BlockState,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    if state.block() == bubble {
        return Ok(true);
    }
    let fluid = state.data().fluid;
    Ok(reader
        .fluid_tags()?
        .contains_key(FluidTags::BUBBLE_COLUMN_CAN_OCCUPY, fluid.fluid_type)
        && state.block().is_instance_of(BlockClass::LiquidBlock)
        && fluid.source
        && fluid.amount >= 8)
}

/// Port of BubbleColumnBlock#getColumnState with live tag membership and the
/// native push-up-before-drag-down priority.
fn get_column_state(
    bubble: Block,
    below: BlockState,
    occupy: BlockState,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    if below.block() == bubble {
        return Ok(below);
    }
    let tags = reader.block_tags()?;
    if tags.contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_PUSH_UP, below.block()) {
        Ok(bubble
            .default_state()
            .with(properties::DRAG, false)
            .expect("bubble drag"))
    } else if tags.contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_DRAG_DOWN, below.block()) {
        Ok(bubble
            .default_state()
            .with(properties::DRAG, true)
            .expect("bubble drag"))
    } else if occupy.block() == bubble {
        Ok(Block::WATER.default_state())
    } else {
        Ok(occupy)
    }
}

/// Port of BubbleColumnBlock#updateColumn's explicit-state overload. The first
/// setBlock result is ignored; an unchanged later block stops the upward walk.
pub(super) fn update_column(
    bubble: Block,
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    occupy: BlockState,
    below: BlockState,
) -> Result<(), LevelError> {
    if can_occupy(bubble, occupy, &level.as_reader())? {
        let column = get_column_state(bubble, below, occupy, &level.as_reader())?;
        level.set_block(pos, column, UpdateFlags::CLIENTS)?;
        let mut cursor = pos.above(1);
        while can_occupy(bubble, level.get_block_state(cursor)?, &level.as_reader())? {
            if !level.set_block(cursor, column, UpdateFlags::CLIENTS)? {
                return Ok(());
            }
            cursor = cursor.above(1);
        }
    }
    Ok(())
}

impl BlockBehavior for BubbleColumnBlock {
    /// Port of BubbleColumnBlock#canSurvive.
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = reader.get_block_state(pos.below(1))?;
        if below.block() == state.block() {
            return Ok(true);
        }
        let tags = reader.block_tags()?;
        Ok(
            tags.contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_PUSH_UP, below.block())
                || tags.contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_DRAG_DOWN, below.block()),
        )
    }
    /// Port of BubbleColumnBlock#entityInside. The shared contact dispatcher
    /// publishes native isPrecise on the canonical entity before this call.
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level.entity::<EntityData>(entity)?.inside_block_is_precise {
            return Ok(());
        }
        let above = level.get_block_state(pos.above(1))?;
        // Native passes the original pos, despite reading the state above.
        let nothing_above = LevelReaderRef::Server(level)
            .collision_shape_without_context(above, pos)?
            .is_empty()
            && above.data().fluid.fluid_type == Fluid::EMPTY;
        let drag = state.get(properties::DRAG).expect("bubble drag");
        let behavior = level.entity_behavior(entity)?;
        if nothing_above {
            behavior.on_above_bubble_column(entity, drag, pos, level)
        } else {
            behavior.on_inside_bubble_column(entity, drag, level)
        }
    }
    /// Port of BubbleColumnBlock#pickupBlock: flags 11, then a water bucket,
    /// regardless of setBlock's boolean return.
    fn pickup_block(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        level.set_block(pos, Block::AIR.default_state(), UpdateFlags::ALL_IMMEDIATE)?;
        Ok(Some(Item::WATER_BUCKET))
    }
    /// Port of BubbleColumnBlock#tick.
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        update_column(
            state.block(),
            &mut LevelAccessRef::Server(level),
            pos,
            state,
            below,
        )
    }
    /// Port of BubbleColumnBlock#updateShape. Water is scheduled first, and the
    /// survival query precedes even the unconditional downward-neighbor branch.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        level.schedule_fluid_tick(pos, Fluid::WATER, 5)?;
        if !self.can_survive(state, pos, &level.as_reader())?
            || direction == Direction::Down
            || (direction == Direction::Up
                && neighbor.block() != state.block()
                && can_occupy(state.block(), neighbor, &level.as_reader())?)
        {
            level.schedule_block_tick(pos, state.block(), 5)?;
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests;
