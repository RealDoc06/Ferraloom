//! Java 26.3 saved-data codec. Field names/defaults come from the target's
//! ScoreboardSaveData.Packed, Objective.Packed, Score.Packed and PlayerTeam.Packed
//! codecs, checked against an actual DataVersion 5023 scoreboard.dat.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use pumpkin_nbt::{
    NbtCompound,
    nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag},
    tag::NbtTag,
};
use pumpkin_util::{
    text::{TextContent, color::ARGBColor, style::Style},
    version::JavaMinecraftVersion,
};
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;

use super::*;

/// Keep unmodeled fields on their owning records, not a second live scoreboard.
/// Removed records must also lose their retained data before names can be reused.
#[derive(Clone, Debug, Default)]
pub(super) struct PreservedData {
    root: NbtCompound,
    pub(super) objectives: FxHashMap<String, NbtCompound>,
    pub(super) scores: FxHashMap<String, FxHashMap<String, NbtCompound>>,
    pub(super) teams: FxHashMap<String, NbtCompound>,
}

impl PreservedData {
    pub(super) fn remove_display_objective(&mut self, name: &str) {
        if let Some(NbtTag::Compound(data)) = self.root.child_tags.get_mut("data")
            && let Some(NbtTag::Compound(slots)) = data.child_tags.get_mut("DisplaySlots")
        {
            slots
                .child_tags
                .retain(|_, value| value.extract_string() != Some(name));
        }
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn field<'a, T>(
    data: &'a NbtCompound,
    key: &str,
    decode: impl FnOnce(&'a NbtTag) -> Option<T>,
) -> io::Result<Option<T>> {
    data.get(key)
        .map(|tag| decode(tag).ok_or_else(|| invalid(format!("Invalid scoreboard field {key}"))))
        .transpose()
}

fn string<'a>(data: &'a NbtCompound, key: &str) -> io::Result<&'a str> {
    field(data, key, NbtTag::extract_string)?
        .ok_or_else(|| invalid(format!("Missing scoreboard field {key}")))
}

fn list<'a>(data: &'a NbtCompound, key: &str) -> io::Result<&'a [NbtTag]> {
    Ok(field(data, key, NbtTag::extract_list)?.unwrap_or_default())
}

fn compound(tag: &NbtTag) -> io::Result<&NbtCompound> {
    match tag {
        NbtTag::Compound(data) => Ok(data),
        _ => Err(invalid("Expected scoreboard compound")),
    }
}

