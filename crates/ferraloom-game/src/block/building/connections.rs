//! WallBlock, FenceBlock and IronBarsBlock placement and connection updates.
use super::{facing, schedule_water};
use crate::{
    block::{
        BlockBehavior, BlockCollisionContext, BlockMethod, BlockPlaceContext, LevelAccessRef,
        LevelReaderRef,
    },
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
    tags::BlockTags,
};
use ferraloom_util::{
    BlockPos, Direction, VoxelShape,
    shapes::{self, BooleanOp},
};
use std::sync::LazyLock;

pub(super) struct Connections;
pub(super) static CONNECTIONS: Connections = Connections;
const SIDES: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(
        block.class(),
        C::WallBlock
            | C::FenceBlock
            | C::IronBarsBlock
            | C::WeatheringCopperBarsBlock
            | C::StainedGlassPaneBlock
    ) && matches!(
        method,
        BlockMethod::GetStateForPlacement
            | BlockMethod::UpdateShape
            | BlockMethod::UpdateFromNeighbourShapes
    )
}
fn bars(block: Block) -> bool {
    block.is_instance_of(C::IronBarsBlock)
}

/// Port of Block#isExceptionForConnection. The shulker membership stays a live
/// typed tag query; native explicit identities and the LeavesBlock test precede it.
fn exception(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    let block = state.block();
    if block.is_instance_of(C::LeavesBlock)
        || matches!(
            block,
            Block::BARRIER
                | Block::CARVED_PUMPKIN
                | Block::JACK_O_LANTERN
                | Block::MELON
                | Block::PUMPKIN
        )
    {
        return Ok(true);
    }
    Ok(level
        .block_tags()?
        .contains_key(BlockTags::SHULKER_BOXES, block))
}
fn gate_connects(state: BlockState, side: Direction) -> bool {
    state.block().is_instance_of(C::FenceGateBlock)
        && facing(state).axis() == side.clockwise().axis()
}
fn connects(
    block: Block,
    state: BlockState,
    solid: bool,
    side: Direction,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let tags = level.block_tags()?;
    if block.class() == C::FenceBlock {
        let same_fence = tags.contains_key(BlockTags::FENCES, state.block())
            && tags.contains_key(BlockTags::WOODEN_FENCES, state.block())
                == tags.contains_key(BlockTags::WOODEN_FENCES, block);
        let gate = gate_connects(state, side);
        return Ok(!exception(state, level)? && solid || same_fence || gate);
    }
    if block.class() == C::WallBlock {
        let gate = gate_connects(state, side);
        return Ok(tags.contains_key(BlockTags::WALLS, state.block())
            || !exception(state, level)? && solid
            || bars(state.block())
            || gate);
    }
    Ok(!exception(state, level)? && solid
        || bars(state.block())
        || tags.contains_key(BlockTags::WALLS, state.block()))
}
fn with_connection(state: BlockState, side: Direction, value: bool) -> BlockState {
    macro_rules! write {
        ($property:ident,$ty:ident) => {
            state
                .with(
                    p::$property,
                    if value { p::$ty::True } else { p::$ty::False },
                )
                .unwrap()
        };
    }
    match side {
        Direction::North => write!(NORTH, North),
        Direction::East => write!(EAST, East),
        Direction::South => write!(SOUTH, South),
        Direction::West => write!(WEST, West),
        _ => unreachable!("horizontal building connection"),
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum WallSide {
    None,
    Low,
    Tall,
}
fn wall_side(state: BlockState, side: Direction) -> WallSide {
    macro_rules! read {
        ($property:ident,$ty:ident) => {
            match state.get(p::$property).unwrap() {
                p::$ty::None => WallSide::None,
                p::$ty::Low => WallSide::Low,
                p::$ty::Tall => WallSide::Tall,
                _ => unreachable!("wall side"),
            }
        };
    }
    match side {
        Direction::North => read!(NORTH, North),
        Direction::East => read!(EAST, East),
        Direction::South => read!(SOUTH, South),
        Direction::West => read!(WEST, West),
        _ => unreachable!("horizontal wall direction"),
    }
}
fn with_wall_side(state: BlockState, side: Direction, value: WallSide) -> BlockState {
    macro_rules! write {
        ($property:ident,$ty:ident) => {
            state
                .with(
                    p::$property,
                    match value {
                        WallSide::None => p::$ty::None,
                        WallSide::Low => p::$ty::Low,
                        WallSide::Tall => p::$ty::Tall,
                    },
                )
                .unwrap()
        };
    }
    match side {
        Direction::North => write!(NORTH, North),
        Direction::East => write!(EAST, East),
        Direction::South => write!(SOUTH, South),
        Direction::West => write!(WEST, West),
        _ => unreachable!("horizontal wall direction"),
    }
}
// WallBlock's two-pixel coverage probes, in NORTH/EAST/SOUTH/WEST order.
static WALL_TESTS: LazyLock<[VoxelShape; 4]> = LazyLock::new(|| {
    [
        shapes::box_shape(7.0 / 16.0, 0.0, 0.0, 9.0 / 16.0, 1.0, 9.0 / 16.0),
        shapes::box_shape(7.0 / 16.0, 0.0, 7.0 / 16.0, 1.0, 1.0, 9.0 / 16.0),
        shapes::box_shape(7.0 / 16.0, 0.0, 7.0 / 16.0, 9.0 / 16.0, 1.0, 1.0),
        shapes::box_shape(0.0, 0.0, 7.0 / 16.0, 9.0 / 16.0, 1.0, 9.0 / 16.0),
    ]
});
static POST_TEST: LazyLock<VoxelShape> =
    LazyLock::new(|| shapes::box_shape(7.0 / 16.0, 0.0, 7.0 / 16.0, 9.0 / 16.0, 1.0, 9.0 / 16.0));
fn covered(above: &VoxelShape, test: &VoxelShape) -> bool {
    !shapes::join_is_not_empty(test, above, BooleanOp::OnlyFirst)
}
fn connections(state: BlockState) -> [bool; 4] {
    SIDES.map(|side| wall_side(state, side) != WallSide::None)
}

/// Port of WallBlock's shared updateShape/updateSides/shouldRaisePost helpers.
fn wall_shape(
    level: &LevelReaderRef<'_, '_>,
    mut state: BlockState,
    top_pos: BlockPos,
    top: BlockState,
    connections: [bool; 4],
) -> Result<BlockState, LevelError> {
    let above = level
        .collision_shape(top, top_pos, &BlockCollisionContext::empty())?
        .get_face_shape(Direction::Down);
    for ((side, connects), test) in SIDES.into_iter().zip(connections).zip(WALL_TESTS.iter()) {
        let value = if !connects {
            WallSide::None
        } else if covered(&above, test) {
            WallSide::Tall
        } else {
            WallSide::Low
        };
        state = with_wall_side(state, side, value);
    }
    if top.block().is_instance_of(C::WallBlock) && top.get(p::UP) == Some(true) {
        return Ok(state.with(p::UP, true).unwrap());
    }
    let north = wall_side(state, Direction::North);
    let south = wall_side(state, Direction::South);
    let east = wall_side(state, Direction::East);
    let west = wall_side(state, Direction::West);
    let n = north == WallSide::None;
    let s = south == WallSide::None;
    let e = east == WallSide::None;
    let w = west == WallSide::None;
    let corner = (n && s && w && e) || n != s || w != e;
    if corner {
        return Ok(state.with(p::UP, true).unwrap());
    }
    let high = (north == WallSide::Tall && south == WallSide::Tall)
        || (east == WallSide::Tall && west == WallSide::Tall);
    let post = !high
        && (level
            .block_tags()?
            .contains_key(BlockTags::WALL_POST_OVERRIDE, top.block())
            || covered(&above, &POST_TEST));
    Ok(state.with(p::UP, post).unwrap())
}

impl BlockBehavior for Connections {
    /// Ports WallBlock/FenceBlock/IronBarsBlock#getStateForPlacement, retaining
    /// each native family's distinct neighbor snapshot and face-query order.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let water = reader.get_fluid_state(pos)?.fluid_type == Fluid::WATER;
        let directions = if bars(block) {
            [
                Direction::North,
                Direction::South,
                Direction::West,
                Direction::East,
            ]
        } else {
            SIDES
        };
        let positions = directions.map(|side| pos.relative(side, 1));
        let neighbors = [
            reader.get_block_state(positions[0])?,
            reader.get_block_state(positions[1])?,
            reader.get_block_state(positions[2])?,
            reader.get_block_state(positions[3])?,
        ];
        let top = if block.class() == C::WallBlock {
            Some(reader.get_block_state(pos.above(1))?)
        } else {
            None
        };
        let mut state = block.default_state().with(p::WATERLOGGED, water).unwrap();
        let mut connected = [false; 4];
        for (index, ((side, at), neighbor)) in directions
            .into_iter()
            .zip(positions)
            .zip(neighbors)
            .enumerate()
        {
            let solid = reader.is_face_sturdy(neighbor, at, side.opposite())?;
            let value = connects(block, neighbor, solid, side.opposite(), &reader)?;
            if block.class() == C::WallBlock {
                connected[index] = value;
            } else {
                state = with_connection(state, side, value);
            }
        }
        if let Some(top) = top {
            state = wall_shape(&reader, state, pos.above(1), top, connected)?;
        }
        Ok(Some(state))
    }

    /// Ports each family's updateShape, using the captured neighbor for the
    /// changed side while retaining all other connections from the input state.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        schedule_water(state, pos, level)?;
        let block = state.block();
        let reader = level.as_reader();
        if block.class() == C::WallBlock {
            if direction == Direction::Down {
                return Ok(state);
            }
            let mut sides = connections(state);
            if direction == Direction::Up {
                return wall_shape(&reader, state, neighbor_pos, neighbor, sides);
            }
            let index = SIDES.iter().position(|side| *side == direction).unwrap();
            sides[index] = connects(
                block,
                neighbor,
                reader.is_face_sturdy(neighbor, neighbor_pos, direction.opposite())?,
                direction.opposite(),
                &reader,
            )?;
            let above = pos.above(1);
            return wall_shape(&reader, state, above, reader.get_block_state(above)?, sides);
        }
        if !direction.axis().is_horizontal() {
            return Ok(state);
        }
        let value = connects(
            block,
            neighbor,
            reader.is_face_sturdy(neighbor, neighbor_pos, direction.opposite())?,
            direction.opposite(),
            &reader,
        )?;
        Ok(with_connection(state, direction, value))
    }
}
