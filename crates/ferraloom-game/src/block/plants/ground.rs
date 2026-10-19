//! Ports of SnowyBlock updates and FarmlandBlock/PathBlock soil conversion.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef};
use crate::{
    block::UpdateFlags,
    entity::{EntityData, EntityRef, EntityState},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    registries::game_event::Id as GameEvent,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockPos, Direction, Vec3, random::RandomSource};
pub struct Ground;
pub static GROUND: Ground = Ground;

/// Port of FarmlandBlock/PathBlock#turnToBaseBlock, including entity lifting
/// before the new collision shape is installed.
pub(super) fn turn_to_base_block(
    source: Option<EntityRef>,
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    let new_state = level.push_entities_up(state, Block::DIRT.default_state(), pos)?;
    LevelAccessRef::Server(level).set_block(pos, new_state, UpdateFlags::ALL)?;
    level.game_event(
        GameEvent::BLOCK_CHANGE,
        Vec3::at_center_of(pos.into()),
        source,
        Some(new_state),
    )?;
    Ok(())
}

fn maintains_farmland(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    Ok(level.block_tags()?.contains_key(
        BlockTags::MAINTAINS_FARMLAND,
        level.get_block_state(pos.above(1))?.block(),
    ))
}

/// Port of FarmlandBlock#isNearWater. BlockPos traversal advances X first.
fn near_water(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let water = level.fluid_tags()?;
    for z in -4..=4 {
        for y in 0..=1 {
            for x in -4..=4 {
                if water.contains_key(
                    FluidTags::WATER,
                    level.get_fluid_state(pos.offset(x, y, z))?.fluid_type,
                ) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
impl BlockBehavior for Ground {
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if state.block().class() == BlockClass::PathBlock
            || !self.can_survive(state, pos, &LevelReaderRef::Server(level))?
        {
            turn_to_base_block(None, state, pos, level)?;
        }
        Ok(())
    }

    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let moisture = state.get(p::MOISTURE).unwrap();
        if !near_water(&LevelReaderRef::Server(level), pos)?
            && !level.is_raining_at(pos.above(1))?
        {
            if moisture > 0 {
                LevelAccessRef::Server(level).set_block(
                    pos,
                    state.with(p::MOISTURE, moisture - 1).unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            } else if !maintains_farmland(&LevelReaderRef::Server(level), pos)? {
                turn_to_base_block(None, state, pos, level)?;
            }
        } else if moisture < 7 {
            LevelAccessRef::Server(level).set_block(
                pos,
                state.with(p::MOISTURE, 7).unwrap(),
                UpdateFlags::CLIENTS,
            )?;
        }
        Ok(())
    }
    /// Port of FarmlandBlock#fallOn, including the draw before instanceof and
    /// the player's spawn-protection check without a world-border restriction.
    fn fall_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        fall_distance: f64,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if f64::from(level.random().next_float()) < fall_distance - 0.5
            && level.entity_state(entity)?.as_living().is_some()
        {
            let can_trample = if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
                !level.is_under_spawn_protection(entity, pos)?
            } else {
                level.rules().mob_griefing
            };
            if can_trample && {
                let dimensions = &level.entity::<EntityData>(entity)?.dimensions;
                dimensions.width * dimensions.width * dimensions.height > 0.512_f32
            } {
                turn_to_base_block(Some(entity), state, pos, level)?;
            }
        }
        super::super::BASE_BLOCK.fall_on(state, pos, entity, fall_distance, level)
    }

    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let above = level.get_block_state(pos.above(1))?;
        Ok(!above.is_solid()
            || if state.block().class() == BlockClass::FarmlandBlock {
                maintains_farmland(level, pos)?
            } else {
                above.block().is_instance_of(BlockClass::FenceGateBlock)
            })
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
        if state.get(p::SNOWY).is_some() {
            return Ok(if direction == Direction::Up {
                state
                    .with(
                        p::SNOWY,
                        level
                            .as_reader()
                            .block_tags()?
                            .contains_key(BlockTags::SNOW, neighbor.block()),
                    )
                    .unwrap()
            } else {
                state
            });
        }
        if direction == Direction::Up && !self.can_survive(state, pos, &level.as_reader())? {
            level.schedule_block_tick(pos, state.block(), 1)?;
        }
        Ok(state)
    }
}