impl Scoreboard {
    /// Only a missing file creates an empty board. I/O and schema errors abort
    /// loading before a server or any world can save replacement defaults.
    pub(crate) fn load(world_path: &Path) -> io::Result<Self> {
        let path = world_path.join("data/minecraft/scoreboard.dat");
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A broken symlink is an existing, unreadable file, not a new world.
                match fs::symlink_metadata(&path) {
                    Err(missing) if missing.kind() == io::ErrorKind::NotFound => {
                        return Ok(Self::new());
                    }
                    _ => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        let root = read_gzip_compound_tag(file)
            .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
        Self::from_saved_data(root).map_err(|error| invalid(format!("{}: {error}", path.display())))
    }

    pub(crate) fn save(&self, world_path: &Path) -> io::Result<()> {
        let root = self.to_saved_data()?;
        atomic_write(&world_path.join("data/minecraft/scoreboard.dat"), |file| {
            write_gzip_compound_tag(root, file).map_err(|error| match error {
                pumpkin_nbt::Error::Incomplete(error) => error,
                error => invalid(error.to_string()),
            })
        })
    }

    fn from_saved_data(mut root: NbtCompound) -> io::Result<Self> {
        if let Some(version) = field(&root, "DataVersion", NbtTag::extract_int)?
            && version > MAXIMUM_SUPPORTED_WORLD_DATA_VERSION
        {
            return Err(invalid(format!(
                "Unsupported scoreboard DataVersion {version}"
            )));
        }
        let data = root
            .get_compound("data")
            .ok_or_else(|| invalid("Missing scoreboard data compound"))?;
        let mut board = Self::new();
        for tag in list(data, "Objectives")? {
            let record = compound(tag)?;
            let name = string(record, "Name")?;
            if board.objectives.contains_key(name) {
                return Err(invalid(format!("Duplicate scoreboard objective {name}")));
            }
            let display = record
                .get("DisplayName")
                .ok_or_else(|| invalid("Missing objective DisplayName"))?;
            let render_type =
                match field(record, "RenderType", NbtTag::extract_string)?.unwrap_or("integer") {
                    "integer" => RenderType::Integer,
                    "hearts" => RenderType::Hearts,
                    value => return Err(invalid(format!("Unknown scoreboard RenderType {value}"))),
                };
            // These are not exposed by gameplay yet, but must survive migration.
            field(record, "display_auto_update", NbtTag::extract_bool)?;
            board.add_objective(
                &NoTarget,
                ScoreboardObjective::new(
                    name,
                    read_text(display)?,
                    render_type,
                    read_format(record.get("format"))?,
                    field(record, "CriteriaName", NbtTag::extract_string)?.unwrap_or("dummy"),
                ),
            );
            board
                .preserved
                .objectives
                .insert(name.to_owned(), record.clone());
        }
        for tag in list(data, "PlayerScores")? {
            let record = compound(tag)?;
            let name = string(record, "Name")?;
            let objective = string(record, "Objective")?;
            if !board.objectives.contains_key(objective)
                || board.get_score(name, objective).is_some()
            {
                return Err(invalid(format!(
                    "Invalid or duplicate score {name}/{objective}"
                )));
            }
            board.update_score(
                &NoTarget,
                ScoreboardScore {
                    entity_name: name.to_owned(),
                    objective_name: objective.to_owned(),
                    value: VarInt(field(record, "Score", NbtTag::extract_int)?.unwrap_or(0)),
                    // Score.Packed's absent Locked default is false, unlike new scores.
                    locked: field(record, "Locked", NbtTag::extract_bool)?.unwrap_or(false),
                    display_name: record.get("display").map(read_text).transpose()?,
                    number_format: read_format(record.get("format"))?,
                },
            );
            board
                .preserved
                .scores
                .entry(objective.to_owned())
                .or_default()
                .insert(name.to_owned(), record.clone());
        }
        for tag in list(data, "Teams")? {
            let record = compound(tag)?;
            let name = string(record, "Name")?;
            if board.teams.contains_key(name) {
                return Err(invalid(format!("Duplicate scoreboard team {name}")));
            }
            let mut players = Vec::new();
            for member in list(record, "Players")? {
                let player = member
                    .extract_string()
                    .ok_or_else(|| invalid("Invalid team member"))?;
                players.push(player.to_owned());
            }
            visibility(
                field(record, "DeathMessageVisibility", NbtTag::extract_string)?
                    .unwrap_or("always"),
            )?;
            board.add_team(
                &NoTarget,
                Team {
                    name: name.to_owned(),
                    display_name: record
                        .get("DisplayName")
                        .map(read_text)
                        .transpose()?
                        .unwrap_or_else(|| TextComponent::text(name.to_owned())),
                    options: i8::from(
                        field(record, "AllowFriendlyFire", NbtTag::extract_bool)?.unwrap_or(true),
                    ) | (i8::from(
                        field(record, "SeeFriendlyInvisibles", NbtTag::extract_bool)?
                            .unwrap_or(true),
                    ) << 1),
                    nametag_visibility: visibility(
                        field(record, "NameTagVisibility", NbtTag::extract_string)?
                            .unwrap_or("always"),
                    )?,
                    collision_rule: collision(
                        field(record, "CollisionRule", NbtTag::extract_string)?.unwrap_or("always"),
                    )?,
                    color: field(record, "TeamColor", NbtTag::extract_string)?
                        .map(|color| {
                            NamedColor::try_from(color)
                                .map_err(|()| invalid(format!("Unknown team color {color}")))
                        })
                        .transpose()?
                        .unwrap_or(NamedColor::White),
                    player_prefix: record
                        .get("MemberNamePrefix")
                        .map(read_text)
                        .transpose()?
                        .unwrap_or_else(TextComponent::empty),
                    player_suffix: record
                        .get("MemberNameSuffix")
                        .map(read_text)
                        .transpose()?
                        .unwrap_or_else(TextComponent::empty),
                    players,
                },
            );
            board
                .preserved
                .teams
                .insert(name.to_owned(), record.clone());
        }
        let mut retained_slots = NbtCompound::new();
        if let Some(tag) = data.get("DisplaySlots") {
            for (name, value) in &compound(tag)?.child_tags {
                let objective = value
                    .extract_string()
                    .ok_or_else(|| invalid("Invalid display slot objective"))?;
                if let Some((slot, _)) = SLOTS.iter().find(|(_, key)| *key == name.as_ref()) {
                    if !board.objectives.contains_key(objective) {
                        return Err(invalid(format!("Unknown display objective {objective}")));
                    }
                    board.set_display_objective(&NoTarget, *slot, Some(objective));
                } else {
                    retained_slots.put(name, value.clone());
                }
            }
        }
        let mut retained_data = data.clone();
        for key in ["Objectives", "PlayerScores", "Teams"] {
            retained_data.child_tags.remove(key);
        }
        retained_data.put_compound("DisplaySlots", retained_slots);
        root.put_compound("data", retained_data);
        board.preserved.root = root;
        Ok(board)
    }

    fn to_saved_data(&self) -> io::Result<NbtCompound> {
        let mut root = self.preserved.root.clone();
        root.put_int("DataVersion", MAXIMUM_SUPPORTED_WORLD_DATA_VERSION);
        let mut data = root.get_compound("data").cloned().unwrap_or_default();
        let mut objectives = Vec::with_capacity(self.objectives.len());
        for objective in self.objectives.values() {
            let mut record = self
                .preserved
                .objectives
                .get(&objective.name)
                .cloned()
                .unwrap_or_default();
            record.put_string("Name", objective.name.clone());
            record.put_string("CriteriaName", objective.criterion.clone());
            record.put_string(
                "RenderType",
                match objective.render_type {
                    RenderType::Integer => "integer",
                    RenderType::Hearts => "hearts",
                }
                .into(),
            );
            put_text(&mut record, "DisplayName", Some(&objective.display_name))?;
            put_format(&mut record, objective.number_format.as_ref())?;
            objectives.push(NbtTag::Compound(record));
        }
        let mut scores = Vec::new();
        for (objective, entries) in &self.scores {
            for score in entries.values() {
                let mut record = self
                    .preserved
                    .scores
                    .get(objective)
                    .and_then(|entries| entries.get(&score.entity_name))
                    .cloned()
                    .unwrap_or_default();
                record.put_string("Name", score.entity_name.clone());
                record.put_string("Objective", score.objective_name.clone());
                record.put_int("Score", score.value.0);
                record.put_bool("Locked", score.locked);
                put_text(&mut record, "display", score.display_name.as_ref())?;
                put_format(&mut record, score.number_format.as_ref())?;
                scores.push(NbtTag::Compound(record));
            }
        }
        let mut teams = Vec::with_capacity(self.teams.len());
        for team in self.teams.values() {
            let original = self.preserved.teams.get(&team.name);
            let mut record = original.cloned().unwrap_or_default();
            record.put_string("Name", team.name.clone());
            put_text(&mut record, "DisplayName", Some(&team.display_name))?;
            put_text(&mut record, "MemberNamePrefix", Some(&team.player_prefix))?;
            put_text(&mut record, "MemberNameSuffix", Some(&team.player_suffix))?;
            record.put_bool("AllowFriendlyFire", team.options & 1 != 0);
            record.put_bool("SeeFriendlyInvisibles", team.options & 2 != 0);
            record.put_string("NameTagVisibility", team.nametag_visibility.to_str().into());
            record.put_string("CollisionRule", team.collision_rule.to_str().into());
            // Java's absent color means no team color. The engine only models
            // named colors: do not turn an unchanged absence into explicit white.
            if team.color != NamedColor::White
                || original.is_none_or(|record| record.get("TeamColor").is_some())
            {
                record.put_string("TeamColor", team.color.name().into());
            }
            record.put_list(
                "Players",
                team.players
                    .iter()
                    .map(|name| NbtTag::String(name.clone().into()))
                    .collect(),
            );
            teams.push(NbtTag::Compound(record));
        }
        let mut slots = data
            .get_compound("DisplaySlots")
            .cloned()
            .unwrap_or_default();
        for (slot, name) in &self.display_slots {
            let key = SLOTS
                .iter()
                .find(|(candidate, _)| candidate == slot)
                .ok_or_else(|| invalid("Unknown modeled display slot"))?
                .1;
            slots.put_string(key, name.clone());
        }
        data.put_list("Objectives", objectives);
        data.put_list("PlayerScores", scores);
        data.put_list("Teams", teams);
        data.put_compound("DisplaySlots", slots);
        root.put_compound("data", data);
        Ok(root)
    }
}

fn visibility(value: &str) -> io::Result<NameTagVisibility> {
    match value {
        "always" => Ok(NameTagVisibility::Always),
        "never" => Ok(NameTagVisibility::Never),
        "hideForOtherTeams" => Ok(NameTagVisibility::HideForOtherTeams),
        "hideForOwnTeam" => Ok(NameTagVisibility::HideForOwnTeam),
        _ => Err(invalid(format!("Unknown team visibility {value}"))),
    }
}

fn collision(value: &str) -> io::Result<CollisionRule> {
    match value {
        "always" => Ok(CollisionRule::Always),
        "never" => Ok(CollisionRule::Never),
        "pushOtherTeams" => Ok(CollisionRule::PushOtherTeams),
        "pushOwnTeam" => Ok(CollisionRule::PushOwnTeam),
        _ => Err(invalid(format!("Unknown team collision rule {value}"))),
    }
}

fn read_format(tag: Option<&NbtTag>) -> io::Result<Option<NumberFormat>> {
    let Some(tag) = tag else { return Ok(None) };
    let data = compound(tag)?;
    let kind = string(data, "type")?;
    Ok(Some(
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "blank" => NumberFormat::Blank,
            "fixed" => NumberFormat::Fixed(read_text(
                data.get("value")
                    .ok_or_else(|| invalid("Missing fixed format value"))?,
            )?),
            // StyledFormat uses Style's map codec inline, not a nested style field.
            "styled" => NumberFormat::Styled(
                serde_json::from_value::<Style>(text_json(tag)?)
                    .map_err(|error| invalid(format!("Invalid scoreboard style: {error}")))?,
            ),
            _ => {
                return Err(invalid(format!(
                    "Unsupported scoreboard number format {kind}"
                )));
            }
        },
    ))
}

