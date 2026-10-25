//! Canonical sculk receiver storage. Plants owns listener and ticker mechanics;
//! data is shared by worldgen persistence and live listener/ticker mechanics.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::plants::sculk::{ChargeCursor, SculkSpreader},
    level::{Level, LevelError},
};
use ferraloom_data::properties as p;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{BlockPos, Direction};

pub mod vibration;
pub use vibration::{VibrationData, VibrationEvent, VibrationInfo, VibrationSelector};

/// Port of SculkSensorBlockEntity's canonical state.
#[derive(Clone, Debug)]
pub struct SculkSensorBlockEntity {
    pub parent: BlockEntityData,
    pub vibration_data: VibrationData,
    pub last_vibration_frequency: i32,
}
impl SculkSensorBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            vibration_data: VibrationData::default(),
            last_vibration_frequency: 0,
        }
    }
    fn load(&mut self, tag: &CompoundTag) {
        self.last_vibration_frequency = tag.get_int_or("last_vibration_frequency", 0);
        self.vibration_data = tag
            .get("listener")
            .and_then(VibrationData::decode)
            .unwrap_or_default();
    }
    fn save(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert(
            "last_vibration_frequency",
            Tag::Int(self.last_vibration_frequency),
        );
        tag.insert("listener", self.vibration_data.save());
        tag
    }
}
/// Port of CalibratedSculkSensorBlockEntity; persistence is inherited unchanged.
#[derive(Clone, Debug)]
pub struct CalibratedSculkSensorBlockEntity {
    pub parent: SculkSensorBlockEntity,
}
impl CalibratedSculkSensorBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: SculkSensorBlockEntity::new(parent),
        }
    }
}
/// Port of SculkShriekerBlockEntity's warning and vibration data.
#[derive(Clone, Debug)]
pub struct SculkShriekerBlockEntity {
    pub parent: BlockEntityData,
    pub warning_level: i32,
    pub vibration_data: VibrationData,
}
impl SculkShriekerBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            warning_level: 0,
            vibration_data: VibrationData::default(),
        }
    }
}
/// Port of SculkCatalystBlockEntity's single CatalystListener spreader state.
#[derive(Clone, Debug)]
pub struct SculkCatalystBlockEntity {
    pub parent: BlockEntityData,
    pub sculk_spreader: SculkSpreader,
}
impl SculkCatalystBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            sculk_spreader: SculkSpreader::create_level_spreader(),
        }
    }
}

fn ranged(tag: &CompoundTag, key: &str, max: i32, default: i32) -> i32 {
    tag.get_int(key)
        .filter(|value| (0..=max).contains(value))
        .unwrap_or(default)
}
/// Port of ChargeCursor.CODEC's data only. The spread algorithm remains the
/// plants-owned SculkSpreader; invalid optional fields retain codec partials.
fn cursor(value: &Tag) -> Option<ChargeCursor> {
    let tag = value.as_compound()?;
    let position = Vec::<i32>::from_nbt(tag.get("pos")?).ok()?;
    let [x, y, z] = position.as_slice() else {
        return None;
    };
    let facings = tag.get_list("facings").and_then(|list| {
        list.iter().try_fold(0u8, |mask, value| {
            let name = value.as_string()?.to_string_checked().ok()?;
            let index = Direction::VALUES
                .iter()
                .position(|direction| direction.name() == name)?;
            Some(mask | (1 << index))
        })
    });
    Some(ChargeCursor {
        pos: BlockPos::new(*x, *y, *z),
        charge: ranged(tag, "charge", 1000, 0),
        decay_delay: ranged(tag, "decay_delay", 1, 1),
        update_delay: ranged(tag, "update_delay", i32::MAX, 0),
        facings,
    })
}
fn save_cursor(cursor: &ChargeCursor) -> Tag {
    let mut tag = CompoundTag::new();
    tag.insert(
        "pos",
        Tag::IntArray(vec![cursor.pos.x, cursor.pos.y, cursor.pos.z]),
    );
    for (key, value, default) in [
        ("charge", cursor.charge, 0),
        ("decay_delay", cursor.decay_delay, 1),
        ("update_delay", cursor.update_delay, 0),
    ] {
        if value != default {
            tag.insert(key, Tag::Int(value));
        }
    }
    if let Some(mask) = cursor.facings {
        tag.insert(
            "facings",
            Tag::List(ListTag(
                Direction::VALUES
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| mask & (1 << index) != 0)
                    .map(|(_, direction)| Tag::String(direction.name().into()))
                    .collect(),
            )),
        );
    }
    Tag::Compound(tag)
}

