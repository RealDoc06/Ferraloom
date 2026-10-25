//! Persistence of VibrationSystem.Data, VibrationSelector and VibrationInfo.
//! Selection, travel, particles and delivery are separate plants-owned behavior.
use crate::entity::EntityRef;
use ferraloom_data::registries::game_event::Id as GameEvent;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{Identifier, component::Persistent};
use ferraloom_util::{Uuid, Vec3};

/// Port of VibrationInfo. The cached entity is transient, independent of UUIDs.
#[derive(Clone, Debug)]
pub struct VibrationInfo {
    pub game_event: GameEvent,
    pub distance: f32,
    pub pos: Vec3,
    pub uuid: Option<Uuid>,
    pub projectile_owner_uuid: Option<Uuid>,
    pub entity: Option<EntityRef>,
}
impl VibrationInfo {
    fn decode(value: &Tag) -> Option<Self> {
        let tag = value.as_compound()?;
        let name =
            Identifier::parse(&tag.get_string("game_event")?.to_string_checked().ok()?).ok()?;
        let game_event = GameEvent::from_name(name.as_str())?;
        let distance = tag.get_float("distance")?;
        if !(0.0..=f32::MAX).contains(&distance) {
            return None;
        }
        let coordinates = Vec::<f64>::from_nbt(tag.get("pos")?).ok()?;
        let [x, y, z] = coordinates.as_slice() else {
            return None;
        };
        Some(Self {
            game_event,
            distance,
            pos: Vec3::new(*x, *y, *z),
            uuid: tag.get("source").and_then(|tag| Uuid::from_nbt(tag).ok()),
            projectile_owner_uuid: tag
                .get("projectile_owner")
                .and_then(|tag| Uuid::from_nbt(tag).ok()),
            entity: None,
        })
    }
    fn save(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.insert("game_event", Tag::String(self.game_event.name().into()));
        tag.insert("distance", Tag::Float(self.distance));
        tag.insert(
            "pos",
            Tag::List(ListTag(vec![
                Tag::Double(self.pos.x),
                Tag::Double(self.pos.y),
                Tag::Double(self.pos.z),
            ])),
        );
        if let Some(uuid) = self.uuid {
            tag.insert("source", Tag::IntArray(uuid.to_int_array().to_vec()));
        }
        if let Some(uuid) = self.projectile_owner_uuid {
            tag.insert(
                "projectile_owner",
                Tag::IntArray(uuid.to_int_array().to_vec()),
            );
        }
        Tag::Compound(tag)
    }
}
#[derive(Clone, Debug)]
pub struct VibrationEvent {
    pub event: VibrationInfo,
    pub tick: i64,
}
/// Port of VibrationSelector's stored optional candidate. Without an event,
/// the constructor discards even a present tick and saves -1.
#[derive(Clone, Debug, Default)]
pub struct VibrationSelector {
    pub current_vibration_data: Option<VibrationEvent>,
}
impl VibrationSelector {
    fn decode(value: &Tag) -> Option<Self> {
        let tag = value.as_compound()?;
        let tick = tag.get_long("tick")?;
        Some(Self {
            current_vibration_data: tag
                .get("event")
                .and_then(VibrationInfo::decode)
                .map(|event| VibrationEvent { event, tick }),
        })
    }
    fn save(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.insert(
            "tick",
            Tag::Long(
                self.current_vibration_data
                    .as_ref()
                    .map_or(-1, |value| value.tick),
            ),
        );
        if let Some(value) = &self.current_vibration_data {
            tag.insert("event", value.event.save());
        }
        Tag::Compound(tag)
    }
}
/// Port of VibrationSystem.Data. Fresh data differs from successful decoding in
/// its transient reload flag, even when the persisted listener has no event.
#[derive(Clone, Debug, Default)]
pub struct VibrationData {
    pub current_vibration: Option<VibrationInfo>,
    pub travel_time_in_ticks: i32,
    pub selection_strategy: VibrationSelector,
    pub reload_vibration_particle: bool,
}
impl VibrationData {
    pub(super) fn decode(value: &Tag) -> Option<Self> {
        let tag = value.as_compound()?;
        Some(Self {
            current_vibration: tag.get("event").and_then(VibrationInfo::decode),
            selection_strategy: VibrationSelector::decode(tag.get("selector")?)?,
            // TagValueInput#read retains this optional field's count-0 partial.
            travel_time_in_ticks: tag
                .get_int("event_delay")
                .filter(|value| *value >= 0)
                .unwrap_or(0),
            reload_vibration_particle: true,
        })
    }
    pub(super) fn save(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.insert("event_delay", Tag::Int(self.travel_time_in_ticks));
        tag.insert("selector", self.selection_strategy.save());
        if let Some(value) = &self.current_vibration {
            tag.insert("event", value.save());
        }
        Tag::Compound(tag)
    }
}