fn put_format(data: &mut NbtCompound, format: Option<&NumberFormat>) -> io::Result<()> {
    if read_format(data.get("format"))?.as_ref() == format {
        return Ok(());
    }
    let Some(format) = format else {
        data.child_tags.remove("format");
        return Ok(());
    };
    let mut encoded = NbtCompound::new();
    let kind = match format {
        NumberFormat::Blank => "minecraft:blank",
        NumberFormat::Fixed(text) => {
            encoded.put("value", encode_text(text));
            "minecraft:fixed"
        }
        NumberFormat::Styled(style) => {
            let mut text = TextComponent::empty();
            text.0.style = Box::new(style.clone());
            encoded = text.0.to_nbt_compound();
            encoded.child_tags.remove("text");
            for value in encoded.child_tags.values_mut() {
                encode_object_contents(value);
            }
            "minecraft:styled"
        }
    };
    encoded.put_string("type", kind.into());
    data.put_compound("format", encoded);
    Ok(())
}

fn read_text(tag: &NbtTag) -> io::Result<TextComponent> {
    match tag {
        NbtTag::String(value) => Ok(TextComponent::text(value.to_string())),
        NbtTag::List(children) if !children.is_empty() => {
            let mut text = TextComponent::empty();
            text.0.extra = children
                .iter()
                .map(|tag| read_text(tag).map(|text| text.0))
                .collect::<io::Result<_>>()?;
            Ok(text)
        }
        NbtTag::Compound(data) => {
            if data.child_tags.len() == 1
                && let Some(value) = data.get("")
            {
                return read_text(value);
            }
            let mut content = data.clone();
            content.child_tags.remove("extra");
            content.child_tags.remove("with");
            let mut text = if let Some(player) = data.get("player")
                && !["text", "translate", "selector", "keybind"]
                    .iter()
                    .any(|key| data.get(key).is_some())
            {
                // 26.3 ObjectContents / PlayerSprite uses type=object, object=player.
                // The existing text model exposes this as TextContent::PlayerSprite.
                let profile = compound(player)?.clone();
                let mut text = TextComponent::player_sprite(
                    profile,
                    field(data, "hat", NbtTag::extract_bool)?.unwrap_or(true),
                );
                text.0.style = Box::new(
                    serde_json::from_value(text_json(&NbtTag::Compound(content))?).map_err(
                        |error| invalid(format!("Invalid scoreboard sprite style: {error}")),
                    )?,
                );
                text
            } else {
                serde_json::from_value::<TextComponent>(text_json(&NbtTag::Compound(content))?)
                    .map_err(|error| {
                        invalid(format!("Invalid or unsupported scoreboard text: {error}"))
                    })?
            };
            text.0.extra = list(data, "extra")?
                .iter()
                .map(|tag| read_text(tag).map(|text| text.0))
                .collect::<io::Result<_>>()?;
            if let TextContent::Translate { with, .. } = text.0.content.as_mut() {
                *with = list(data, "with")?
                    .iter()
                    .map(|tag| {
                        // Java translation arguments also admit primitive values. Keep
                        // their exact original tags on disk while projecting display text.
                        let text = match tag {
                            NbtTag::Byte(value) => TextComponent::text((value != &0).to_string()),
                            NbtTag::Short(value) => TextComponent::text(value.to_string()),
                            NbtTag::Int(value) => TextComponent::text(value.to_string()),
                            NbtTag::Long(value) => TextComponent::text(value.to_string()),
                            NbtTag::Float(value) => TextComponent::text(value.to_string()),
                            NbtTag::Double(value) => TextComponent::text(value.to_string()),
                            _ => read_text(tag)?,
                        };
                        Ok(text.0)
                    })
                    .collect::<io::Result<_>>()?;
            }
            Ok(text)
        }
        _ => Err(invalid("Invalid scoreboard text component")),
    }
}

