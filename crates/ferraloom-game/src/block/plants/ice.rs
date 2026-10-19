//! Ports of IceBlock melting and FrostedIceBlock's scheduled decay chain.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, Orientation, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p, registries::game_event::Id as GameEvent};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, Direction, Vec3, random::RandomSource};

pub struct Ice;
pub static ICE: Ice = Ice;

/// Port of IceBlock#melt. The required environment query happens before any
/// write, and evaporation does not emit the non-evaporating branch's event.
fn melt(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    if level.water_evaporates(pos)? {
        level.remove_block(pos, false)?;
    } else {
        LevelAccessRef::Server(level).set_block(
            pos,
            Block::WATER.default_state(),
            UpdateFlags::ALL,
        )?;
        level.neighbor_changed(pos, Block::WATER, None)?;
        level.game_event(
            GameEvent::BLOCK_DESTROY,
            Vec3::at_center_of(pos.into()),
            None,
            Some(state),
        )?;
    }
    Ok(())
}
impl BlockBehavior for Ice {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if i32::from(level.brightness(ferraloom_light::LightLayer::Block, pos)?)
            > 11 - i32::from(state.data().opacity)
        {
            melt(state, pos, level)?;
        }
        Ok(())
    }
}

fn fewer_neighbors_than(
    pos: BlockPos,
    limit: usize,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let mut count = 0;
    for direction in Direction::VALUES {
        if level.get_block_state(pos.relative(direction, 1))?.block() == Block::FROSTED_ICE {
            count += 1;
            if count >= limit {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn slightly_melt(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<bool, LevelError> {
    let age = state.get(p::AGE).unwrap();
    if age < 3 {
        LevelAccessRef::Server(level).set_block(
            pos,
            state.with(p::AGE, age + 1).unwrap(),
            UpdateFlags::CLIENTS,
        )?;
        Ok(false)
    } else {
        melt(state, pos, level)?;
        Ok(true)
    }
}

pub struct FrostedIce;
pub static FROSTED_ICE: FrostedIce = FrostedIce;
impl BlockBehavior for FrostedIce {
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if level.random().next_int_bound(3) == 0
            || fewer_neighbors_than(pos, 4, &LevelReaderRef::Server(level))?
        {
            let dimension = level.dimension_key()?;
            let brightness =
                if dimension.namespace() == "minecraft" && dimension.path() == "the_end" {
                    i32::from(level.brightness(ferraloom_light::LightLayer::Block, pos)?)
                } else {
                    level.max_local_raw_brightness(pos)?
                };
            if brightness
                > 11 - i32::from(state.get(p::AGE).unwrap()) - i32::from(state.data().opacity)
                && slightly_melt(state, pos, level)?
            {
                for direction in Direction::VALUES {
                    let neighbor_pos = pos.relative(direction, 1);
                    let neighbor = level.get_block_state(neighbor_pos)?;
                    if neighbor.block() == Block::FROSTED_ICE
                        && !slightly_melt(neighbor, neighbor_pos, level)?
                    {
                        let delay = 20 + level.random().next_int_bound(21);
                        level.schedule_block_tick_after(neighbor_pos, Block::FROSTED_ICE, delay)?;
                    }
                }
                return Ok(());
            }
        }
        let delay = 20 + level.random().next_int_bound(21);
        level.schedule_block_tick_after(pos, Block::FROSTED_ICE, delay)
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if source == Block::FROSTED_ICE
            && fewer_neighbors_than(pos, 2, &LevelReaderRef::Server(level))?
        {
            melt(state, pos, level)?;
        }
        Ok(())
    }
    /// Port of FrostedIceBlock#onPlace. Draw from the live level's canonical
    /// stream even when the pending-tick container already contains this tick.
    fn on_place(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let delay = 60 + level.random().next_int_bound(61);
        level.schedule_block_tick_after(pos, Block::FROSTED_ICE, delay)
    }
    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ItemStack, LevelError> {
        Ok(ItemStack::default())
    }
}
