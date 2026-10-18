//! Remaining furniture/utility placement virtuals and BellBlock support queries.
use super::{direction, facing};
use crate::{
    block::{BlockMethod, BlockPlaceContext, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid, tags::FluidTags,
};
use ferraloom_protocol::component::{ComponentTypeId, DataComponent};
use ferraloom_util::{Axis, BlockPos, Direction};

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    if method == BlockMethod::CanSurvive {
        return block.class() == C::BellBlock;
    }
    method == BlockMethod::GetStateForPlacement
        && matches!(
            block.class(),
            C::ChiseledBookShelfBlock
                | C::ShelfBlock
                | C::DecoratedPotBlock
                | C::LecternBlock
                | C::BellBlock
                | C::BeehiveBlock
                | C::CampfireBlock
                | C::CrafterBlock
                | C::ConduitBlock
                | C::CopperGolemStatueBlock
                | C::WeatheringCopperGolemStatueBlock
                | C::CommandBlock
                | C::JigsawBlock
                | C::TestBlock
                | C::VaultBlock
        )
}
/// Port of FrontAndTop#fromFrontAndTop for the twelve valid front/top pairs.
fn orientation(front: Direction, top: Direction) -> p::Orientation {
    use Direction::*;
    match (front, top) {
        (Down, East) => p::Orientation::DownEast,
        (Down, North) => p::Orientation::DownNorth,
        (Down, South) => p::Orientation::DownSouth,
        (Down, West) => p::Orientation::DownWest,
        (Up, East) => p::Orientation::UpEast,
        (Up, North) => p::Orientation::UpNorth,
        (Up, South) => p::Orientation::UpSouth,
        (Up, West) => p::Orientation::UpWest,
        (West, Up) => p::Orientation::WestUp,
        (East, Up) => p::Orientation::EastUp,
        (North, Up) => p::Orientation::NorthUp,
        (South, Up) => p::Orientation::SouthUp,
        _ => unreachable!("native front/top placement pair"),
    }
}
/// Port of BellBlock#canSurvive and FaceAttachedHorizontalDirectionalBlock#canAttach.
pub(super) fn can_survive(
    state: BlockState,
    pos: BlockPos,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let connection = match state.get(p::ATTACHMENT).unwrap() {
        p::Attachment::Floor => Direction::Down,
        p::Attachment::Ceiling => Direction::Up,
        _ => direction(state.get(p::FACING).unwrap()),
    };
    if connection == Direction::Up {
        return reader.can_support_center(pos.above(1), Direction::Down);
    }
    let support = pos.relative(connection, 1);
    reader.is_face_sturdy(
        reader.get_block_state(support)?,
        support,
        connection.opposite(),
    )
}
fn sturdy(
    pos: BlockPos,
    face: Direction,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    reader.is_face_sturdy(reader.get_block_state(pos)?, pos, face)
}
/// Port of BellBlock#getStateForPlacement. Horizontal failure tests the floor
/// first and then validates the selected floor/ceiling fallback normally.
fn bell(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockState>, LevelError> {
    let face = context.clicked_face();
    let pos = context.clicked_pos();
    if face.axis() == Axis::Y {
        let placed = block
            .default_state()
            .with(
                p::ATTACHMENT,
                if face == Direction::Down {
                    p::Attachment::Ceiling
                } else {
                    p::Attachment::Floor
                },
            )
            .unwrap()
            .with(p::FACING, facing(context.horizontal_direction(reader)?))
            .unwrap();
        return Ok(reader.can_survive(placed, pos)?.then_some(placed));
    }
    let double = if face.axis() == Axis::X {
        sturdy(pos.relative(Direction::West, 1), Direction::East, reader)?
            && sturdy(pos.relative(Direction::East, 1), Direction::West, reader)?
    } else {
        sturdy(pos.relative(Direction::North, 1), Direction::South, reader)?
            && sturdy(pos.relative(Direction::South, 1), Direction::North, reader)?
    };
    let placed = block
        .default_state()
        .with(p::FACING, facing(face.opposite()))
        .unwrap()
        .with(
            p::ATTACHMENT,
            if double {
                p::Attachment::DoubleWall
            } else {
                p::Attachment::SingleWall
            },
        )
        .unwrap();
    if reader.can_survive(placed, pos)? {
        return Ok(Some(placed));
    }
    let floor = sturdy(pos.below(1), Direction::Up, reader)?;
    let placed = placed
        .with(
            p::ATTACHMENT,
            if floor {
                p::Attachment::Floor
            } else {
                p::Attachment::Ceiling
            },
        )
        .unwrap();
    Ok(reader.can_survive(placed, pos)?.then_some(placed))
}
/// Port of LecternBlock#getStateForPlacement. Game-master permissions are a
/// required canonical Level query, independent of Creative game mode.
fn lectern(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let stack = context.item_in_hand(reader)?;
    let can_use = if let Some(player) = context.player() {
        match reader {
            LevelReaderRef::Server(level) => level.can_use_game_master_blocks(player)?,
            LevelReaderRef::Worldgen(_) => return Err(LevelError::UnsupportedWorldgenEnvironment),
        }
    } else {
        false
    };
    let has_book = if can_use {
        match stack
            .component(ComponentTypeId::BLOCK_ENTITY_DATA)
            .map_err(|_| LevelError::Inventory("lectern placement components"))?
        {
            Some(DataComponent::BlockEntityData(value)) => match &value.tag.0 {
                ferraloom_nbt::Tag::Compound(tag) => tag.get("Book").is_some(),
                _ => false,
            },
            _ => false,
        }
    } else {
        false
    };
    Ok(block
        .default_state()
        .with(
            p::FACING,
            facing(context.horizontal_direction(reader)?.opposite()),
        )
        .unwrap()
        .with(p::HAS_BOOK, has_book)
        .unwrap())
}
/// Ports the utility furniture classes' getStateForPlacement methods. Only the
/// native Lectern and TestBlock overrides read item components here.
pub(super) fn get_state_for_placement(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockState>, LevelError> {
    let state = block.default_state();
    let pos = context.clicked_pos();
    let placed = match block.class() {
        C::BellBlock => return bell(block, context, reader),
        C::LecternBlock => lectern(block, context, reader)?,
        C::ShelfBlock => {
            let waterlogged = reader.get_fluid_state(pos)?.fluid_type == fluid::Id::WATER;
            state
                .with(
                    p::FACING,
                    facing(context.horizontal_direction(reader)?.opposite()),
                )
                .unwrap()
                .with(p::POWERED, reader.has_neighbor_signal(pos)?)
                .unwrap()
                .with(p::WATERLOGGED, waterlogged)
                .unwrap()
        }
        C::DecoratedPotBlock => {
            let waterlogged = reader.get_fluid_state(pos)?.fluid_type == fluid::Id::WATER;
            state
                .with(p::FACING, facing(context.horizontal_direction(reader)?))
                .unwrap()
                .with(p::WATERLOGGED, waterlogged)
                .unwrap()
                .with(p::CRACKED, false)
                .unwrap()
        }
        C::CampfireBlock => {
            let waterlogged = reader.get_fluid_state(pos)?.fluid_type == fluid::Id::WATER;
            let smoke = reader.get_block_state(pos.below(1))?.block() == Block::HAY_BLOCK;
            state
                .with(p::WATERLOGGED, waterlogged)
                .unwrap()
                .with(p::SIGNAL_FIRE, smoke)
                .unwrap()
                .with(p::LIT, !waterlogged)
                .unwrap()
                .with(p::FACING, facing(context.horizontal_direction(reader)?))
                .unwrap()
        }
        C::CrafterBlock => {
            let front = context.nearest_looking_direction(reader)?.opposite();
            let top = match front {
                Direction::Down => context.horizontal_direction(reader)?.opposite(),
                Direction::Up => context.horizontal_direction(reader)?,
                _ => Direction::Up,
            };
            state
                .with(p::ORIENTATION, orientation(front, top))
                .unwrap()
                .with(p::TRIGGERED, reader.has_neighbor_signal(pos)?)
                .unwrap()
        }
        C::JigsawBlock => {
            let front = context.clicked_face();
            let top = if front.axis() == Axis::Y {
                context.horizontal_direction(reader)?.opposite()
            } else {
                Direction::Up
            };
            state.with(p::ORIENTATION, orientation(front, top)).unwrap()
        }
        C::ConduitBlock => {
            let fluid = reader.get_fluid_state(pos)?;
            state
                .with(
                    p::WATERLOGGED,
                    reader
                        .fluid_tags()?
                        .contains_key(FluidTags::WATER, fluid.fluid_type)
                        && fluid.amount == 8,
                )
                .unwrap()
        }
        C::CopperGolemStatueBlock | C::WeatheringCopperGolemStatueBlock => {
            let waterlogged = reader.get_fluid_state(pos)?.fluid_type == fluid::Id::WATER;
            state
                .with(
                    p::FACING,
                    facing(context.horizontal_direction(reader)?.opposite()),
                )
                .unwrap()
                .with(p::WATERLOGGED, waterlogged)
                .unwrap()
        }
        C::CommandBlock => state
            .with(
                p::FACING,
                facing(context.nearest_looking_direction(reader)?.opposite()),
            )
            .unwrap(),
        C::TestBlock => {
            let properties = context
                .item_in_hand(reader)?
                .component(ComponentTypeId::BLOCK_STATE)
                .map_err(|_| LevelError::Inventory("test block placement components"))?;
            let mode = match properties {
                Some(DataComponent::BlockState(values)) => values
                    .0
                    .iter()
                    .rev()
                    .find(|(key, _)| key == "mode")
                    .and_then(|(_, value)| match value.as_str() {
                        "start" => Some(p::Mode::Start),
                        "log" => Some(p::Mode::Log),
                        "fail" => Some(p::Mode::Fail),
                        "accept" => Some(p::Mode::Accept),
                        _ => None,
                    }),
                _ => None,
            };
            mode.map_or(state, |mode| state.with(p::MODE, mode).unwrap())
        }
        C::ChiseledBookShelfBlock | C::BeehiveBlock | C::VaultBlock => state
            .with(
                p::FACING,
                facing(context.horizontal_direction(reader)?.opposite()),
            )
            .unwrap(),
        _ => unreachable!("registered furniture placement class"),
    };
    Ok(Some(placed))
}
