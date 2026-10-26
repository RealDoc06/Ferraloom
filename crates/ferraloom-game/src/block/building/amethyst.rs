//! BuddingAmethystBlock's state-gated random growth.
use crate::{
    block::BlockBehavior,
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, properties as p, registries::fluid::Id as Fluid};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};

pub(super) struct BuddingAmethyst;
pub(super) static BUDDING_AMETHYST: BuddingAmethyst = BuddingAmethyst;

/// Port of BuddingAmethystBlock#canClusterGrowAtState. Full falling water also
/// qualifies; waterlogged blocks do not, even when their fluid is full water.
pub(super) fn can_cluster_grow_at_state(state: BlockState) -> bool {
    state.is_air() || state.block() == Block::WATER && state.data().fluid.amount == 8
}

impl BlockBehavior for BuddingAmethyst {
    /// Port of BuddingAmethystBlock#randomTick. The direction draw happens only
    /// after the one-in-five roll; the old fluid's exact type sets waterlogging.
    fn random_tick(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if level.random().next_int_bound(5) != 0 {
            return Ok(());
        }
        let direction = Direction::VALUES[level.random().next_int_bound(6) as usize];
        let target = pos.relative(direction, 1);
        let old = level.get_block_state(target)?;
        let next = if can_cluster_grow_at_state(old) {
            Some(Block::SMALL_AMETHYST_BUD)
        } else {
            let promoted = match old.block() {
                Block::SMALL_AMETHYST_BUD => Some(Block::MEDIUM_AMETHYST_BUD),
                Block::MEDIUM_AMETHYST_BUD => Some(Block::LARGE_AMETHYST_BUD),
                Block::LARGE_AMETHYST_BUD => Some(Block::AMETHYST_CLUSTER),
                _ => None,
            };
            promoted.filter(|_| super::facing(old) == direction)
        };
        if let Some(next) = next {
            let state = super::with_facing(next.default_state(), direction)
                .with(p::WATERLOGGED, old.data().fluid.fluid_type == Fluid::WATER)
                .unwrap();
            level.set_block_and_update(target, state)?;
        }
        Ok(())
    }
}
