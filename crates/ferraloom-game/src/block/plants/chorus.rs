//! Ports of ChorusPlantBlock connections/support and ChorusFlowerBlock growth.
use super::{
    super::{BlockBehavior, BlockPlaceContext, LevelAccessRef, LevelReaderRef, UpdateFlags},
    multiface,
};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, VoxelShape, core::Plane, random::RandomSource, shapes};

pub struct Chorus;
pub static CHORUS: Chorus = Chorus;

/// Port of ChorusPlantBlock#getStateWithConnections, including read order.
pub(super) fn with_connections(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let directions = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    let mut neighbors = [Block::AIR.default_state(); 6];
    for (neighbor, direction) in neighbors.iter_mut().zip(directions) {
        *neighbor = level.get_block_state(pos.relative(direction, 1))?;
    }
    let mut result = state;
    for (neighbor, direction) in neighbors.into_iter().zip(directions) {
        let connects = neighbor.block() == state.block()
            || neighbor.block() == Block::CHORUS_FLOWER
            || direction == Direction::Down
                && level
                    .block_tags()?
                    .contains_key(BlockTags::SUPPORTS_CHORUS_PLANT, neighbor.block());
        result =
            multiface::with_face(result, direction, connects).expect("chorus connection property");
    }
    Ok(result)
}

