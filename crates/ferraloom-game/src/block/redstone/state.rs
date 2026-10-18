//! Typed access to the shared generated properties used by this family.
use ferraloom_data::{BlockState, properties as p};
use ferraloom_util::Direction;

pub(super) const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

pub(super) fn facing(state: BlockState) -> Direction {
    match state.get(p::FACING).unwrap() {
        p::Facing::Down => Direction::Down,
        p::Facing::Up => Direction::Up,
        p::Facing::North => Direction::North,
        p::Facing::South => Direction::South,
        p::Facing::West => Direction::West,
        p::Facing::East => Direction::East,
    }
}

/// Port of RedstoneSide. Each direction's generated property also serves other
/// block classes; these conversions accept only the wire's three legal values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RedstoneSide {
    None,
    Side,
    Up,
}
impl RedstoneSide {
    pub fn connected(self) -> bool {
        self != Self::None
    }
}
pub(super) fn side(state: BlockState, direction: Direction) -> RedstoneSide {
    macro_rules! read {
        ($prop:ident,$ty:ident) => {
            match state.get(p::$prop).unwrap() {
                p::$ty::None => RedstoneSide::None,
                p::$ty::Side => RedstoneSide::Side,
                p::$ty::Up => RedstoneSide::Up,
                _ => unreachable!("wire property"),
            }
        };
    }
    match direction {
        Direction::North => read!(NORTH, North),
        Direction::East => read!(EAST, East),
        Direction::South => read!(SOUTH, South),
        Direction::West => read!(WEST, West),
        _ => unreachable!("horizontal wire direction"),
    }
}
pub(super) fn with_side(state: BlockState, direction: Direction, side: RedstoneSide) -> BlockState {
    macro_rules! write {
        ($prop:ident,$ty:ident) => {
            state
                .with(
                    p::$prop,
                    match side {
                        RedstoneSide::None => p::$ty::None,
                        RedstoneSide::Side => p::$ty::Side,
                        RedstoneSide::Up => p::$ty::Up,
                    },
                )
                .unwrap()
        };
    }
    match direction {
        Direction::North => write!(NORTH, North),
        Direction::East => write!(EAST, East),
        Direction::South => write!(SOUTH, South),
        Direction::West => write!(WEST, West),
        _ => unreachable!("horizontal wire direction"),
    }
}
