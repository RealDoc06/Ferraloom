//! Port of PistonBaseBlock#isPushable and PistonStructureResolver. The ordered
//! push/destroy lists are used as-is by movement, never coordinate-sorted.
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, PushReaction, properties};
use ferraloom_util::{BlockPos, Direction};

/// Port of PistonBaseBlock#isPushable, including the 26.3 PUSH reaction's
/// connection-direction check and its early return before block-entity checks.
pub fn is_pushable(
    state: BlockState,
    level: &Level,
    pos: BlockPos,
    direction: Direction,
    allow_destroyable: bool,
    connection_direction: Direction,
) -> bool {
    if pos.y < level.min_build_height()
        || pos.y > level.max_build_height()
        || !level.world_border().is_within_block(pos)
    {
        return false;
    }
    if state.is_air() {
        return true;
    }
    if (direction == Direction::Down && pos.y == level.min_build_height())
        || (direction == Direction::Up && pos.y == level.max_build_height())
    {
        return false;
    }
    if matches!(state.block(), Block::PISTON | Block::STICKY_PISTON) {
        if state.get(properties::EXTENDED).unwrap() {
            return false;
        }
    } else {
        if state.data().destroy_time == -1.0 {
            return false;
        }
        match state.data().push_reaction {
            PushReaction::Immoveable => return false,
            PushReaction::Popped => return allow_destroyable,
            PushReaction::Push => return direction == connection_direction,
            PushReaction::PushPull | PushReaction::IgnoreEntity => {}
        }
    }
    !state.has_block_entity()
}

pub fn is_sticky(state: BlockState) -> bool {
    matches!(state.block(), Block::SLIME_BLOCK | Block::HONEY_BLOCK)
}
/// Port of PistonStructureResolver#canStickToEachOther.
pub fn can_stick_to_each_other(first: BlockState, second: BlockState) -> bool {
    !matches!(
        (first.block(), second.block()),
        (Block::SLIME_BLOCK, Block::HONEY_BLOCK) | (Block::HONEY_BLOCK, Block::SLIME_BLOCK)
    ) && (is_sticky(first) || is_sticky(second))
}

/// Port of PistonStructureResolver. Reusing a resolver clears both lists before
/// every attempt, including attempts that fail after collecting some branches.
pub struct PistonStructureResolver {
    piston_pos: BlockPos,
    piston_direction: Direction,
    extending: bool,
    start_pos: BlockPos,
    push_direction: Direction,
    to_push: Vec<BlockPos>,
    to_destroy: Vec<BlockPos>,
}
impl PistonStructureResolver {
    pub const MAX_PUSH_DEPTH: usize = 12;
    pub fn new(piston_pos: BlockPos, direction: Direction, extending: bool) -> Self {
        Self {
            piston_pos,
            piston_direction: direction,
            extending,
            start_pos: piston_pos.relative(direction, if extending { 1 } else { 2 }),
            push_direction: if extending {
                direction
            } else {
                direction.opposite()
            },
            to_push: Vec::new(),
            to_destroy: Vec::new(),
        }
    }
    pub fn push_direction(&self) -> Direction {
        self.push_direction
    }
    pub fn to_push(&self) -> &[BlockPos] {
        &self.to_push
    }
    pub fn to_destroy(&self) -> &[BlockPos] {
        &self.to_destroy
    }
    pub fn resolve(&mut self, level: &Level) -> Result<bool, LevelError> {
        self.to_push.clear();
        self.to_destroy.clear();
        let next = level.get_block_state(self.start_pos)?;
        if !is_pushable(
            next,
            level,
            self.start_pos,
            self.push_direction,
            false,
            self.piston_direction,
        ) {
            if self.extending && next.data().push_reaction == PushReaction::Popped {
                self.to_destroy.push(self.start_pos);
                return Ok(true);
            }
            return Ok(false);
        }
        if !self.add_block_line(level, self.start_pos, self.push_direction)? {
            return Ok(false);
        }
        let mut i = 0;
        while i < self.to_push.len() {
            let pos = self.to_push[i];
            if is_sticky(level.get_block_state(pos)?) && !self.add_branching_blocks(level, pos)? {
                return Ok(false);
            }
            i += 1;
        }
        Ok(true)
    }
    fn add_block_line(
        &mut self,
        level: &Level,
        start: BlockPos,
        direction: Direction,
    ) -> Result<bool, LevelError> {
        let mut next = level.get_block_state(start)?;
        if next.is_air()
            || !is_pushable(next, level, start, self.push_direction, false, direction)
            || start == self.piston_pos
            || self.to_push.contains(&start)
        {
            return Ok(true);
        }
        let mut count = 1;
        if count + self.to_push.len() > Self::MAX_PUSH_DEPTH {
            return Ok(false);
        }
        while is_sticky(next) {
            let pos = start.relative(self.push_direction.opposite(), count as i32);
            let previous = next;
            next = level.get_block_state(pos)?;
            if next.is_air()
                || !can_stick_to_each_other(previous, next)
                || !is_pushable(
                    next,
                    level,
                    pos,
                    self.push_direction,
                    false,
                    self.push_direction.opposite(),
                )
                || pos == self.piston_pos
            {
                break;
            }
            count += 1;
            if count + self.to_push.len() > Self::MAX_PUSH_DEPTH {
                return Ok(false);
            }
        }
        let mut added = count;
        for i in (0..count).rev() {
            self.to_push
                .push(start.relative(self.push_direction.opposite(), i as i32));
        }
        for i in 1.. {
            let pos = start.relative(self.push_direction, i);
            if let Some(collision) = self.to_push.iter().position(|p| *p == pos) {
                self.reorder_list_at_collision(added, collision);
                for j in 0..=collision + added {
                    let branch = self.to_push[j];
                    if is_sticky(level.get_block_state(branch)?)
                        && !self.add_branching_blocks(level, branch)?
                    {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            next = level.get_block_state(pos)?;
            if next.is_air() {
                return Ok(true);
            }
            if !is_pushable(
                next,
                level,
                pos,
                self.push_direction,
                true,
                self.push_direction,
            ) || pos == self.piston_pos
            {
                return Ok(false);
            }
            if next.data().push_reaction == PushReaction::Popped {
                self.to_destroy.push(pos);
                return Ok(true);
            }
            if self.to_push.len() >= Self::MAX_PUSH_DEPTH {
                return Ok(false);
            }
            self.to_push.push(pos);
            added += 1;
        }
        unreachable!()
    }
    fn reorder_list_at_collision(&mut self, added: usize, collision: usize) {
        // The tail just added moves in front of the collision-to-tail segment.
        let tail = self.to_push.len() - added;
        self.to_push[collision..].rotate_left(tail - collision);
    }
    fn add_branching_blocks(&mut self, level: &Level, from: BlockPos) -> Result<bool, LevelError> {
        let state = level.get_block_state(from)?;
        for direction in Direction::VALUES {
            if direction.axis() != self.push_direction.axis() {
                let neighbor = from.relative(direction, 1);
                if can_stick_to_each_other(level.get_block_state(neighbor)?, state)
                    && !self.add_block_line(level, neighbor, direction)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
