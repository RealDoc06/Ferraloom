//! Port of CropBlock growth, including beetroot and torchflower RNG gates.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, random::RandomSource};
pub struct Crop;
pub static CROP: Crop = Crop;
pub fn max_age(block: Block) -> u8 {
    match block.class() {
        BlockClass::BeetrootBlock => 3,
        BlockClass::TorchflowerCropBlock => 2,
        _ => 7,
    }
}
fn state_for_age(state: BlockState, age: u8) -> BlockState {
    if state.block() == Block::TORCHFLOWER_CROP && age == 2 {
        Block::TORCHFLOWER.default_state()
    } else {
        state
            .block()
            .default_state()
            .with(p::AGE, age)
            .expect("crop age")
    }
}
/// Port of CropBlock#getGrowthSpeed, retaining float additions and read order.
pub fn growth_speed(
    block: Block,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<f32, LevelError> {
    let mut speed = 1.0_f32;
    for x in -1..=1 {
        for z in -1..=1 {
            let below = level.get_block_state(pos.below(1).offset(x, 0, z))?;
            let mut contribution = 0.0_f32;
            if level
                .block_tags()?
                .contains_key(BlockTags::GROWS_CROPS, below.block())
            {
                contribution = if below.get(p::MOISTURE).unwrap_or(0) > 0 {
                    3.0
                } else {
                    1.0
                };
            }
            if x != 0 || z != 0 {
                contribution /= 4.0;
            }
            speed += contribution;
        }
    }
    let horizontal = level.get_block_state(pos.west(1))?.block() == block
        || level.get_block_state(pos.east(1))?.block() == block;
    let vertical = level.get_block_state(pos.north(1))?.block() == block
        || level.get_block_state(pos.south(1))?.block() == block;
    let crowded = if horizontal && vertical {
        true
    } else {
        level.get_block_state(pos.west(1).north(1))?.block() == block
            || level.get_block_state(pos.east(1).north(1))?.block() == block
            || level.get_block_state(pos.east(1).south(1))?.block() == block
            || level.get_block_state(pos.west(1).south(1))?.block() == block
    };
    Ok(if crowded { speed / 2.0 } else { speed })
}
/// Port of CropBlock#growCrops. The age increment consumes Level#getRandom.
pub fn grow_crops(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let increase = if state.block() == Block::TORCHFLOWER_CROP {
        1
    } else {
        let n = (level.random()?.next_int_bound(4) + 2) as u8;
        if state.block() == Block::BEETROOTS {
            n / 3
        } else {
            n
        }
    };
    let age = (state.get(p::AGE).unwrap() + increase).min(max_age(state.block()));
    level.set_block(pos, state_for_age(state, age), UpdateFlags::CLIENTS)?;
    Ok(())
}
impl BlockBehavior for Crop {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if matches!(state.block(), Block::BEETROOTS | Block::TORCHFLOWER_CROP)
            && level.random().next_int_bound(3) == 0
        {
            return Ok(());
        }
        let age = state.get(p::AGE).unwrap();
        if LevelReaderRef::Server(level).get_raw_brightness(pos, 0)? >= 9
            && age < max_age(state.block())
        {
            let speed = growth_speed(state.block(), pos, &LevelReaderRef::Server(level))?;
            if level.random().next_int_bound((25.0_f32 / speed) as i32 + 1) == 0 {
                LevelAccessRef::Server(level).set_block(
                    pos,
                    state_for_age(state, age + 1),
                    UpdateFlags::CLIENTS,
                )?;
            }
        }
        Ok(())
    }
}
pub struct NetherWart;
pub static NETHER_WART: NetherWart = NetherWart;
impl BlockBehavior for NetherWart {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let age = state.get(p::AGE).unwrap();
        if age < 3 && level.random().next_int_bound(10) == 0 {
            LevelAccessRef::Server(level).set_block(
                pos,
                state.with(p::AGE, age + 1).unwrap(),
                UpdateFlags::CLIENTS,
            )?;
        }
        Ok(())
    }
}

pub struct SweetBerry;
pub static SWEET_BERRY: SweetBerry = SweetBerry;
impl BlockBehavior for SweetBerry {
    /// Port of SweetBerryBushBlock#randomTick, including its captured new state.
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let age = state.get(p::AGE).unwrap();
        if age < 3
            && level.random().next_int_bound(5) == 0
            && LevelReaderRef::Server(level).get_raw_brightness(pos.above(1), 0)? >= 9
        {
            let next = state.with(p::AGE, age + 1).unwrap();
            LevelAccessRef::Server(level).set_block(pos, next, UpdateFlags::CLIENTS)?;
            level.game_event(
                ferraloom_data::registries::game_event::Id::BLOCK_CHANGE,
                ferraloom_util::Vec3::at_center_of(pos.into()),
                None,
                Some(next),
            )?;
        }
        Ok(())
    }
}
