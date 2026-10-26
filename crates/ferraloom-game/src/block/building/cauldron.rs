//! CauldronBlock and LayeredCauldronBlock precipitation filling.
use crate::{
    block::BlockBehavior,
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, properties as p, registries::game_event::Id as GameEvent};
use ferraloom_util::{BlockPos, Vec3, random::RandomSource};
use ferraloom_worldgen::biome_manager::Precipitation;

pub(super) struct Cauldron;
pub(super) static CAULDRON: Cauldron = Cauldron;

/// Port of CauldronBlock#shouldHandlePrecipitation. NONE draws nothing; full
/// and mismatched layered cauldrons still draw before their later state gates.
pub(super) fn should_handle_precipitation(level: &mut Level, precipitation: Precipitation) -> bool {
    match precipitation {
        Precipitation::Rain => level.random().next_float() < 0.05_f32,
        Precipitation::Snow => level.random().next_float() < 0.1_f32,
        Precipitation::None => false,
    }
}
impl BlockBehavior for Cauldron {
    /// Ports of CauldronBlock/LayeredCauldronBlock#handlePrecipitation. The empty
    /// receiver emits Context.of(null entity); layered receivers capture the new
    /// state in the context, after setBlockAndUpdate has run its callbacks.
    fn handle_precipitation(
        &self,
        state: BlockState,
        pos: BlockPos,
        precipitation: Precipitation,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !should_handle_precipitation(level, precipitation) {
            return Ok(());
        }
        let empty = state.block() == Block::CAULDRON;
        let next = if empty {
            match precipitation {
                Precipitation::Rain => Block::WATER_CAULDRON.default_state(),
                Precipitation::Snow => Block::POWDER_SNOW_CAULDRON.default_state(),
                Precipitation::None => return Ok(()),
            }
        } else {
            // Blocks#register passes RAIN to water and SNOW to powder snow.
            let expected = if state.block() == Block::WATER_CAULDRON {
                Precipitation::Rain
            } else {
                Precipitation::Snow
            };
            let fill = state.get(p::LEVEL).unwrap();
            if fill == 3 || precipitation != expected {
                return Ok(());
            }
            state.with(p::LEVEL, fill + 1).unwrap()
        };
        level.set_block_and_update(pos, next)?;
        level.game_event(
            GameEvent::BLOCK_CHANGE,
            Vec3::at_center_of(pos.into()),
            None,
            if empty { None } else { Some(next) },
        )
    }
}
