//! Ports of FallingBlock and VegetationBlock's live/core methods.
use super::{BlockBehavior, LevelAccessRef, LevelReaderRef};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, Direction};
pub struct FallingBlock;
pub static FALLING_BLOCK: FallingBlock = FallingBlock;
impl BlockBehavior for FallingBlock {
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.schedule_block_tick_after(pos, state.block(), 2)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_>,
    ) -> Result<BlockState, LevelError> {
        level.schedule_block_tick(pos, state.block(), 2)?;
        Ok(state)
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        if below.is_air()
            || below.data().replaceable
            || below.liquid()
            || level
                .block_tags()?
                .contains_key(ferraloom_data::tags::BlockTags::FIRE, below.block())
        {
            let min_y = level
                .chunk(ferraloom_util::ChunkPos::containing(pos))
                .unwrap()
                .min_y();
            if pos.y >= min_y {
                // FallingBlockEntity#fall removes the block synchronously. The
                // entity insertion hook carries the original state.
                level.set_block(
                    pos,
                    super::fluid::legacy_block(state.data().fluid),
                    super::UpdateFlags::ALL,
                    super::UPDATE_LIMIT,
                )?;
                level.emit(LevelEffect::FallingBlock { pos, state });
            }
        }
        Ok(())
    }
}
pub struct VegetationBlock;
pub static VEGETATION_BLOCK: VegetationBlock = VegetationBlock;
impl BlockBehavior for VegetationBlock {
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_>,
    ) -> Result<bool, LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        match level {
            LevelReaderRef::Server(level) => Ok(level.block_tags()?.contains_key(
                ferraloom_data::tags::BlockTags::SUPPORTS_VEGETATION,
                below.block(),
            )),
        }
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_>,
    ) -> Result<BlockState, LevelError> {
        Ok(if self.can_survive(state, pos, &level.as_reader())? {
            state
        } else {
            Block::AIR.default_state()
        })
    }
}

/// Port of ChainBlock's waterlogging and scheduled fluid update. Placement
/// context and entity/pathfinding methods belong to their respective ports.
pub struct ChainBlock;
pub static CHAIN_BLOCK: ChainBlock = ChainBlock;
impl BlockBehavior for ChainBlock {
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_>,
    ) -> Result<BlockState, LevelError> {
        if state.get(ferraloom_data::properties::WATERLOGGED) == Some(true) {
            level.schedule_fluid_tick(pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
        }
        Ok(state)
    }
    fn can_place_liquid(
        &self,
        _state: BlockState,
        fluid: ferraloom_data::registries::fluid::Id,
    ) -> bool {
        fluid == ferraloom_data::registries::fluid::Id::WATER
    }
    fn place_liquid(
        &self,
        state: BlockState,
        pos: BlockPos,
        fluid: ferraloom_data::FluidState,
        level: &mut LevelAccessRef<'_>,
    ) -> Result<bool, LevelError> {
        super::fluid::SimpleWaterloggedBlock::place_liquid(state, pos, fluid, level)
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_>,
    ) -> Result<Option<ferraloom_data::Item>, LevelError> {
        super::fluid::SimpleWaterloggedBlock::pickup_block(state, pos, level)
    }
}

/// Port of AirBlock#getShape. All remaining simulation methods inherit base.
pub struct AirBlock;
pub static AIR_BLOCK: AirBlock = AirBlock;
impl BlockBehavior for AirBlock {
    fn shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::empty())
    }
}
