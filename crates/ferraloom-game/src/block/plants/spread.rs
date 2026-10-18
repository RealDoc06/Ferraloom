//! Ports of MushroomBlock and StemBlock random growth.
use super::{
    super::{BlockBehavior, LevelAccessRef, UpdateFlags},
    crops,
};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, core::Plane, random::RandomSource};
pub struct Mushroom;
pub static MUSHROOM: Mushroom = Mushroom;
fn offset(pos: BlockPos, level: &mut Level) -> BlockPos {
    pos.offset(
        level.random().next_int_bound(3) - 1,
        level.random().next_int_bound(2) - level.random().next_int_bound(2),
        level.random().next_int_bound(3) - 1,
    )
}
impl BlockBehavior for Mushroom {
    fn random_tick(
        &self,
        state: BlockState,
        mut pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if level.random().next_int_bound(25) != 0 {
            return Ok(());
        }
        let mut count = 0;
        for z in -4..=4 {
            for y in -1..=1 {
                for x in -4..=4 {
                    if level.get_block_state(pos.offset(x, y, z))?.block() == state.block() {
                        count += 1;
                        if count > 4 {
                            return Ok(());
                        }
                    }
                }
            }
        }
        let mut candidate = offset(pos, level);
        for _ in 0..4 {
            if level.get_block_state(candidate)?.is_air() && level.can_survive(state, candidate)? {
                pos = candidate;
            }
            candidate = offset(pos, level);
        }
        if level.get_block_state(candidate)?.is_air() && level.can_survive(state, candidate)? {
            LevelAccessRef::Server(level).set_block(candidate, state, UpdateFlags::CLIENTS)?;
        }
        Ok(())
    }
}
pub struct Stem;
pub static STEM: Stem = Stem;
/// Port of StemBlock#randomTick, also called synchronously by bonemeal at age 7.
pub(super) fn grow_stem(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if level.as_reader().get_raw_brightness(pos, 0)? < 9 {
        return Ok(());
    }
    let speed = crops::growth_speed(state.block(), pos, &level.as_reader())?;
    if level
        .random()?
        .next_int_bound((25.0_f32 / speed) as i32 + 1)
        != 0
    {
        return Ok(());
    }
    let age = state.get(p::AGE).unwrap();
    if age < 7 {
        level.set_block(
            pos,
            state.with(p::AGE, age + 1).unwrap(),
            UpdateFlags::CLIENTS,
        )?;
        return Ok(());
    }
    let direction = Plane::Horizontal.random_direction(level.random()?);
    let at = pos.relative(direction, 1);
    let below = level.get_block_state(at.below(1))?;
    let (fruit, attached, tag) = if state.block() == Block::MELON_STEM {
        (
            Block::MELON,
            Block::ATTACHED_MELON_STEM,
            BlockTags::SUPPORTS_MELON_STEM_FRUIT,
        )
    } else {
        (
            Block::PUMPKIN,
            Block::ATTACHED_PUMPKIN_STEM,
            BlockTags::SUPPORTS_PUMPKIN_STEM_FRUIT,
        )
    };
    if level.get_block_state(at)?.is_air()
        && level
            .as_reader()
            .block_tags()?
            .contains_key(tag, below.block())
    {
        let facing = match direction {
            Direction::North => p::Facing::North,
            Direction::East => p::Facing::East,
            Direction::South => p::Facing::South,
            Direction::West => p::Facing::West,
            _ => unreachable!("horizontal stem"),
        };
        level.set_block(at, fruit.default_state(), UpdateFlags::ALL)?;
        level.set_block(
            pos,
            attached.default_state().with(p::FACING, facing).unwrap(),
            UpdateFlags::ALL,
        )?;
    }
    Ok(())
}
impl BlockBehavior for Stem {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        grow_stem(state, pos, &mut LevelAccessRef::Server(level))
    }
}
