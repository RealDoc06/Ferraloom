//! Ports of CactusBlock and SugarCaneBlock age/height growth.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, random::RandomSource};
pub struct ColumnPlant;
pub static COLUMN_PLANT: ColumnPlant = ColumnPlant;
impl BlockBehavior for ColumnPlant {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let above = pos.above(1);
        if !level.get_block_state(above)?.is_air() {
            return Ok(());
        }
        let cactus = state.block() == Block::CACTUS;
        let age = state.get(p::AGE).unwrap();
        let mut height = 1;
        while level.get_block_state(pos.below(height))?.block() == state.block() {
            height += 1;
            if cactus && height == 3 && age == 15 {
                return Ok(());
            }
        }
        if cactus {
            if age == 8
                && super::survival::PLANT.can_survive(
                    Block::CACTUS.default_state(),
                    above,
                    &LevelReaderRef::Server(level),
                )?
            {
                if level.random().next_double() <= if height >= 3 { 0.25 } else { 0.1 } {
                    LevelAccessRef::Server(level).set_block(
                        above,
                        Block::CACTUS_FLOWER.default_state(),
                        UpdateFlags::ALL,
                    )?;
                }
            } else if age == 15 && height < 3 {
                LevelAccessRef::Server(level).set_block(
                    above,
                    state.block().default_state(),
                    UpdateFlags::ALL,
                )?;
                let next = state.with(p::AGE, 0).unwrap();
                LevelAccessRef::Server(level).set_block(pos, next, UpdateFlags::NONE)?;
                level.neighbor_changed_with_state(next, above, state.block(), None, false)?;
            }
            if age < 15 {
                LevelAccessRef::Server(level).set_block(
                    pos,
                    state.with(p::AGE, age + 1).unwrap(),
                    UpdateFlags::NONE,
                )?;
            }
        } else if height < 3 {
            if age == 15 {
                LevelAccessRef::Server(level).set_block(
                    above,
                    state.block().default_state(),
                    UpdateFlags::ALL,
                )?;
            }
            LevelAccessRef::Server(level).set_block(
                pos,
                state
                    .with(p::AGE, if age == 15 { 0 } else { age + 1 })
                    .unwrap(),
                UpdateFlags::NONE,
            )?;
        }
        Ok(())
    }
}
