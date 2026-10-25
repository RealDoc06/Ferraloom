//! Furniture/container placement virtuals, shared by live and worldgen dispatch.
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p, registries::fluid, tags::BlockTags,
};
use ferraloom_util::{BlockPos, Direction};

mod furniture;
mod signs;
pub use signs::wall_hanging_sign_can_place;

pub(super) struct ContainerPlacement;
pub(super) static CONTAINER_PLACEMENT: ContainerPlacement = ContainerPlacement;

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    signs::supports(block, method)
        || furniture::supports(block, method)
        || method == BlockMethod::GetStateForPlacement
            && (block.is_instance_of(BlockClass::AbstractBedBlock)
                || block.is_instance_of(BlockClass::AbstractFurnaceBlock)
                || block.is_instance_of(BlockClass::ChestBlock)
                || matches!(
                    block.class(),
                    BlockClass::BarrelBlock
                        | BlockClass::EnderChestBlock
                        | BlockClass::ShulkerBoxBlock
                ))
}
fn facing(direction: Direction) -> p::Facing {
    match direction {
        Direction::Down => p::Facing::Down,
        Direction::Up => p::Facing::Up,
        Direction::North => p::Facing::North,
        Direction::South => p::Facing::South,
        Direction::West => p::Facing::West,
        Direction::East => p::Facing::East,
    }
}
fn direction(value: p::Facing) -> Direction {
    match value {
        p::Facing::Down => Direction::Down,
        p::Facing::Up => Direction::Up,
        p::Facing::North => Direction::North,
        p::Facing::South => Direction::South,
        p::Facing::West => Direction::West,
        p::Facing::East => Direction::East,
    }
}
/// Port of ChestBlock#chestCanConnectTo and CopperChestBlock's live-tag override.
pub(super) fn chest_can_connect_to(
    block: Block,
    neighbor: BlockState,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    Ok(if block.is_instance_of(BlockClass::CopperChestBlock) {
        reader
            .block_tags()?
            .contains_key(BlockTags::COPPER_CHESTS, neighbor.block())
            && neighbor.property_values("type") == block.default_state().property_values("type")
    } else {
        neighbor.block() == block
    })
}
/// Port of ChestBlock#candidatePartnerFacing. Joined neighbors cannot accept
/// another partner, even when their class allows cross-variant connections.
fn candidate_partner_facing(
    block: Block,
    pos: BlockPos,
    side: Direction,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<Option<Direction>, LevelError> {
    let state = reader.get_block_state(pos.relative(side, 1))?;
    let connects = chest_can_connect_to(block, state, reader)?;
    Ok(
        if connects && state.get(p::TYPE_) == Some(p::Type::Single) {
            state.get(p::FACING).map(direction)
        } else {
            None
        },
    )
}
/// Port of ChestBlock#getStateForPlacement. Query order matters: replaced fluid,
/// secondary use, explicit clicked-face partner, then clockwise-first joining.
fn chest_placement(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let mut chest_type = p::Type::Single;
    let mut forward = context.horizontal_direction(reader)?.opposite();
    let pos = context.clicked_pos();
    let waterlogged = reader.get_fluid_state(pos)?.fluid_type == fluid::Id::WATER;
    let secondary = context.is_secondary_use_active(reader)?;
    let clicked = context.clicked_face();
    if clicked.axis().is_horizontal()
        && secondary
        && let Some(partner) = candidate_partner_facing(block, pos, clicked.opposite(), reader)?
        && partner.axis() != clicked.axis()
    {
        forward = partner;
        chest_type = if forward.counter_clockwise() == clicked.opposite() {
            p::Type::Right
        } else {
            p::Type::Left
        };
    }
    if chest_type == p::Type::Single && !secondary {
        if candidate_partner_facing(block, pos, forward.clockwise(), reader)? == Some(forward) {
            chest_type = p::Type::Left;
        } else if candidate_partner_facing(block, pos, forward.counter_clockwise(), reader)?
            == Some(forward)
        {
            chest_type = p::Type::Right;
        }
    }
    Ok(block
        .default_state()
        .with(p::FACING, facing(forward))
        .unwrap()
        .with(p::TYPE_, chest_type)
        .unwrap()
        .with(p::WATERLOGGED, waterlogged)
        .unwrap())
}
/// Port of CopperChestBlock#getLeastOxidizedChestOfConnectedBlocks. The native
/// connected-state read is eager even for a single chest; placement mutates no
/// neighbor. Mixed wax states predict both halves unwaxed before choosing.
fn least_oxidized_connected(
    state: BlockState,
    pos: BlockPos,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let connected =
        reader.get_block_state(pos.relative(super::menu::connected_direction(state), 1))?;
    let (Some(own), Some(other)) = (
        state.block().copper_chest_properties(),
        connected.block().copper_chest_properties(),
    ) else {
        return Ok(state);
    };
    if state.get(p::TYPE_) == Some(p::Type::Single) {
        return Ok(state);
    }
    let mut own_block = state.block();
    let mut other_block = connected.block();
    if own.is_waxed != other.is_waxed {
        own_block = own.unwaxed.unwrap_or(own_block);
        other_block = other.unwaxed.unwrap_or(other_block);
    }
    let chosen = if own.weathering_stage <= other.weathering_stage {
        own_block
    } else {
        other_block
    };
    Ok(chosen.default_state().with_properties_of(state))
}
impl BlockBehavior for ContainerPlacement {
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        signs::update_shape(state, pos, direction, neighbor_pos, neighbor, level)
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if state.block().class() == BlockClass::BellBlock {
            furniture::can_survive(state, pos, reader)
        } else {
            signs::can_survive(state, pos, reader)
        }
    }
    /// Shared per-method dispatcher for furniture/container getStateForPlacement
    /// ports. Sign and utility subclasses keep their own native read order.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        if signs::supports(block, BlockMethod::GetStateForPlacement) {
            return signs::get_state_for_placement(block, context, &reader);
        }
        if furniture::supports(block, BlockMethod::GetStateForPlacement) {
            return furniture::get_state_for_placement(block, context, &reader);
        }
        let state = block.default_state();
        if block.is_instance_of(BlockClass::AbstractBedBlock) {
            let forward = context.horizontal_direction(&reader)?;
            let head = context.clicked_pos().relative(forward, 1);
            if !reader.can_be_replaced(reader.get_block_state(head)?, context)? {
                return Ok(None);
            }
            let within_border = match reader {
                LevelReaderRef::Server(level) => level.world_border().is_within_block(head),
                LevelReaderRef::Worldgen(_) => {
                    return Err(LevelError::UnsupportedWorldgenEnvironment);
                }
                LevelReaderRef::PathNavigation(_) => {
                    return Err(LevelError::UnsupportedPathNavigationQuery(
                        crate::level::path_navigation_region::PathNavigationQuery::Environment,
                    ));
                }
            };
            return Ok(within_border.then(|| state.with(p::FACING, facing(forward)).unwrap()));
        }
        if block.is_instance_of(BlockClass::ChestBlock) {
            let state = chest_placement(block, context, &reader)?;
            return Ok(Some(
                if block.is_instance_of(BlockClass::CopperChestBlock) {
                    least_oxidized_connected(state, context.clicked_pos(), &reader)?
                } else {
                    state
                },
            ));
        }
        Ok(Some(match block.class() {
            BlockClass::BarrelBlock => state
                .with(
                    p::FACING,
                    facing(context.nearest_looking_direction(&reader)?.opposite()),
                )
                .unwrap(),
            BlockClass::ShulkerBoxBlock => state
                .with(p::FACING, facing(context.clicked_face()))
                .unwrap(),
            BlockClass::EnderChestBlock => {
                let waterlogged =
                    reader.get_fluid_state(context.clicked_pos())?.fluid_type == fluid::Id::WATER;
                state
                    .with(
                        p::FACING,
                        facing(context.horizontal_direction(&reader)?.opposite()),
                    )
                    .unwrap()
                    .with(p::WATERLOGGED, waterlogged)
                    .unwrap()
            }
            _ => state
                .with(
                    p::FACING,
                    facing(context.horizontal_direction(&reader)?.opposite()),
                )
                .unwrap(),
        }))
    }
}

#[cfg(test)]
mod furniture_tests;
#[cfg(test)]
mod helper_tests;
#[cfg(test)]
mod post_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod support_tests;