fn encode_text(text: &TextComponent) -> NbtTag {
    // Do not use TextComponent's Serialize/encoding path: it translates text
    // for a client. Saved data must retain translation keys and arguments.
    let mut tag = text.0.to_nbt_tag_for_version(&JavaMinecraftVersion::V_26_3);
    encode_object_contents(&mut tag);
    tag
}

/// Adapt the engine's player-sprite spelling to the target saved-data codec,
/// including nested translation arguments, children and hover text.
fn encode_object_contents(tag: &mut NbtTag) {
    match tag {
        NbtTag::Compound(data) => {
            if data.get_string("type") == Some("minecraft:player_sprite") {
                data.put_string("type", "object".into());
                data.put_string("object", "player".into());
            }
            for value in data.child_tags.values_mut() {
                encode_object_contents(value);
            }
        }
        NbtTag::List(values) => {
            for value in values {
                encode_object_contents(value);
            }
        }
        _ => {}
    }
}

fn put_text(data: &mut NbtCompound, key: &str, text: Option<&TextComponent>) -> io::Result<()> {
    if data.get(key).map(read_text).transpose()?.as_ref() == text {
        return Ok(()); // Retain unknown fields and the exact original text tree.
    }
    if let Some(text) = text {
        data.put(key, encode_text(text));
    } else {
        data.child_tags.remove(key);
    }
    Ok(())
}

