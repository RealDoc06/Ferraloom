//! BoneMealItem delegates every growth stage to the canonical block family.
use crate::{
    block::{
        InteractionResult, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags, UseOnContext,
        plants::{BonemealSource, BonemealableBlock, bonemealable_behavior},
    },
    inventory::{StackOperation, StackRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    registries::game_event,
    tags::{BlockTags, TagKey},
};
use ferraloom_util::{BlockPos, Direction, core::Plane, random::RandomSource};

/// Port of BoneMealItem#useOn. Crop growth succeeds even when its success roll
/// declines to grow; the stack is still shrunk and the server event is emitted.
pub(super) fn use_on(
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    let pos = context.clicked_pos();
    let relative = pos.relative(context.clicked_face(), 1);
    let receiver = super::block_item::retained(context.stack_view())?;
    if grow_crop(receiver, pos, operation)? {
        super::stack::cause_use_vibration(
            receiver,
            context.player(),
            game_event::Id::ITEM_INTERACT_FINISH,
            operation,
        )?;
        operation.level.level_event(None, 1505, pos, 15)?;
        return Ok(Some(InteractionResult::SUCCESS_SERVER));
    }
    let reader = LevelReaderRef::Server(operation.level);
    let state = reader.get_block_state(pos)?;
    if reader.is_face_sturdy(state, pos, context.clicked_face())?
        && grow_water_plant(receiver, relative, Some(context.clicked_face()), operation)?
    {
        super::stack::cause_use_vibration(
            receiver,
            context.player(),
            game_event::Id::ITEM_INTERACT_FINISH,
            operation,
        )?;
        operation.level.level_event(None, 1505, relative, 15)?;
        Ok(Some(InteractionResult::SUCCESS))
    } else {
        Ok(Some(InteractionResult::Pass))
    }
}

fn behavior(state: BlockState) -> Result<Option<&'static dyn BonemealableBlock>, LevelError> {
    if !state.block().is_instance_of(BlockClass::BonemealableBlock) {
        return Ok(None);
    }
    bonemealable_behavior(state.block())
        .map(Some)
        .ok_or(LevelError::UnsupportedBlock(state.block()))
}

/// Port of BoneMealItem#growCrop on the live ServerLevel. All callbacks draw
/// from the same canonical stream through short LevelAccessRef borrows.
pub fn grow_crop(
    receiver: StackRef,
    pos: BlockPos,
    operation: &mut StackOperation<'_>,
) -> Result<bool, LevelError> {
    let state = operation.level.get_block_state(pos)?;
    let Some(behavior) = behavior(state)? else {
        return Ok(false);
    };
    let mut access = LevelAccessRef::Server(operation.level);
    if !behavior.is_valid_bonemeal_target(state, pos, &mut access, BonemealSource::Interaction)? {
        return Ok(false);
    }
    if behavior.is_bonemeal_success(state, pos, &mut access, BonemealSource::Interaction)? {
        behavior.perform_bonemeal(state, pos, &mut access, BonemealSource::Interaction)?;
    }
    shrink(receiver, operation)?;
    Ok(true)
}

fn shrink(receiver: StackRef, operation: &mut StackOperation<'_>) -> Result<(), LevelError> {
    let stack = operation
        .level
        .stack_mut(receiver)
        .map_err(super::stack::receiver_error)?;
    let count = crate::entity::player::inventory::count(stack);
    crate::entity::player::inventory::set_count(stack, count.wrapping_sub(1));
    Ok(())
}

fn random_tag_state(
    level: &mut Level,
    tag: TagKey<Block>,
) -> Result<Option<BlockState>, LevelError> {
    let Some(tag) = level.block_tags()?.resolve(tag) else {
        return Ok(None);
    };
    let size = level.block_tags()?.values(tag).len();
    if size == 0 {
        return Ok(None);
    }
    let index = level.random().next_int_bound(size as i32) as usize;
    Ok(Some(level.block_tags()?.values(tag)[index].default_state()))
}

/// Port of BoneMealItem#growWaterPlant's bounded random walk, including biome
/// tags, tag encounter order, wall-fan retries and repeated seagrass growth.
pub fn grow_water_plant(
    receiver: StackRef,
    pos: BlockPos,
    clicked_face: Option<Direction>,
    operation: &mut StackOperation<'_>,
) -> Result<bool, LevelError> {
    let level = &mut *operation.level;
    if level.get_block_state(pos)?.block() != Block::WATER
        || LevelReaderRef::Server(level).get_fluid_state(pos)?.amount != 8
    {
        return Ok(false);
    }
    'attempt: for attempt in 0..128 {
        let mut test = pos;
        let mut grow = Block::SEAGRASS.default_state();
        for _ in 0..attempt / 16 {
            let x = level.random().next_int_bound(3) - 1;
            let y = (level.random().next_int_bound(3) - 1) * level.random().next_int_bound(3) / 2;
            let z = level.random().next_int_bound(3) - 1;
            test = test.offset(x, y, z);
            let reader = LevelReaderRef::Server(level);
            if reader.is_collision_shape_full_block(reader.get_block_state(test)?, test)? {
                continue 'attempt;
            }
        }
        let coral_biome = {
            let biome = level.biome_at(test)?;
            level
                .context()
                .generator
                .access()
                .data()
                .biome_tags
                .get(
                    &"minecraft:produces_corals_from_bonemeal"
                        .parse()
                        .expect("builtin tag"),
                )
                .is_some_and(|values| values.contains(biome))
        };
        if coral_biome {
            if attempt == 0 && clicked_face.is_some_and(|face| face.axis().is_horizontal()) {
                grow = random_tag_state(level, BlockTags::WALL_CORALS)?.unwrap_or(grow);
                if grow.get(p::FACING).is_some() {
                    grow = grow
                        .with(p::FACING, clicked_face.unwrap())
                        .expect("horizontal coral face");
                }
            } else if level.random().next_int_bound(4) == 0 {
                grow = random_tag_state(level, BlockTags::UNDERWATER_BONEMEALS)?.unwrap_or(grow);
            }
        }
        if level
            .block_tags()?
            .contains_key(BlockTags::WALL_CORALS, grow.block())
            && grow.get(p::FACING).is_some()
        {
            let mut retries = 0;
            // The native condition tests survival before its d < 4 guard.
            while !level.can_survive(grow, test)? && retries < 4 {
                let direction =
                    Plane::Horizontal.faces()[level.random().next_int_bound(4) as usize];
                grow = grow.with(p::FACING, direction).expect("wall coral face");
                retries += 1;
            }
        }
        if level.can_survive(grow, test)? {
            let state = level.get_block_state(test)?;
            if state.block() == Block::WATER
                && LevelReaderRef::Server(level).get_fluid_state(test)?.amount == 8
            {
                level.set_block(test, grow, UpdateFlags::ALL, UPDATE_LIMIT)?;
            } else if state.block() == Block::SEAGRASS {
                let behavior = bonemealable_behavior(Block::SEAGRASS).expect("seagrass bonemeal");
                let mut access = LevelAccessRef::Server(level);
                if behavior.is_valid_bonemeal_target(
                    state,
                    test,
                    &mut access,
                    BonemealSource::Interaction,
                )? && access.random()?.next_int_bound(10) == 0
                {
                    behavior.perform_bonemeal(
                        state,
                        test,
                        &mut access,
                        BonemealSource::Interaction,
                    )?;
                }
            }
        }
    }
    shrink(receiver, operation)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
