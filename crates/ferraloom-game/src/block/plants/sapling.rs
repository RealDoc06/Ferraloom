//! Ports of SaplingBlock and MangrovePropaguleBlock growth.
use super::{
    super::{BlockBehavior, LevelAccessRef, UpdateFlags},
    tree_grower::TreeGrower,
};
use crate::level::{Level, LevelError};
use ferraloom_data::{BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, random::RandomSource};

pub struct Sapling;
pub static SAPLING: Sapling = Sapling;

/// Port of SaplingBlock#advanceTree, retaining native stage write flags.
pub(super) fn advance_tree(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    if state.get(p::STAGE) == Some(0) {
        LevelAccessRef::Server(level).set_block(
            pos,
            state.with(p::STAGE, 1).unwrap(),
            UpdateFlags(260),
        )?;
    } else {
        TreeGrower::for_block(state.block())
            .expect("sapling grower")
            .grow_tree(state, pos, level)?;
    }
    Ok(())
}
impl BlockBehavior for Sapling {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.block().class() == BlockClass::MangrovePropaguleBlock {
            if state.get(p::HANGING) == Some(true) {
                let age = state.get(p::AGE).unwrap();
                if age < 4 {
                    LevelAccessRef::Server(level).set_block(
                        pos,
                        state.with(p::AGE, age + 1).unwrap(),
                        UpdateFlags::CLIENTS,
                    )?;
                }
            } else if level.random().next_int_bound(7) == 0 {
                advance_tree(state, pos, level)?;
            }
        } else if level.max_local_raw_brightness(pos.above(1))? >= 9
            && level.random().next_int_bound(7) == 0
        {
            advance_tree(state, pos, level)?;
        }
        Ok(())
    }
}
