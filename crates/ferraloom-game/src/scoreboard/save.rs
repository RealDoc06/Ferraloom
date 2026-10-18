//! Port of ScoreboardSaveData.Packed.CODEC. Storage supplies the DataVersion envelope and gzip IO.
use super::*;
use crate::commands::arguments::scoreboard::{COLORS, DISPLAY_SLOTS};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{Reader, Wire, chat::Style};
use ferraloom_storage::{Error as StorageError, Result as StorageResult, SavedDataCodec};

pub const VISIBILITIES: &[&str] = &["always", "never", "hideForOtherTeams", "hideForOwnTeam"];
pub const COLLISIONS: &[&str] = &["always", "never", "pushOtherTeams", "pushOwnTeam"];
/// Port of ScoreboardSaveData. Payload only; SavedData owns its DataVersion and dirty envelope.
#[derive(Clone, Debug, Default)]
pub struct ScoreboardSaveData(pub Scoreboard);
impl ScoreboardSaveData {
    pub const STORAGE_ID: &'static str = "minecraft:scoreboard";
}
fn invalid(message: impl Into<String>) -> StorageError {
    StorageError::Invalid(format!("scoreboard: {}", message.into()))
}
fn protocol<T>(result: ferraloom_protocol::Result<T>) -> StorageResult<T> {
    result.map_err(|e| invalid(e.to_string()))
}
fn compound(tag: &Tag) -> StorageResult<&CompoundTag> {
    tag.as_compound()
        .ok_or_else(|| invalid("expected compound"))
}
fn string(tag: &Tag) -> StorageResult<String> {
    tag.as_string()
        .map(|s| s.to_string_lossy())
        .ok_or_else(|| invalid("expected string"))
}
fn required<'a>(map: &'a CompoundTag, key: &str) -> StorageResult<&'a Tag> {
    map.get(key)
        .ok_or_else(|| invalid(format!("missing {key}")))
}
fn text(map: &CompoundTag, key: &str) -> StorageResult<String> {
    string(required(map, key)?)
}
fn default_text(map: &CompoundTag, key: &str, default: &str) -> StorageResult<String> {
    map.get(key)
        .map(string)
        .transpose()
        .map(|s| s.unwrap_or_else(|| default.to_owned()))
}
// PrimitiveCodec.INT uses Number.intValue, not NumericTag.getAsInt's floor.
fn integer(tag: &Tag) -> StorageResult<i32> {
    match tag {
        Tag::Byte(v) => Ok(i32::from(*v)),
        Tag::Short(v) => Ok(i32::from(*v)),
        Tag::Int(v) => Ok(*v),
        Tag::Long(v) => Ok(*v as i32),
        Tag::Float(v) => Ok(*v as i32),
        Tag::Double(v) => Ok(*v as i32),
        _ => Err(invalid("expected number")),
    }
}
fn boolean(map: &CompoundTag, key: &str, default: bool) -> StorageResult<bool> {
    map.get(key)
        .map(|value| {
            value
                .as_double()
                .map(|value| value != 0.0)
                .ok_or_else(|| invalid(format!("invalid {key}")))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn list<'a>(map: &'a CompoundTag, key: &str) -> StorageResult<&'a [Tag]> {
    map.get(key)
        .map(|v| {
            v.as_list()
                .map(|v| v.0.as_slice())
                .ok_or_else(|| invalid(format!("invalid {key}")))
        })
        .transpose()
        .map(|v| v.unwrap_or(&[]))
}
fn component(map: &CompoundTag, key: &str) -> StorageResult<Option<Component>> {
    map.get(key)
        .map(|v| protocol(Component::from_nbt(v.clone())))
        .transpose()
}
pub(super) fn enum_value<T: Wire>(name: &str, names: &[&str]) -> StorageResult<T> {
    let index = names
        .iter()
        .position(|&v| v == name)
        .ok_or_else(|| invalid(format!("unknown enum value {name}")))?;
    protocol(T::decode(&mut Reader::new(&[index as u8])))
}
fn put(map: &mut CompoundTag, key: &str, value: &str) {
    map.insert(key, Tag::String(value.into()));
}
fn put_component(map: &mut CompoundTag, key: &str, value: &Component) -> StorageResult<()> {
    map.insert(key, protocol(value.to_nbt())?);
    Ok(())
}
fn put_list(map: &mut CompoundTag, key: &str, values: Vec<Tag>) {
    if !values.is_empty() {
        map.insert(key, Tag::List(ListTag(values)));
    }
}
fn put_bool(map: &mut CompoundTag, key: &str, value: bool, default: bool) {
    if value != default {
        map.insert(key, Tag::Byte(i8::from(value)));
    }
}
fn format_decode(tag: &Tag) -> StorageResult<NumberFormat> {
    let map = compound(tag)?;
    let kind = text(map, "type")?;
    match kind.strip_prefix("minecraft:").unwrap_or(&kind) {
        "blank" => Ok(NumberFormat::Blank {}),
        "fixed" => Ok(NumberFormat::Fixed {
            value: component(map, "value")?.ok_or_else(|| invalid("missing fixed value"))?,
        }),
        "styled" => Ok(NumberFormat::Styled {
            style: protocol(Style::from_nbt(tag.clone()))?,
        }),
        name => Err(invalid(format!("unknown number format {name}"))),
    }
}
fn format_encode(value: &NumberFormat) -> StorageResult<Tag> {
    let mut map = CompoundTag::new();
    let kind = match value {
        NumberFormat::Blank {} => "blank",
        NumberFormat::Fixed { value } => {
            put_component(&mut map, "value", value)?;
            "fixed"
        }
        NumberFormat::Styled { style } => {
            map = compound(&protocol(style.to_nbt())?)?.clone();
            "styled"
        }
    };
    put(&mut map, "type", &format!("minecraft:{kind}"));
    Ok(Tag::Compound(map))
}
fn get_format(map: &CompoundTag) -> StorageResult<Option<NumberFormat>> {
    map.get("format").map(format_decode).transpose()
}
fn put_format(map: &mut CompoundTag, value: &Option<NumberFormat>) -> StorageResult<()> {
    if let Some(value) = value {
        map.insert("format", format_encode(value)?);
    }
    Ok(())
}

impl SavedDataCodec for ScoreboardSaveData {
    fn decode(tag: &Tag) -> StorageResult<Self> {
        let map = compound(tag)?;
        let mut board = Scoreboard::default();
        for value in list(map, "Objectives")? {
            let value = compound(value)?;
            let name = text(value, "Name")?;
            let criterion = default_text(value, "CriteriaName", "dummy")?;
            let criteria = ObjectiveCriteria::by_name(&criterion)
                .ok_or_else(|| invalid(format!("unknown criterion {criterion}")))?;
            let render_type = enum_value(
                &default_text(value, "RenderType", "integer")?,
                &["integer", "hearts"],
            )?;
            let display_name =
                component(value, "DisplayName")?.ok_or_else(|| invalid("missing DisplayName"))?;
            let objective = Objective {
                criteria,
                display_name,
                render_type,
                display_auto_update: boolean(value, "display_auto_update", false)?,
                number_format: get_format(value)?,
            };
            board
                .add_objective(name, objective)
                .map_err(|e| invalid(e.to_string()))?;
        }
        for value in list(map, "PlayerScores")? {
            let value = compound(value)?;
            let name = text(value, "Name")?;
            let objective = text(value, "Objective")?;
            let score = Score {
                value: value.get("Score").map(integer).transpose()?.unwrap_or(0),
                locked: boolean(value, "Locked", false)?,
                display: component(value, "display")?,
                number_format: get_format(value)?,
            };
            // Port of Scoreboard#loadPlayerScore: orphan scores are logged and ignored by vanilla.
            if board.get_objective(&objective).is_some() {
                *board
                    .get_or_create_score(&name, &objective)
                    .map_err(|e| invalid(e.to_string()))? = score;
            }
        }
        if let Some(slots) = map.get("DisplaySlots") {
            for (slot, name) in compound(slots)?.iter() {
                let slot = enum_value::<DisplaySlot>(&slot.to_string_lossy(), DISPLAY_SLOTS)?;
                let name = string(name)?;
                if board.get_objective(&name).is_some() {
                    board
                        .set_display_objective(slot, Some(name))
                        .map_err(|e| invalid(e.to_string()))?;
                }
            }
        }
        for value in list(map, "Teams")? {
            let value = compound(value)?;
            let name = text(value, "Name")?;
            let team = board.add_player_team(&name);
            if let Some(display) = component(value, "DisplayName")? {
                team.display_name = display;
            }
            team.prefix = component(value, "MemberNamePrefix")?.unwrap_or_default();
            team.suffix = component(value, "MemberNameSuffix")?.unwrap_or_default();
            team.color = value
                .get("TeamColor")
                .map(|v| enum_value(&string(v)?, COLORS))
                .transpose()?;
            team.allow_friendly_fire = boolean(value, "AllowFriendlyFire", true)?;
            team.see_friendly_invisibles = boolean(value, "SeeFriendlyInvisibles", true)?;
            team.name_tag_visibility = enum_value(
                &default_text(value, "NameTagVisibility", "always")?,
                VISIBILITIES,
            )?;
            team.death_message_visibility = enum_value(
                &default_text(value, "DeathMessageVisibility", "always")?,
                VISIBILITIES,
            )?;
            team.collision_rule =
                enum_value(&default_text(value, "CollisionRule", "always")?, COLLISIONS)?;
            for holder in list(value, "Players")? {
                board
                    .add_player_to_team(&string(holder)?, &name)
                    .map_err(|e| invalid(e.to_string()))?;
            }
        }
        Ok(Self(board))
    }
    fn encode(&self) -> StorageResult<Tag> {
        let board = &self.0;
        let mut map = CompoundTag::new();
        let mut objectives = Vec::new();
        for (name, objective) in board.objectives().rev() {
            let mut value = CompoundTag::new();
            put(&mut value, "Name", name);
            if objective.criteria.name != "dummy" {
                put(&mut value, "CriteriaName", &objective.criteria.name);
            }
            put_component(&mut value, "DisplayName", &objective.display_name)?;
            if objective.render_type == RenderType::Hearts {
                put(&mut value, "RenderType", "hearts");
            }
            put_bool(
                &mut value,
                "display_auto_update",
                objective.display_auto_update,
                false,
            );
            put_format(&mut value, &objective.number_format)?;
            objectives.push(Tag::Compound(value));
        }
        put_list(&mut map, "Objectives", objectives);
        let mut scores = Vec::new();
        for holder in board.tracked_players() {
            for (objective, score) in board.player_scores(holder) {
                let mut value = CompoundTag::new();
                put(&mut value, "Name", holder);
                put(&mut value, "Objective", objective);
                if score.value != 0 {
                    value.insert("Score", Tag::Int(score.value));
                }
                put_bool(&mut value, "Locked", score.locked, false);
                if let Some(display) = &score.display {
                    put_component(&mut value, "display", display)?;
                }
                put_format(&mut value, &score.number_format)?;
                scores.push(Tag::Compound(value));
            }
        }
        put_list(&mut map, "PlayerScores", scores);
        let mut slots = CompoundTag::new();
        for (index, objective) in board.display_slots.iter().enumerate() {
            if let Some(name) = objective {
                put(&mut slots, DISPLAY_SLOTS[index], name);
            }
        }
        if !slots.is_empty() {
            map.insert("DisplaySlots", Tag::Compound(slots));
        }
        let mut teams = Vec::new();
        for team in board.teams().rev() {
            let mut value = CompoundTag::new();
            put(&mut value, "Name", team.name());
            put_component(&mut value, "DisplayName", &team.display_name)?;
            if team.prefix != Component::default() {
                put_component(&mut value, "MemberNamePrefix", &team.prefix)?;
            }
            if team.suffix != Component::default() {
                put_component(&mut value, "MemberNameSuffix", &team.suffix)?;
            }
            if let Some(color) = team.color {
                put(&mut value, "TeamColor", COLORS[color as usize]);
            }
            put_bool(
                &mut value,
                "AllowFriendlyFire",
                team.allow_friendly_fire,
                true,
            );
            put_bool(
                &mut value,
                "SeeFriendlyInvisibles",
                team.see_friendly_invisibles,
                true,
            );
            if team.name_tag_visibility != Visibility::Always {
                put(
                    &mut value,
                    "NameTagVisibility",
                    VISIBILITIES[team.name_tag_visibility as usize],
                );
            }
            if team.death_message_visibility != Visibility::Always {
                put(
                    &mut value,
                    "DeathMessageVisibility",
                    VISIBILITIES[team.death_message_visibility as usize],
                );
            }
            if team.collision_rule != CollisionRule::Always {
                put(
                    &mut value,
                    "CollisionRule",
                    COLLISIONS[team.collision_rule as usize],
                );
            }
            put_list(
                &mut value,
                "Players",
                team.members()
                    .map(|s| Tag::String(s.as_str().into()))
                    .collect(),
            );
            teams.push(Tag::Compound(value));
        }
        put_list(&mut map, "Teams", teams);
        Ok(Tag::Compound(map))
    }
}
