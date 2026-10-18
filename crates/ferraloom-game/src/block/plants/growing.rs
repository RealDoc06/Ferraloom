//! Ports of GrowingPlantBlock, GrowingPlantHeadBlock and GrowingPlantBodyBlock.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p, registries::fluid::Id, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};
pub struct GrowingPlant;
pub static GROWING_PLANT: GrowingPlant = GrowingPlant;
pub(super) fn family(block: Block) -> (Block, Block, Direction) {
    match block {
        Block::KELP | Block::KELP_PLANT => (Block::KELP, Block::KELP_PLANT, Direction::Up),
        Block::TWISTING_VINES | Block::TWISTING_VINES_PLANT => (
            Block::TWISTING_VINES,
            Block::TWISTING_VINES_PLANT,
            Direction::Up,
        ),
        Block::WEEPING_VINES | Block::WEEPING_VINES_PLANT => (
            Block::WEEPING_VINES,
            Block::WEEPING_VINES_PLANT,
            Direction::Down,
        ),
        Block::CAVE_VINES | Block::CAVE_VINES_PLANT => {
            (Block::CAVE_VINES, Block::CAVE_VINES_PLANT, Direction::Down)
        }
        _ => unreachable!("registered growing plant"),
    }
}

pub(super) fn can_grow_into(head: Block, state: BlockState) -> bool {
    if head == Block::KELP {
        state.block() == Block::WATER
    } else {
        state.is_air()
    }
}

/// Port of GrowingPlantBodyBlock#getHeadPos and BlockUtil#getTopConnectedBlock.
pub(super) fn head_pos(
    state: BlockState,
    mut pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockPos>, LevelError> {
    let (head, body, direction) = family(state.block());
    if state.block() == head {
        return Ok(Some(pos));
    }
    loop {
        pos = pos.relative(direction, 1);
        let forward = level.get_block_state(pos)?;
        if forward.block() != body {
            return Ok((forward.block() == head).then_some(pos));
        }
    }
}

/// Port of GrowingPlantHeadBlock#performBonemeal and NetherVines' decreasing
/// probability. The length draw is consumed before examining the first target.
pub(super) fn bonemeal(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let (head, _, direction) = family(state.block());
    let mut at = pos.relative(direction, 1);
    let mut age = (state.get(p::AGE).unwrap() + 1).min(25);
    let count = if head == Block::KELP {
        1
    } else {
        let mut probability = 1.0;
        let mut count = 0;
        while level.random()?.next_double() < probability {
            count += 1;
            probability *= 0.826;
        }
        count
    };
    for _ in 0..count {
        if !can_grow_into(head, level.get_block_state(at)?)
            || !super::bonemeal::inside_height(&level.as_reader(), at)
        {
            break;
        }
        level.set_block(at, state.with(p::AGE, age).unwrap(), UpdateFlags::ALL)?;
        at = at.relative(direction, 1);
        age = (age + 1).min(25);
    }
    Ok(())
}
impl BlockBehavior for GrowingPlant {
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let (head, body, direction) = family(state.block());
        let at = pos.relative(direction.opposite(), 1);
        let support = level.get_block_state(at)?;
        if head == Block::KELP
            && level
                .block_tags()?
                .contains_key(BlockTags::CANNOT_SUPPORT_KELP, support.block())
        {
            return Ok(false);
        }
        Ok(support.block() == head
            || support.block() == body
            || level.is_face_sturdy(support, at, direction)?)
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            level.destroy_block(pos, true, UPDATE_LIMIT)?;
        }
        Ok(())
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let (head, body, growth) = family(state.block());
        let is_head = state.block() == head;
        if direction == growth.opposite() {
            if !self.can_survive(state, pos, &level.as_reader())? {
                level.schedule_block_tick(pos, state.block(), 1)?;
            } else if is_head {
                let forward = level.get_block_state(pos.relative(growth, 1))?;
                if forward.block() == head || forward.block() == body {
                    return Ok(body.default_state().with_properties_of(state));
                }
            }
        }
        if direction == growth {
            let continues = neighbor.block() == head || neighbor.block() == body;
            if is_head && continues {
                return Ok(body.default_state().with_properties_of(state));
            }
            if !is_head && !continues {
                let age = level.random()?.next_int_bound(25) as u8;
                return Ok(head
                    .default_state()
                    .with(p::AGE, age)
                    .unwrap()
                    .with_properties_of(state));
            }
        }
        if head == Block::KELP {
            level.schedule_fluid_tick(pos, Id::WATER, 5)?;
        }
        Ok(state)
    }
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let (head, _body, direction) = family(state.block());
        let age = state.get(p::AGE).expect("growing head age");
        let probability = if head == Block::KELP { 0.14 } else { 0.1 };
        if age < 25 && level.random().next_double() < probability {
            let at = pos.relative(direction, 1);
            let old = level.get_block_state(at)?;
            if can_grow_into(head, old) {
                let mut next = state.with(p::AGE, age + 1).unwrap();
                if head == Block::CAVE_VINES {
                    next = next
                        .with(p::BERRIES, level.random().next_float() < 0.11)
                        .unwrap();
                }
                level.set_block(at, next, UpdateFlags::ALL, UPDATE_LIMIT)?;
            }
        }
        Ok(())
    }
}
