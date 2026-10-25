//! Item-side placement subclasses. Block virtuals remain in their family registries.
use super::{ItemBehavior, required_behavior};
use crate::{
    block::{
        BlockCollisionContext, BlockPlaceContext, InteractionResult, LevelReaderRef, UPDATE_LIMIT,
        UpdateFlags, UseOnContext,
    },
    entity::player::send_build_limit_message,
    inventory::StackOperation,
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockState, Item, ItemClass, ItemMethod, registries::sound_event, tags::FluidTags,
};
use ferraloom_util::Direction;

struct DoubleHigh;
impl ItemBehavior for DoubleHigh {
    /// Port of DoubleHighBlockItem#placeBlock; clearing the upper half precedes base placement.
    fn place_block(
        &self,
        _item: Item,
        context: &BlockPlaceContext<'_>,
        state: BlockState,
        operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        let above = context.clicked_pos().above(1);
        let reader = LevelReaderRef::Server(operation.level);
        let fluid = reader.get_fluid_state(above)?;
        let block = if reader
            .fluid_tags()?
            .contains_key(FluidTags::WATER, fluid.fluid_type)
        {
            Block::WATER
        } else {
            Block::AIR
        };
        operation
            .level
            .set_block(above, block.default_state(), UpdateFlags(27), UPDATE_LIMIT)?;
        operation.level.set_block(
            context.clicked_pos(),
            state,
            UpdateFlags::ALL_IMMEDIATE,
            UPDATE_LIMIT,
        )
    }
}
static DOUBLE_HIGH: DoubleHigh = DoubleHigh;

