//! CampfireBlock helpers shared by the live block and predicate paths.
use super::{BlockCollisionContext, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{
    BlockPos,
    shapes::{self, BooleanOp},
};

fn is_lit_campfire(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    // CampfireBlock#isLitCampfire checks the property before the tag, then LIT.
    Ok(state.get(p::LIT).is_some()
        && level
            .block_tags()?
            .contains_key(BlockTags::CAMPFIRES, state.block())
        && state.get(p::LIT) == Some(true))
}
/// Port of CampfireBlock#isSmokeyPos. The collision receiver is the state below,
/// but its contextual position is the original caller position. An obstruction
/// permits exactly one further campfire test before returning.
pub fn is_smokey_pos(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let post = shapes::block_box(6.0, 0.0, 6.0, 10.0, 16.0, 10.0);
    for distance in 1..=5 {
        let at = pos.below(distance);
        let state = level.get_block_state(at)?;
        if is_lit_campfire(state, level)? {
            return Ok(true);
        }
        let shape = level.collision_shape(state, pos, &BlockCollisionContext::empty())?;
        if shapes::join_is_not_empty(&post, &shape, BooleanOp::And) {
            return is_lit_campfire(level.get_block_state(at.below(1))?, level);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;
