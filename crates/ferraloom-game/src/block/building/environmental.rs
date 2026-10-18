//! Creaking-heart log activation and potent-sulfur placement state selection.
use crate::{
    block::{
        BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef,
        entity::{BlockEntityData, BlockEntityType},
    },
    level::{LevelError, environment::EnvironmentAttributes},
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
    tags::BlockTags,
};
use ferraloom_util::{Axis, BlockPos, Direction};
pub(super) struct Environmental;
pub(super) static ENVIRONMENTAL: Environmental = Environmental;
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(block.class(), C::CreakingHeartBlock | C::PotentSulfurBlock)
        && matches!(
            method,
            BlockMethod::GetStateForPlacement
                | BlockMethod::UpdateShape
                | BlockMethod::UpdateFromNeighbourShapes
        )
}
fn heart(
    pos: BlockPos,
    face: Direction,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let axis = face.axis();
    let state = Block::CREAKING_HEART
        .default_state()
        .with(
            p::AXIS,
            match axis {
                Axis::X => p::Axis::X,
                Axis::Y => p::Axis::Y,
                Axis::Z => p::Axis::Z,
            },
        )
        .unwrap();
    // Direction.Axis#getDirections returns positive before negative.
    for side in [axis.positive(), axis.negative()] {
        let log = level.get_block_state(pos.relative(side, 1))?;
        if !level
            .block_tags()?
            .contains_key(BlockTags::PALE_OAK_LOGS, log.block())
            || log.get(p::AXIS) != state.get(p::AXIS)
        {
            return Ok(state);
        }
    }
    let active = level.get_value(EnvironmentAttributes::CREAKING_ACTIVE, pos)?;
    Ok(state
        .with(
            p::CREAKING_HEART_STATE,
            if active {
                p::CreakingHeartState::Awake
            } else {
                p::CreakingHeartState::Dormant
            },
        )
        .unwrap())
}
/// Port of PotentSulfurBlock#validBlockState. The only pending native mutation
/// is the matching potent-sulfur receiver's resetCountdown, awaiting its canonical
/// typed hook on restart. Placement without a matching receiver is complete.
fn sulfur(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let reader = level.as_reader();
    let above = reader.get_fluid_state(pos.above(1))?;
    if above.fluid_type != Fluid::WATER || !above.source {
        return Ok(state
            .with(p::POTENT_SULFUR_STATE, p::PotentSulfurState::Dry)
            .unwrap());
    }
    let below = reader.get_block_state(pos.below(1))?;
    let fluid = below.data().fluid;
    let source_if_fluid = fluid.empty || fluid.source;
    if reader
        .block_tags()?
        .contains_key(BlockTags::CAUSES_CONTINUOUS_GEYSER_ERUPTIONS, below.block())
        && source_if_fluid
    {
        return Ok(state
            .with(p::POTENT_SULFUR_STATE, p::PotentSulfurState::Continuous)
            .unwrap());
    }
    if !reader
        .block_tags()?
        .contains_key(BlockTags::CAUSES_PERIODIC_GEYSER_ERUPTIONS, below.block())
        || !source_if_fluid
    {
        return Ok(state
            .with(p::POTENT_SULFUR_STATE, p::PotentSulfurState::Wet)
            .unwrap());
    }
    let current = state.get(p::POTENT_SULFUR_STATE).unwrap();
    if !matches!(
        current,
        p::PotentSulfurState::Erupting | p::PotentSulfurState::Dormant
    ) {
        match level {
            LevelAccessRef::Server(level) => {
                if let Some(handle) = level.block_entity_ref(pos)?
                    && level.block_entity::<BlockEntityData>(handle)?.kind
                        == BlockEntityType::POTENT_SULFUR
                {
                    return Err(LevelError::UnsupportedBlockEntity(Block::POTENT_SULFUR));
                }
            }
            LevelAccessRef::Worldgen(region) => {
                // WorldGenRegion#getBlockEntity must activate pending NBT before
                // the instanceof test; a tag alone is not a canonical receiver.
                if region
                    .block_entity([pos.x, pos.y, pos.z])
                    .map_err(|_| LevelError::UnsupportedWorldgenEnvironment)?
                    .is_some()
                {
                    return Err(LevelError::UnsupportedWorldgenEnvironment);
                }
            }
        }
    }
    Ok(if current == p::PotentSulfurState::Erupting {
        state
    } else {
        state
            .with(p::POTENT_SULFUR_STATE, p::PotentSulfurState::Dormant)
            .unwrap()
    })
}
impl BlockBehavior for Environmental {
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        Ok(Some(if block.class() == C::CreakingHeartBlock {
            heart(
                context.clicked_pos(),
                context.clicked_face(),
                &level.as_reader(),
            )?
        } else {
            sulfur(block.default_state(), context.clicked_pos(), level)?
        }))
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.block().class() == C::CreakingHeartBlock {
            level.schedule_block_tick(pos, state.block(), 1)?;
            Ok(state)
        } else {
            sulfur(state, pos, level)
        }
    }
}
