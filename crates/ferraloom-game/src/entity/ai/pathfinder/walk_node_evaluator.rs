use super::{PathComputationType, PathType};
use crate::{block::LevelReaderRef, level::LevelError};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::BlockPos;

/// Port of NodeEvaluator#isBurningBlock, including live campfire/fire tags.
pub fn is_burning_block(
    state: BlockState,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let block = state.block();
    Ok(level.block_tags()?.contains_key(BlockTags::FIRE, block)
        || block == Block::LAVA
        || block == Block::MAGMA_BLOCK
        || (state.get(p::LIT).is_some()
            && level
                .block_tags()?
                .contains_key(BlockTags::CAMPFIRES, block)
            && state.get(p::LIT) == Some(true))
        || block == Block::LAVA_CAULDRON)
}

/// Port of WalkNodeEvaluator#getPathTypeFromState. Every shape/tag read goes
/// through the supplied reader; no chunk acquisition or unbounded neighbor read.
pub fn get_path_type_from_state(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<PathType, LevelError> {
    let state = level.get_block_state(pos)?;
    let block = state.block();
    if state.is_air() {
        return Ok(PathType::Open);
    }
    let tags = level.block_tags()?;
    if tags.contains_key(BlockTags::TRAPDOORS, block)
        || matches!(block, Block::LILY_PAD | Block::BIG_DRIPLEAF)
    {
        return Ok(PathType::Trapdoor);
    }
    if block == Block::POWDER_SNOW {
        return Ok(PathType::PowderSnow);
    }
    if matches!(block, Block::CACTUS | Block::SWEET_BERRY_BUSH) {
        return Ok(PathType::Damaging);
    }
    if block == Block::HONEY_BLOCK {
        return Ok(PathType::StickyHoney);
    }
    if block == Block::COCOA {
        return Ok(PathType::Cocoa);
    }
    if block == Block::WITHER_ROSE || tags.contains_key(BlockTags::SPELEOTHEMS, block) {
        return Ok(PathType::DamageCautious);
    }
    let fluid = state.data().fluid.fluid_type;
    if level.fluid_tags()?.contains_key(FluidTags::LAVA, fluid) {
        return Ok(PathType::Lava);
    }
    if is_burning_block(state, level)? {
        return Ok(PathType::Fire);
    }
    if block.is_instance_of(BlockClass::DoorBlock) {
        return Ok(if state.get(p::OPEN).expect("DoorBlock OPEN property") {
            PathType::DoorOpen
        } else if block
            .redstone_properties()
            .block_set
            .expect("DoorBlock block set")
            .can_open_by_hand
        {
            PathType::DoorWoodClosed
        } else {
            PathType::DoorIronClosed
        });
    }
    if block.is_instance_of(BlockClass::BaseRailBlock) {
        return Ok(PathType::Rail);
    }
    if block.is_instance_of(BlockClass::LeavesBlock) {
        return Ok(PathType::Leaves);
    }
    if tags.contains_key(BlockTags::FENCES, block)
        || tags.contains_key(BlockTags::WALLS, block)
        || (block.is_instance_of(BlockClass::FenceGateBlock)
            && !state.get(p::OPEN).expect("FenceGateBlock OPEN property"))
    {
        return Ok(PathType::Fence);
    }
    if !level.is_pathfindable(state, PathComputationType::Land)? {
        return Ok(PathType::Blocked);
    }
    Ok(
        if level.fluid_tags()?.contains_key(FluidTags::WATER, fluid) {
            PathType::Water
        } else {
            PathType::Open
        },
    )
}

/// Port of WalkNodeEvaluator#getPathTypeStatic. The callback resolves the
/// canonical cache with short borrows and classifies misses through its reader.
pub fn get_path_type_static<E>(
    pos: BlockPos,
    min_y: i32,
    mut path_type: impl FnMut(BlockPos) -> Result<PathType, E>,
) -> Result<PathType, E> {
    let current = path_type(pos)?;
    if current != PathType::Open || pos.y < min_y.wrapping_add(1) {
        return Ok(current);
    }
    Ok(match path_type(pos.below(1))? {
        PathType::Open | PathType::Water | PathType::Lava | PathType::Walkable => PathType::Open,
        PathType::Fire => PathType::Fire,
        PathType::Damaging => PathType::Damaging,
        PathType::StickyHoney => PathType::StickyHoney,
        PathType::PowderSnow => PathType::OnTopOfPowderSnow,
        PathType::DamageCautious => PathType::DamageCautious,
        PathType::Trapdoor => PathType::OnTopOfTrapdoor,
        _ => check_neighbour_blocks(pos, PathType::Walkable, path_type)?,
    })
}

/// Port of WalkNodeEvaluator#checkNeighbourBlocks, preserving dx/dy/dz order
/// and the first hazard found, rather than taking a maximum hazard cost.
pub fn check_neighbour_blocks<E>(
    pos: BlockPos,
    fallback: PathType,
    mut path_type: impl FnMut(BlockPos) -> Result<PathType, E>,
) -> Result<PathType, E> {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx != 0 || dz != 0 {
                    match path_type(pos.offset(dx, dy, dz))? {
                        PathType::Damaging => return Ok(PathType::DamagingInNeighbor),
                        PathType::Fire | PathType::Lava => return Ok(PathType::FireInNeighbor),
                        PathType::Water => return Ok(PathType::WaterBorder),
                        PathType::DamageCautious => return Ok(PathType::DamageCautious),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(fallback)
}

pub(super) mod prepared;
pub use prepared::WalkNodeEvaluator;
