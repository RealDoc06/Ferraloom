//! Ports of GrowingPlantBlock, GrowingPlantHeadBlock and GrowingPlantBodyBlock.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p, registries::fluid::Id, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};
pub struct GrowingPlant;
pub static GROWING_PLANT: GrowingPlant = GrowingPlant;
fn family(block: Block) -> (Block, Block, Direction) {
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
            if if head == Block::KELP {
                old.block() == Block::WATER
            } else {
                old.is_air()
            } {
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
