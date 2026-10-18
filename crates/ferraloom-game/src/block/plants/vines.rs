//! Port of VineBlock's attachment maintenance, placement and random spreading.
use super::{
    super::{BlockBehavior, BlockPlaceContext, LevelAccessRef, LevelReaderRef, UpdateFlags},
    multiface::{can_attach_to_neighbor, has_any_face, has_face, with_face},
};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos, Direction, core::Plane, random::RandomSource};
pub struct Vines;
pub static VINES: Vines = Vines;
/// Port of VineBlock#isAcceptableNeighbour; pos is the neighbor's position.
pub fn acceptable_neighbor(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    toward: Direction,
) -> Result<bool, LevelError> {
    can_attach_to_neighbor(level, level.get_block_state(pos)?, pos, toward)
}
fn can_support_at_face(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    face: Direction,
) -> Result<bool, LevelError> {
    if face == Direction::Down {
        return Ok(false);
    }
    if acceptable_neighbor(level, pos.relative(face, 1), face)? {
        return Ok(true);
    }
    if face.axis().is_vertical() {
        return Ok(false);
    }
    let above = level.get_block_state(pos.above(1))?;
    Ok(above.block() == Block::VINE && has_face(above, face))
}

fn can_spread(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let mut count = 0;
    for z in -4..=4 {
        for y in -1..=1 {
            for x in -4..=4 {
                if level.get_block_state(pos.offset(x, y, z))?.block() == Block::VINE {
                    count += 1;
                    if count > 4 {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(true)
}
fn has_horizontal_connection(state: BlockState) -> bool {
    Plane::Horizontal
        .faces()
        .iter()
        .any(|&face| has_face(state, face))
}
fn write(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    LevelAccessRef::Server(level).set_block(pos, state, UpdateFlags::CLIENTS)?;
    Ok(())
}
fn updated_state(
    mut state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    if has_face(state, Direction::Up) {
        state = with_face(
            state,
            Direction::Up,
            acceptable_neighbor(level, pos.above(1), Direction::Down)?,
        )
        .unwrap();
    }
    for &face in Plane::Horizontal.faces() {
        if has_face(state, face) {
            let supported = acceptable_neighbor(level, pos.relative(face, 1), face)? || {
                let above = level.get_block_state(pos.above(1))?;
                above.block() == Block::VINE && has_face(above, face)
            };
            state = with_face(state, face, supported).unwrap();
        }
    }
    Ok(state)
}
impl BlockBehavior for Vines {
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let clicked = level.get_block_state(context.clicked_pos())?;
        if clicked.block() == Block::VINE {
            Ok(Direction::VALUES
                .into_iter()
                .filter(|&face| has_face(clicked, face))
                .count()
                < 5)
        } else {
            super::super::BASE_BLOCK.can_be_replaced(state, context, level)
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let pos = context.clicked_pos();
        let clicked = level.get_block_state(pos)?;
        let same = clicked.block() == block;
        let result = if same { clicked } else { block.default_state() };
        for face in context.nearest_looking_directions(level)? {
            if face != Direction::Down
                && !(same && has_face(clicked, face))
                && can_support_at_face(level, pos, face)?
            {
                return Ok(with_face(result, face, true));
            }
        }
        Ok(same.then_some(result))
    }

    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level.rules().spread_vines || level.random().next_int_bound(4) != 0 {
            return Ok(());
        }
        let direction = Direction::random(level.random());
        if direction.axis().is_horizontal() && !has_face(state, direction) {
            if !can_spread(&LevelReaderRef::Server(level), pos)? {
                return Ok(());
            }
            let at = pos.relative(direction, 1);
            let edge = level.get_block_state(at)?;
            if edge.is_air() {
                let cw = direction.clockwise();
                let ccw = direction.counter_clockwise();
                let has_cw = has_face(state, cw);
                let has_ccw = has_face(state, ccw);
                let cw_pos = at.relative(cw, 1);
                let ccw_pos = at.relative(ccw, 1);
                if has_cw && acceptable_neighbor(&LevelReaderRef::Server(level), cw_pos, cw)? {
                    write(
                        level,
                        at,
                        with_face(Block::VINE.default_state(), cw, true).unwrap(),
                    )?;
                } else if has_ccw
                    && acceptable_neighbor(&LevelReaderRef::Server(level), ccw_pos, ccw)?
                {
                    write(
                        level,
                        at,
                        with_face(Block::VINE.default_state(), ccw, true).unwrap(),
                    )?;
                } else {
                    let opposite = direction.opposite();
                    if has_cw
                        && level.get_block_state(cw_pos)?.is_air()
                        && acceptable_neighbor(
                            &LevelReaderRef::Server(level),
                            pos.relative(cw, 1),
                            opposite,
                        )?
                    {
                        write(
                            level,
                            cw_pos,
                            with_face(Block::VINE.default_state(), opposite, true).unwrap(),
                        )?;
                    } else if has_ccw
                        && level.get_block_state(ccw_pos)?.is_air()
                        && acceptable_neighbor(
                            &LevelReaderRef::Server(level),
                            pos.relative(ccw, 1),
                            opposite,
                        )?
                    {
                        write(
                            level,
                            ccw_pos,
                            with_face(Block::VINE.default_state(), opposite, true).unwrap(),
                        )?;
                    } else if f64::from(level.random().next_float()) < 0.05
                        && acceptable_neighbor(
                            &LevelReaderRef::Server(level),
                            at.above(1),
                            Direction::Up,
                        )?
                    {
                        write(
                            level,
                            at,
                            with_face(Block::VINE.default_state(), Direction::Up, true).unwrap(),
                        )?;
                    }
                }
            } else if acceptable_neighbor(&LevelReaderRef::Server(level), at, direction)? {
                write(level, pos, with_face(state, direction, true).unwrap())?;
            }
            return Ok(());
        }
        let chunk_pos = ChunkPos::containing(pos);
        let chunk = level
            .chunk(chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?;
        let (min_y, max_y) = (chunk.min_y(), chunk.max_y() - 1);
        if direction == Direction::Up && pos.y < max_y {
            if can_support_at_face(&LevelReaderRef::Server(level), pos, direction)? {
                return write(level, pos, with_face(state, Direction::Up, true).unwrap());
            }
            let above = pos.above(1);
            if level.get_block_state(above)?.is_air() {
                if !can_spread(&LevelReaderRef::Server(level), pos)? {
                    return Ok(());
                }
                let mut next = state;
                for &face in Plane::Horizontal.faces() {
                    if level.random().next_boolean()
                        || !acceptable_neighbor(
                            &LevelReaderRef::Server(level),
                            above.relative(face, 1),
                            face,
                        )?
                    {
                        next = with_face(next, face, false).unwrap();
                    }
                }
                if has_horizontal_connection(next) {
                    write(level, above, next)?;
                }
                return Ok(());
            }
        }
        if pos.y > min_y {
            let below = pos.below(1);
            let old = level.get_block_state(below)?;
            if old.is_air() || old.block() == Block::VINE {
                let before = if old.is_air() {
                    Block::VINE.default_state()
                } else {
                    old
                };
                let mut next = before;
                for &face in Plane::Horizontal.faces() {
                    if level.random().next_boolean() && has_face(state, face) {
                        next = with_face(next, face, true).unwrap();
                    }
                }
                if before != next && has_horizontal_connection(next) {
                    write(level, below, next)?;
                }
            }
        }
        Ok(())
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(has_any_face(updated_state(state, pos, level)?))
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if direction == Direction::Down {
            return Ok(state);
        }
        let next = updated_state(state, pos, &level.as_reader())?;
        Ok(if has_any_face(next) {
            next
        } else {
            Block::AIR.default_state()
        })
    }
}
