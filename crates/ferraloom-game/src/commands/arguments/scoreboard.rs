//! Port of ScoreboardSlotArgument and ObjectiveCriteriaArgument.
use super::{enum_word, word};
use crate::commands::exceptions::error_at;
use ferraloom_brigadier::{CommandSyntaxException, StringReader};

pub const COLORS: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];
pub const DISPLAY_SLOTS: &[&str] = &[
    "list",
    "sidebar",
    "below_name",
    "sidebar.team.black",
    "sidebar.team.dark_blue",
    "sidebar.team.dark_green",
    "sidebar.team.dark_aqua",
    "sidebar.team.dark_red",
    "sidebar.team.dark_purple",
    "sidebar.team.gold",
    "sidebar.team.gray",
    "sidebar.team.dark_gray",
    "sidebar.team.blue",
    "sidebar.team.green",
    "sidebar.team.aqua",
    "sidebar.team.red",
    "sidebar.team.light_purple",
    "sidebar.team.yellow",
    "sidebar.team.white",
];
const CUSTOM_CRITERIA: &[&str] = &[
    "dummy",
    "trigger",
    "deathCount",
    "playerKillCount",
    "totalKillCount",
    "health",
    "food",
    "air",
    "armor",
    "xp",
    "level",
];
const STAT_TYPES: &[(&str, &str)] = &[
    ("minecraft:mined", "minecraft:block"),
    ("minecraft:crafted", "minecraft:item"),
    ("minecraft:used", "minecraft:item"),
    ("minecraft:broken", "minecraft:item"),
    ("minecraft:picked_up", "minecraft:item"),
    ("minecraft:dropped", "minecraft:item"),
    ("minecraft:killed", "minecraft:entity_type"),
    ("minecraft:killed_by", "minecraft:entity_type"),
    ("minecraft:custom", "minecraft:custom_stat"),
];
/// Port of ObjectiveCriteriaArgument#listSuggestions and Stat#buildName.
pub fn criteria_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = CUSTOM_CRITERIA
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        for prefix in ["teamkill.", "killedByTeam."] {
            names.extend(COLORS.iter().map(|color| format!("{prefix}{color}")));
        }
        for (kind, registry) in STAT_TYPES {
            for value in ferraloom_data::registry_entries(registry).unwrap_or_default() {
                names.push(format!(
                    "{}:{}",
                    kind.replace(':', "."),
                    value.replace(':', ".")
                ));
            }
        }
        names
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveCriteria {
    pub name: String,
    pub read_only: bool,
    pub hearts: bool,
}
pub fn display_slot(reader: &mut StringReader) -> Result<u8, CommandSyntaxException> {
    let name = enum_word(
        reader,
        DISPLAY_SLOTS,
        "argument.scoreboardDisplaySlot.invalid",
    )?;
    Ok(DISPLAY_SLOTS.iter().position(|slot| *slot == name).unwrap() as u8)
}
pub fn criteria(reader: &mut StringReader) -> Result<ObjectiveCriteria, CommandSyntaxException> {
    let start = reader.cursor();
    let name = word(reader);
    let custom = CUSTOM_CRITERIA.contains(&name.as_str())
        || ["teamkill.", "killedByTeam."].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|color| COLORS.contains(&color))
        });
    let stat = name.split_once(':').is_some_and(|(kind, value)| {
        let kind = by_separator(kind);
        let Some((_, registry)) = STAT_TYPES.iter().find(|(id, _)| *id == kind) else {
            return false;
        };
        let value = by_separator(value);
        ferraloom_data::registry_entries(registry)
            .is_some_and(|entries| entries.contains(&value.as_str()))
    });
    if !custom && !stat {
        reader.set_cursor(start);
        return Err(error_at(reader, "argument.criteria.invalid", &[name]));
    }
    Ok(ObjectiveCriteria {
        hearts: name == "health",
        read_only: matches!(
            name.as_str(),
            "health" | "food" | "air" | "armor" | "xp" | "level"
        ),
        name,
    })
}
fn by_separator(value: &str) -> String {
    value.split_once('.').map_or_else(
        || format!("minecraft:{value}"),
        |(namespace, path)| format!("{namespace}:{path}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_criteria_metadata_includes_writable_stats() {
        #[derive(serde::Deserialize)]
        struct Vector {
            name: String,
            read_only: bool,
            hearts: bool,
        }
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../vanilla/criteria-vectors.json")).unwrap();
        for vector in vectors {
            let mut reader = StringReader::new(&vector.name);
            let actual = criteria(&mut reader).unwrap();
            assert_eq!(reader.cursor(), vector.name.len(), "{}", vector.name);
            assert_eq!(actual.read_only, vector.read_only, "{}", vector.name);
            assert_eq!(actual.hearts, vector.hearts, "{}", vector.name);
            assert_eq!(actual.name, vector.name);
        }
    }
}