fn all_neighbors_empty(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    ignore: Option<Direction>,
) -> Result<bool, LevelError> {
    for &direction in Plane::Horizontal.faces() {
        if Some(direction) != ignore && !level.get_block_state(pos.relative(direction, 1))?.is_air()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn flower(level: &mut Level, pos: BlockPos, age: u8, event: i32) -> Result<(), LevelError> {
    LevelAccessRef::Server(level).set_block(
        pos,
        Block::CHORUS_FLOWER
            .default_state()
            .with(p::AGE, age)
            .unwrap(),
        UpdateFlags::CLIENTS,
    )?;
    level.emit(LevelEffect::LevelEvent {
        pos,
        id: event,
        data: 0,
    });
    Ok(())
}
fn convert_to_plant(level: &mut Level, pos: BlockPos) -> Result<(), LevelError> {
    let state = with_connections(
        Block::CHORUS_PLANT.default_state(),
        pos,
        &LevelReaderRef::Server(level),
    )?;
    LevelAccessRef::Server(level).set_block(pos, state, UpdateFlags::CLIENTS)?;
    Ok(())
}
impl BlockBehavior for Chorus {
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        Ok(Some(with_connections(
            block.default_state(),
            context.clicked_pos(),
            &level.as_reader(),
        )?))
    }
    fn support_shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(shapes::create(
            1.0 / 16.0,
            0.0,
            1.0 / 16.0,
            15.0 / 16.0,
            15.0 / 16.0,
            15.0 / 16.0,
        ))
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        if state.block() == Block::CHORUS_FLOWER {
            if below.block() == Block::CHORUS_PLANT
                || level
                    .block_tags()?
                    .contains_key(BlockTags::SUPPORTS_CHORUS_FLOWER, below.block())
            {
                return Ok(true);
            }
            if !below.is_air() {
                return Ok(false);
            }
            let mut one_neighbor = false;
            for &direction in Plane::Horizontal.faces() {
                let neighbor = level.get_block_state(pos.relative(direction, 1))?;
                if neighbor.block() == Block::CHORUS_PLANT {
                    if one_neighbor {
                        return Ok(false);
                    }
                    one_neighbor = true;
                } else if !neighbor.is_air() {
                    return Ok(false);
                }
            }
            return Ok(one_neighbor);
        }
        let block_above_or_below =
            !level.get_block_state(pos.above(1))?.is_air() && !below.is_air();
        for &direction in Plane::Horizontal.faces() {
            let at = pos.relative(direction, 1);
            if level.get_block_state(at)?.block() == state.block() {
                if block_above_or_below {
                    return Ok(false);
                }
                let lower = level.get_block_state(at.below(1))?;
                if lower.block() == state.block()
                    || level
                        .block_tags()?
                        .contains_key(BlockTags::SUPPORTS_CHORUS_PLANT, lower.block())
                {
                    return Ok(true);
                }
            }
        }
        Ok(below.block() == state.block()
            || level
                .block_tags()?
                .contains_key(BlockTags::SUPPORTS_CHORUS_PLANT, below.block()))
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
        if state.block() == Block::CHORUS_FLOWER {
            if direction != Direction::Up && !self.can_survive(state, pos, &level.as_reader())? {
                level.schedule_block_tick(pos, state.block(), 1)?;
            }
            return Ok(state);
        }
        if !self.can_survive(state, pos, &level.as_reader())? {
            level.schedule_block_tick(pos, state.block(), 1)?;
            return Ok(state);
        }
        let connects = neighbor.block() == state.block()
            || neighbor.block() == Block::CHORUS_FLOWER
            || direction == Direction::Down
                && level
                    .as_reader()
                    .block_tags()?
                    .contains_key(BlockTags::SUPPORTS_CHORUS_PLANT, neighbor.block());
        Ok(multiface::with_face(state, direction, connects).expect("chorus connection property"))
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            LevelAccessRef::Server(level).destroy_block(pos, true)?;
        }
        Ok(())
    }
    /// Port of ChorusFlowerBlock#randomTick. Height RNG is skipped for a
    /// one-block pillar, and branch attempts draw directions independently.
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let above = pos.above(1);
        if !level.get_block_state(above)?.is_air() || above.y > level.max_build_height() {
            return Ok(());
        }
        let age = state.get(p::AGE).unwrap();
        if age >= 5 {
            return Ok(());
        }
        let below = level.get_block_state(pos.below(1))?;
        let mut upwards = false;
        let mut pillar_on_support = false;
        if level
            .block_tags()?
            .contains_key(BlockTags::SUPPORTS_CHORUS_FLOWER, below.block())
        {
            upwards = true;
        } else if below.block() == Block::CHORUS_PLANT {
            let mut height = 1;
            for _ in 0..4 {
                let test = level.get_block_state(pos.below(height + 1))?;
                if test.block() != Block::CHORUS_PLANT {
                    pillar_on_support = level
                        .block_tags()?
                        .contains_key(BlockTags::SUPPORTS_CHORUS_FLOWER, test.block());
                    break;
                }
                height += 1;
            }
            if height < 2
                || height
                    <= level
                        .random()
                        .next_int_bound(if pillar_on_support { 5 } else { 4 })
            {
                upwards = true;
            }
        } else if below.is_air() {
            upwards = true;
        }
        if upwards
            && all_neighbors_empty(&LevelReaderRef::Server(level), above, None)?
            && level.get_block_state(pos.above(2))?.is_air()
        {
            convert_to_plant(level, pos)?;
            flower(level, above, age, 1033)?;
        } else if age < 4 {
            let mut attempts = level.random().next_int_bound(4);
            if pillar_on_support {
                attempts += 1;
            }
            let mut created_branch = false;
            for _ in 0..attempts {
                let direction =
                    Plane::Horizontal.faces()[level.random().next_int_bound(4) as usize];
                let target = pos.relative(direction, 1);
                if level.get_block_state(target)?.is_air()
                    && level.get_block_state(target.below(1))?.is_air()
                    && all_neighbors_empty(
                        &LevelReaderRef::Server(level),
                        target,
                        Some(direction.opposite()),
                    )?
                {
                    flower(level, target, age + 1, 1033)?;
                    created_branch = true;
                }
            }
            if created_branch {
                convert_to_plant(level, pos)?;
            } else {
                flower(level, pos, 5, 1034)?;
            }
        } else {
            flower(level, pos, 5, 1034)?;
        }
        Ok(())
    }
}
