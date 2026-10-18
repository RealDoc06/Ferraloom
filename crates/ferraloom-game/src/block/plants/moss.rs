//! Port of MossyCarpetBlock's connected sides and worldgen placement.
use super::{
    super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags},
    multiface,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction, core::Plane, random::RandomSource};
pub struct MossyCarpet;
pub static MOSSY_CARPET: MossyCarpet = MossyCarpet;
// Direction properties have separate generated types. Normalize only the values
// used by this behavior, preserving typed state reads and writes.
fn side(state: BlockState, face: Direction) -> p::North {
    macro_rules! read {
        ($property:ident,$ty:ident) => {
            match state.get(p::$property).unwrap() {
                p::$ty::Low => p::North::Low,
                p::$ty::Tall => p::North::Tall,
                _ => p::North::None,
            }
        };
    }
    match face {
        Direction::North => read!(NORTH, North),
        Direction::East => read!(EAST, East),
        Direction::South => read!(SOUTH, South),
        Direction::West => read!(WEST, West),
        _ => unreachable!("horizontal carpet side"),
    }
}
fn with_side(state: BlockState, face: Direction, value: p::North) -> BlockState {
    macro_rules! write {
        ($property:ident,$ty:ident) => {
            state
                .with(
                    p::$property,
                    match value {
                        p::North::Low => p::$ty::Low,
                        p::North::Tall => p::$ty::Tall,
                        _ => p::$ty::None,
                    },
                )
                .unwrap()
        };
    }
    match face {
        Direction::North => write!(NORTH, North),
        Direction::East => write!(EAST, East),
        Direction::South => write!(SOUTH, South),
        Direction::West => write!(WEST, West),
        _ => unreachable!("horizontal carpet side"),
    }
}
fn has_faces(state: BlockState) -> bool {
    state.get(p::BOTTOM) == Some(true)
        || Plane::Horizontal
            .faces()
            .iter()
            .any(|&d| side(state, d) != p::North::None)
}
pub(super) fn updated(
    mut state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
    create: bool,
) -> Result<BlockState, LevelError> {
    let base = state.get(p::BOTTOM) == Some(true);
    for &d in Plane::Horizontal.faces() {
        let mut value = if multiface::can_attach_to(level, pos, d)? {
            if create || base {
                p::North::Low
            } else {
                side(state, d)
            }
        } else {
            p::North::None
        };
        if value == p::North::Low {
            let above = level.get_block_state(pos.above(1))?;
            if above.block() == Block::PALE_MOSS_CARPET
                && above.get(p::BOTTOM) == Some(false)
                && side(above, d) != p::North::None
            {
                value = p::North::Tall;
            }
            if !base {
                let below = level.get_block_state(pos.below(1))?;
                if below.block() == Block::PALE_MOSS_CARPET && side(below, d) == p::North::None {
                    value = p::North::None;
                }
            }
        }
        state = with_side(state, d, value);
    }
    Ok(state)
}
impl BlockBehavior for MossyCarpet {
    fn set_placed_by(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut crate::level::Level,
    ) -> Result<(), LevelError> {
        let mut access = LevelAccessRef::Server(level);
        let topper = random_topper(&mut access, pos)?;
        if !topper.is_air() {
            access.set_block(pos.above(1), topper, UpdateFlags::ALL)?;
        }
        Ok(())
    }

    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        Ok(if state.get(p::BOTTOM) == Some(true) {
            !below.is_air()
        } else {
            below.block() == state.block() && below.get(p::BOTTOM) == Some(true)
        })
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
        if !self.can_survive(state, pos, &level.as_reader())? {
            return Ok(Block::AIR.default_state());
        }
        let next = updated(state, pos, &level.as_reader(), false)?;
        Ok(if has_faces(next) {
            next
        } else {
            Block::AIR.default_state()
        })
    }
}
/// Port of MossyCarpetBlock#placeAt; side draws use the level's random stream.
pub fn place_at(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    flags: UpdateFlags,
) -> Result<(), LevelError> {
    let bottom = updated(
        Block::PALE_MOSS_CARPET.default_state(),
        pos,
        &level.as_reader(),
        true,
    )?;
    level.set_block(pos, bottom, flags)?;
    let topper = random_topper(level, pos)?;
    if !topper.is_air() {
        level.set_block(pos.above(1), topper, flags)?;
        let adjusted = updated(bottom, pos, &level.as_reader(), true)?;
        level.set_block(pos, adjusted, flags)?;
    }
    Ok(())
}

/// Port of createTopperWithSideChance using the canonical level RNG.
fn random_topper(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
) -> Result<BlockState, LevelError> {
    let above_pos = pos.above(1);
    let old = level.get_block_state(above_pos)?;
    let is_carpet = old.block() == Block::PALE_MOSS_CARPET;
    if (is_carpet && old.get(p::BOTTOM) == Some(true)) || (!is_carpet && !old.data().replaceable) {
        return Ok(Block::AIR.default_state());
    }
    let mut topper = updated(
        Block::PALE_MOSS_CARPET
            .default_state()
            .with(p::BOTTOM, false)
            .unwrap(),
        above_pos,
        &level.as_reader(),
        true,
    )?;
    for &d in Plane::Horizontal.faces() {
        if side(topper, d) != p::North::None && !level.random()?.next_boolean() {
            topper = with_side(topper, d, p::North::None);
        }
    }
    Ok(if has_faces(topper) && topper != old {
        topper
    } else {
        Block::AIR.default_state()
    })
}

/// Port of createTopperWithSideChance with the bonemeal all-sides predicate.
pub fn bonemeal_topper(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<BlockState, LevelError> {
    let at = pos.above(1);
    let old = level.get_block_state(at)?;
    let is_carpet = old.block() == Block::PALE_MOSS_CARPET;
    if (is_carpet && old.get(p::BOTTOM) == Some(true)) || (!is_carpet && !old.data().replaceable) {
        return Ok(Block::AIR.default_state());
    }
    let next = updated(
        Block::PALE_MOSS_CARPET
            .default_state()
            .with(p::BOTTOM, false)
            .unwrap(),
        at,
        level,
        true,
    )?;
    Ok(if has_faces(next) && next != old {
        next
    } else {
        Block::AIR.default_state()
    })
}
