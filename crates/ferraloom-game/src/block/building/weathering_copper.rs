//! ChangeOverTimeBlock and WeatheringCopper's shared native aging algorithm.
use crate::{
    block::{BlockBehavior, BlockMethod, entity::BlockEntityState},
    level::{
        Level, LevelError,
        blockscan::{BlockMatcher, Continuation},
    },
};
use ferraloom_data::{BlockClass, BlockState, properties as p, weathering_copper::WeatherState};
use ferraloom_util::{BlockPos, random::RandomSource};

pub(super) struct WeatheringCopper;
pub(super) static WEATHERING_COPPER: WeatheringCopper = WeatheringCopper;

/// Port of WeatheringCopper#getChanceModifier.
fn chance_modifier(age: WeatherState) -> f32 {
    if age == WeatherState::Unaffected {
        0.75
    } else {
        1.0
    }
}
/// Port of ChangeOverTimeBlock#getNextState. The canonical matcher preserves
/// Manhattan order and its chunk cache. As native does, the consumer rereads the
/// accepted receiver; a younger neighbor aborts before the final random draw.
pub(super) fn next_state(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<Option<BlockState>, LevelError> {
    let age = state
        .block()
        .weathering_copper_properties()
        .expect("weathering receiver")
        .age;
    let mut same = 0;
    let mut older = 0;
    let aborted = level
        .find_blocks_in_manhattan(pos, 4)
        .filter_pos(|at| *at != pos)
        .filter_state(|state| state.block().weathering_copper_properties().is_some())
        .for_each_until(|at, _| {
            let neighbor = level.get_block_state(at)?;
            if let Some(neighbor) = neighbor.block().weathering_copper_properties() {
                if neighbor.age < age {
                    return Ok(Continuation::Abort);
                }
                if neighbor.age > age {
                    older += 1;
                } else {
                    same += 1;
                }
            }
            Ok(Continuation::Continue)
        })?;
    if aborted {
        return Ok(None);
    }
    let chance = (older + 1) as f32 / (older + same + 1) as f32;
    let actual_chance = chance * chance * chance_modifier(age);
    if level.random().next_float() < actual_chance {
        Ok(state.next_weathering())
    } else {
        Ok(None)
    }
}
/// Port of ChangeOverTimeBlock#changeOverTime. Even a final-stage receiver
/// consumes the native draws when called directly; tick eligibility is separate.
pub(super) fn change_over_time(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.random().next_float() < 0.05688889_f32
        && let Some(next) = next_state(state, pos, level)?
    {
        level.set_block_and_update(pos, next)?;
    }
    Ok(())
}
fn required_query(state: BlockState, pos: BlockPos, phase: &'static str) -> LevelError {
    LevelError::At {
        pos,
        phase,
        source: Box::new(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::RandomTick,
        )),
    }
}
impl BlockBehavior for WeatheringCopper {
    /// Native classes delegate to changeOverTime. Doors require LOWER first;
    /// chests skip RIGHT before resolving a receiver or drawing any randomness.
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        match state.block().class() {
            BlockClass::WeatheringCopperDoorBlock if state.get(p::HALF) != Some(p::Half::Lower) => {
                return Ok(());
            }
            BlockClass::WeatheringCopperChestBlock => {
                if state.get(p::TYPE_) == Some(p::Type::Right) {
                    return Ok(());
                }
                let Some(handle) = level.get_or_promote_block_entity(pos)? else {
                    // Native Level#getBlockEntity uses IMMEDIATE creation. A
                    // genuinely BE-free cell can skip; a missing receiver for
                    // an EntityBlock cannot be fabricated as an absent chest.
                    return if level.get_block_state(pos)?.has_block_entity() {
                        Err(required_query(
                            state,
                            pos,
                            "WeatheringCopperChestBlock#randomTick requires canonical IMMEDIATE block entity resolution",
                        ))
                    } else {
                        Ok(())
                    };
                };
                if matches!(
                    level.block_entity_state(handle)?,
                    BlockEntityState::Chest(_)
                ) {
                    return Err(required_query(
                        state,
                        pos,
                        "WeatheringCopperChestBlock#getEntitiesWithContainerOpen requires the canonical opener query",
                    ));
                }
                return Ok(());
            }
            _ => {}
        }
        change_over_time(state, pos, level)
    }
}
