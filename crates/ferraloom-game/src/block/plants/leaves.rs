//! Port of LeavesBlock's distance updates, decay and waterlogging.
use super::super::{
    BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags,
    fluid::SimpleWaterloggedBlock,
};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::{
    BlockState, FluidState, Item, properties as p, registries::fluid::Id, tags::BlockTags,
};
use ferraloom_util::{BlockPos, Direction, VoxelShape, shapes};

pub struct Leaves;
pub static LEAVES: Leaves = Leaves;
/// Port of LeavesBlock#getOptionalDistanceAt.
pub fn distance_at(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<u8, LevelError> {
    Ok(
        if level
            .block_tags()?
            .contains_key(BlockTags::PREVENTS_NEARBY_LEAF_DECAY, state.block())
        {
            0
        } else {
            state.get(p::DISTANCE).unwrap_or(7)
        },
    )
}
/// Port of LeavesBlock#updateDistance. Direction order and early exit are vanilla's.
pub fn update_distance(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let mut distance = 7;
    for d in Direction::VALUES {
        distance =
            distance.min(distance_at(level.get_block_state(pos.relative(d, 1))?, level)? + 1);
        if distance == 1 {
            break;
        }
    }
    Ok(state.with(p::DISTANCE, distance).expect("leaves distance"))
}
impl BlockBehavior for Leaves {
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        super::survival::schedule_water(state, pos, level)?;
        let distance = distance_at(neighbor, &level.as_reader())? + 1;
        if distance != 1 || state.get(p::DISTANCE) != Some(distance) {
            level.schedule_block_tick(pos, state.block(), 1)?;
        }
        Ok(state)
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let next = update_distance(state, pos, &LevelReaderRef::Server(level))?;
        level.set_block(pos, next, UpdateFlags::ALL, UPDATE_LIMIT)?;
        Ok(())
    }
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.get(p::PERSISTENT) == Some(false) && state.get(p::DISTANCE) == Some(7) {
            level.emit(LevelEffect::DropResources { pos, state });
            level.remove_block(pos, false)?;
        }
        Ok(())
    }
    fn support_shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(shapes::empty())
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
