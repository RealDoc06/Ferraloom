//! HoneycombItem's bespoke wax operation. Generic BlockTransformer ordering is
//! different: honeycomb shrinks unconditionally and emits vibration before sound.
use crate::{
    block::{InteractionResult, UPDATE_LIMIT, UpdateFlags, UseOnContext, entity::BlockEntityRef},
    entity::player::inventory,
    inventory::StackOperation,
    level::{Level, LevelError},
};
use ferraloom_data::{
    BlockClass, properties as p,
    registries::{game_event, sound_event},
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{BlockPos, Direction, Vec3};

/// Port of HoneycombItem#useOn. The context receiver can differ from the
/// ItemStack#useOn receiver; every mutation keeps its original retained identity.
pub(super) fn use_on(
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    let pos = context.clicked_pos();
    let old = operation.level.get_block_state(pos)?;
    let Some(waxed) = old.waxed() else {
        return Ok(Some(InteractionResult::Pass));
    };
    let player = context.player();
    let receiver = super::block_item::retained(context.stack_view())?;
    if let Some(player) = player {
        // Native criterion predicates read a value captured at this invocation;
        // the live operation-rooted receiver is reread after synchronous rewards.
        let stack = operation
            .level
            .stack(receiver)
            .map_err(super::stack::receiver_error)?;
        let captured = stack.copy_with_count(inventory::count(stack));
        crate::advancements::criteria::item_used_on_block(operation.level, player, pos, &captured)?;
    }
    let stack = operation
        .level
        .stack_mut(receiver)
        .map_err(super::stack::receiver_error)?;
    inventory::set_count(stack, inventory::count(stack).wrapping_sub(1));
    operation
        .level
        .set_block(pos, waxed, UpdateFlags::ALL_IMMEDIATE, UPDATE_LIMIT)?;
    operation.level.game_event(
        game_event::Id::BLOCK_CHANGE,
        center(pos),
        player,
        Some(waxed),
    )?;
    operation.level.level_event(player, 3003, pos, 0)?;
    operation.level.play_sound(
        player,
        center(pos),
        sound_event::Id::ITEM_HONEYCOMB_WAX_ON,
        SoundSource::Blocks,
        1.0,
        1.0,
    )?;
    if old.block().is_instance_of(BlockClass::ChestBlock)
        && old
            .get(p::TYPE_)
            .is_some_and(|value| value.as_str() != "single")
    {
        let facing = Direction::by_name(old.get(p::FACING).expect("chest facing").as_str())
            .expect("chest direction");
        let direction = if old.get(p::TYPE_).unwrap().as_str() == "left" {
            facing.clockwise()
        } else {
            facing.counter_clockwise()
        };
        let neighbor = pos.relative(direction, 1);
        let state = operation.level.get_block_state(neighbor)?;
        operation.level.game_event(
            game_event::Id::BLOCK_CHANGE,
            center(neighbor),
            player,
            Some(state),
        )?;
        operation.level.level_event(player, 3003, neighbor, 0)?;
        // HoneycombItem uses the original pos for this second sound too.
        operation.level.play_sound(
            player,
            center(pos),
            sound_event::Id::ITEM_HONEYCOMB_WAX_ON,
            SoundSource::Blocks,
            1.0,
            1.0,
        )?;
    }
    Ok(Some(InteractionResult::SUCCESS))
}
/// Port of HoneycombItem#tryApplyToSign. Its canApplyToSign is unconditionally
/// true; SignBlock owns eligibility, use stats, the block event and consumption.
pub(crate) fn try_apply_to_sign(
    handle: BlockEntityRef,
    level: &mut Level,
) -> Result<bool, LevelError> {
    if !crate::block::containers::sign::set_waxed(handle, true, level)? {
        return Ok(false);
    }
    level.level_event(None, 3003, handle.pos, 0)?;
    level.play_sound(
        None,
        center(handle.pos),
        sound_event::Id::ITEM_HONEYCOMB_WAX_ON,
        SoundSource::Blocks,
        1.0,
        1.0,
    )?;
    Ok(true)
}
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5)
}

#[cfg(test)]
mod tests;