pub(super) struct SculkBehavior {
    catalyst: bool,
}
pub(super) static SCULK_ENTITY: SculkBehavior = SculkBehavior { catalyst: false };
pub(super) static CATALYST_ENTITY: SculkBehavior = SculkBehavior { catalyst: true };
impl BlockEntity for SculkBehavior {
    fn game_event_listener(
        &self,
    ) -> Option<&'static dyn crate::level::game_event::GameEventListener> {
        Some(if self.catalyst {
            &crate::block::plants::sculk::catalyst::LISTENER
        } else {
            &crate::block::plants::sculk::vibration::LISTENER
        })
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        if self.catalyst {
            crate::block::plants::sculk::tick_catalyst(receiver, level)
        } else {
            crate::block::plants::sculk::vibration::tick(receiver, level)
        }
    }
    fn pre_remove_side_effects(
        &self,
        receiver: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let entity = level.block_entity_state(receiver)?;
        if matches!(entity, BlockEntityState::SculkShrieker(_))
            && entity.data().block_state.get(p::SHRIEKING) == Some(true)
        {
            return Err(LevelError::UnsupportedBlockEntity(
                entity.data().block_state.block(),
            ));
        }
        Ok(())
    }
    fn load_additional_live(
        &self,
        receiver: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let sources = cached_entity_refs(level.block_entity_state(receiver)?);
        self.load_additional(level.block_entity_state_mut(receiver)?, tag);
        for source in sources.into_iter().flatten() {
            level.release_entity(source);
        }
        Ok(())
    }
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        match state {
            BlockEntityState::SculkSensor(entity) => entity.load(tag),
            BlockEntityState::CalibratedSculkSensor(entity) => entity.parent.load(tag),
            BlockEntityState::SculkShrieker(entity) => {
                entity.warning_level = tag.get_int_or("warning_level", 0);
                entity.vibration_data = tag
                    .get("listener")
                    .and_then(VibrationData::decode)
                    .unwrap_or_default();
            }
            BlockEntityState::SculkCatalyst(entity) => {
                let cursors = tag
                    .get_list("cursors")
                    .into_iter()
                    .flat_map(|list| list.iter())
                    .filter_map(cursor)
                    .take(32)
                    .collect();
                entity.sculk_spreader.replace_cursors(cursors);
            }
            _ => unreachable!(),
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        Ok(match state {
            BlockEntityState::SculkSensor(entity) => entity.save(),
            BlockEntityState::CalibratedSculkSensor(entity) => entity.parent.save(),
            BlockEntityState::SculkShrieker(entity) => {
                let mut tag = CompoundTag::new();
                tag.insert("warning_level", Tag::Int(entity.warning_level));
                tag.insert("listener", entity.vibration_data.save());
                tag
            }
            BlockEntityState::SculkCatalyst(entity) => {
                let mut tag = CompoundTag::new();
                tag.insert(
                    "cursors",
                    Tag::List(ListTag(
                        entity
                            .sculk_spreader
                            .cursors()
                            .iter()
                            .map(save_cursor)
                            .collect(),
                    )),
                );
                tag
            }
            _ => unreachable!(),
        })
    }
}

/// Strong cached VibrationInfo receivers; UUID-only decoded records own no entity.
/// Retirement calls this before dropping a BE, without invoking world callbacks.
pub(crate) fn cached_entity_refs(
    state: &BlockEntityState,
) -> [Option<crate::entity::EntityRef>; 2] {
    let data = match state {
        BlockEntityState::SculkSensor(value) => &value.vibration_data,
        BlockEntityState::CalibratedSculkSensor(value) => &value.parent.vibration_data,
        BlockEntityState::SculkShrieker(value) => &value.vibration_data,
        _ => return [None, None],
    };
    [
        data.current_vibration.as_ref().and_then(|info| info.entity),
        data.selection_strategy
            .current_vibration_data
            .as_ref()
            .and_then(|candidate| candidate.event.entity),
    ]
}

#[cfg(test)]
mod tests;