/// Adapt NBT booleans and compact children to the engine's serde text model.
/// Unlike TextComponent::from_nbt, decoding errors never become empty text.
fn text_json(tag: &NbtTag) -> io::Result<serde_json::Value> {
    use serde_json::{Value, json};
    Ok(match tag {
        NbtTag::String(value) => Value::String(value.to_string()),
        NbtTag::Byte(value) => json!(value),
        NbtTag::Short(value) => json!(value),
        NbtTag::Int(value) => json!(value),
        NbtTag::Long(value) => json!(value),
        NbtTag::Float(value) => json!(value),
        NbtTag::Double(value) => json!(value),
        NbtTag::List(values) => {
            Value::Array(values.iter().map(text_json).collect::<io::Result<_>>()?)
        }
        NbtTag::Compound(data) => {
            // NbtOps uses an empty-name wrapper for heterogeneous list elements.
            if data.child_tags.len() == 1
                && let Some(value) = data.get("")
            {
                return text_json(value);
            }
            let mut object = serde_json::Map::new();
            for (key, value) in &data.child_tags {
                let value = match key.as_ref() {
                    "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" | "hat" => {
                        Value::Bool(
                            value
                                .extract_bool()
                                .ok_or_else(|| invalid(format!("Invalid text boolean {key}")))?,
                        )
                    }
                    "shadow_color" => {
                        let color = value
                            .extract_int()
                            .ok_or_else(|| invalid("Unsupported text shadow_color"))?;
                        let color = ARGBColor::from_argb_int(color);
                        json!({"alpha": color.alpha, "red": color.red, "green": color.green, "blue": color.blue})
                    }
                    "extra" | "with" => {
                        let children = value
                            .extract_list()
                            .ok_or_else(|| invalid("Invalid text children"))?;
                        Value::Array(
                            children
                                .iter()
                                .map(|child| text_json(child).map(component_object))
                                .collect::<io::Result<_>>()?,
                        )
                    }
                    _ => text_json(value)?,
                };
                object.insert(key.to_string(), value);
            }
            // The engine stores hover text as a vector even for compact strings.
            if let Some(Value::Object(hover)) = object.get_mut("hover_event") {
                let key = match hover.get("action").and_then(Value::as_str) {
                    Some("show_text") => Some("value"),
                    Some("show_entity") => Some("name"),
                    _ => None,
                };
                if let Some(key) = key
                    && let Some(value) = hover.remove(key)
                {
                    let values = match value {
                        Value::Array(values) => values,
                        value => vec![value],
                    };
                    hover.insert(
                        key.into(),
                        Value::Array(values.into_iter().map(component_object).collect()),
                    );
                }
            }
            Value::Object(object)
        }
        NbtTag::IntArray(values) => json!(values),
        NbtTag::LongArray(values) => json!(values),
        NbtTag::ByteArray(values) => json!(values),
        NbtTag::End => return Err(invalid("Unexpected end tag in scoreboard text")),
    })
}