struct StandingAndWall;
impl ItemBehavior for StandingAndWall {
    fn get_placement_state(
        &self,
        item: Item,
        context: &BlockPlaceContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<BlockState>, LevelError> {
        standing_or_wall_state(item, context, operation)
    }
}
static STANDING_AND_WALL: StandingAndWall = StandingAndWall;

/// Port of StandingAndWallBlockItem#getPlacementState. The wall query happens
/// first, even if the first tested direction chooses the standing block.
fn standing_or_wall_state(
    item: Item,
    context: &BlockPlaceContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<BlockState>, LevelError> {
    let wall = item.wall_block().expect("standing/wall item constructor");
    let attachment = item
        .attachment_direction()
        .expect("standing/wall attachment");
    let wall_state = operation.level.get_state_for_placement(wall, context)?;
    let pos = context.clicked_pos();
    let directions =
        context.nearest_looking_directions(&LevelReaderRef::Server(operation.level))?;
    for direction in directions {
        if direction == attachment.opposite() {
            continue;
        }
        let possible = if direction == attachment {
            let block =
                required_behavior(item, ItemMethod::GetBlock)?.get_block(item, operation)?;
            operation.level.get_state_for_placement(block, context)?
        } else {
            wall_state
        };
        if let Some(state) = possible
            && required_behavior(item, ItemMethod::CanPlaceStandingOrWall)?
                .can_place_standing_or_wall(
                    item,
                    state,
                    pos,
                    &LevelReaderRef::Server(operation.level),
                )?
        {
            return Ok(operation
                .level
                .is_unobstructed(state, pos, &BlockCollisionContext::empty())?
                .then_some(state));
        }
    }
    Ok(None)
}

struct GameMaster;
impl ItemBehavior for GameMaster {
    /// Port of GameMasterBlockItem#getPlacementState, before any block query.
    fn get_placement_state(
        &self,
        item: Item,
        context: &BlockPlaceContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<BlockState>, LevelError> {
        if let Some(player) = context.player()
            && !operation.level.can_use_game_master_blocks(player)?
        {
            return Ok(None);
        }
        super::behavior::base_get_placement_state(item, context, operation)
    }
}
static GAME_MASTER: GameMaster = GameMaster;

struct Scaffolding;
impl ItemBehavior for Scaffolding {
    fn must_survive(
        &self,
        _item: Item,
        _operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of ScaffoldingBlockItem#updatePlacementContext, including the seven
    /// horizontal-block limit and the different unbounded vertical traversal.
    fn update_placement_context<'stack>(
        &self,
        item: Item,
        context: BlockPlaceContext<'stack>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<BlockPlaceContext<'stack>>, LevelError> {
        let pos = context.clicked_pos();
        let block = required_behavior(item, ItemMethod::GetBlock)?.get_block(item, operation)?;
        if operation.level.get_block_state(pos)?.block() != block {
            return Ok((crate::block::building::scaffolding_distance(
                &LevelReaderRef::Server(operation.level),
                pos,
            )? != 7)
                .then_some(context));
        }
        let reader = LevelReaderRef::Server(operation.level);
        let direction = if context.is_secondary_use_active(&reader)? {
            if context.is_inside() {
                context.clicked_face().opposite()
            } else {
                context.clicked_face()
            }
        } else if context.clicked_face() == Direction::Up {
            context.horizontal_direction(&reader)?
        } else {
            Direction::Up
        };
        let mut distance = 0;
        let mut target = pos.relative(direction, 1);
        while distance < 7 {
            // Level#isInWorldBounds is narrower than its chunk-coordinate
            // isInValidBounds check; preserve the gameplay horizontal limits.
            if target.y < operation.level.min_build_height()
                || target.y > operation.level.max_build_height()
                || !(-30_000_000..30_000_000).contains(&target.x)
                || !(-30_000_000..30_000_000).contains(&target.z)
            {
                if let Some(player) = context.player()
                    && target.y > operation.level.max_build_height()
                {
                    send_build_limit_message(
                        player,
                        true,
                        operation.level.max_build_height(),
                        operation.level,
                    )?;
                }
                break;
            }
            let state = operation.level.get_block_state(target)?;
            if state.block()
                != required_behavior(item, ItemMethod::GetBlock)?.get_block(item, operation)?
            {
                let reader = LevelReaderRef::Server(operation.level);
                return if reader.can_be_replaced(state, &context)? {
                    Ok(Some(BlockPlaceContext::at(
                        &reader, &context, target, direction,
                    )?))
                } else {
                    Ok(None)
                };
            }
            target = target.relative(direction, 1);
            if direction.axis().is_horizontal() {
                distance += 1;
            }
        }
        Ok(None)
    }
}
static SCAFFOLDING: Scaffolding = Scaffolding;

struct SolidBucket;
impl ItemBehavior for SolidBucket {
    fn get_place_sound(
        &self,
        item: Item,
        _state: BlockState,
        _operation: &mut StackOperation<'_>,
    ) -> Result<sound_event::Id, LevelError> {
        Ok(item.place_sound().expect("solid bucket constructor"))
    }
}
static SOLID_BUCKET: SolidBucket = SolidBucket;

struct PlaceOnWater;
impl ItemBehavior for PlaceOnWater {
    /// Port of PlaceOnWaterBlockItem#useOn. Its air-use raycast is separate.
    fn use_on(
        &self,
        _item: Item,
        _receiver: crate::inventory::StackRef,
        _context: &UseOnContext<'_>,
        _operation: &mut StackOperation<'_>,
    ) -> Result<Option<InteractionResult>, LevelError> {
        Ok(Some(InteractionResult::Pass))
    }
}
static PLACE_ON_WATER: PlaceOnWater = PlaceOnWater;

pub(super) fn ported_behavior(
    method: ItemMethod,
    owner: ItemClass,
) -> Option<&'static dyn ItemBehavior> {
    match (method, owner) {
        (ItemMethod::PlaceBlock, ItemClass::DoubleHighBlockItem) => Some(&DOUBLE_HIGH),
        (
            ItemMethod::GetPlacementState | ItemMethod::CanPlaceStandingOrWall,
            ItemClass::StandingAndWallBlockItem,
        ) => Some(&STANDING_AND_WALL),
        (ItemMethod::GetPlacementState, ItemClass::GameMasterBlockItem) => Some(&GAME_MASTER),
        (
            ItemMethod::UpdatePlacementContext | ItemMethod::MustSurvive,
            ItemClass::ScaffoldingBlockItem,
        ) => Some(&SCAFFOLDING),
        (ItemMethod::GetPlaceSound, ItemClass::SolidBucketItem) => Some(&SOLID_BUCKET),
        (ItemMethod::UseOn, ItemClass::PlaceOnWaterBlockItem) => Some(&PLACE_ON_WATER),
        _ => None,
    }
}