fn component_object(value: serde_json::Value) -> serde_json::Value {
    use serde_json::{Value, json};
    match value {
        Value::String(text) => json!({"text": text}),
        Value::Array(extra) => {
            json!({"text": "", "extra": extra.into_iter().map(component_object).collect::<Vec<_>>()})
        }
        value => value,
    }
}

/// Stage bytes beside the destination and sync them before the only operation
/// that replaces the original. Every failure before rename leaves it intact.
fn atomic_write(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| invalid("Scoreboard path has no parent"))?;
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".scoreboard-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        write(&mut file)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

const SLOTS: [(ScoreboardDisplaySlot, &str); 19] = {
    use ScoreboardDisplaySlot::*;
    [
        (List, "list"),
        (Sidebar, "sidebar"),
        (BelowName, "below_name"),
        (TeamBlack, "sidebar.team.black"),
        (TeamDarkBlue, "sidebar.team.dark_blue"),
        (TeamDarkGreen, "sidebar.team.dark_green"),
        (TeamDarkAqua, "sidebar.team.dark_aqua"),
        (TeamDarkRed, "sidebar.team.dark_red"),
        (TeamDarkPurple, "sidebar.team.dark_purple"),
        (TeamGold, "sidebar.team.gold"),
        (TeamGray, "sidebar.team.gray"),
        (TeamDarkGray, "sidebar.team.dark_gray"),
        (TeamBlue, "sidebar.team.blue"),
        (TeamGreen, "sidebar.team.green"),
        (TeamAqua, "sidebar.team.aqua"),
        (TeamRed, "sidebar.team.red"),
        (TeamLightPurple, "sidebar.team.light_purple"),
        (TeamYellow, "sidebar.team.yellow"),
        (TeamWhite, "sidebar.team.white"),
    ]
};

#[cfg(test)]
mod tests;
