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
    let custom = matches!(
        name.as_str(),
        "dummy"
            | "trigger"
            | "deathCount"
            | "playerKillCount"
            | "totalKillCount"
            | "health"
            | "food"
            | "air"
            | "armor"
            | "xp"
            | "level"
    ) || ["teamkill.", "killedByTeam."].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|color| COLORS.contains(&color))
    });
    let stat = name.split_once(':').is_some_and(|(kind, value)| {
        let kind = by_separator(kind);
        let registry = match kind.as_str() {
            "minecraft:mined" => "minecraft:block",
            "minecraft:crafted"
            | "minecraft:used"
            | "minecraft:broken"
            | "minecraft:picked_up"
            | "minecraft:dropped" => "minecraft:item",
            "minecraft:killed" | "minecraft:killed_by" => "minecraft:entity_type",
            "minecraft:custom" => "minecraft:custom_stat",
            _ => return false,
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
        read_only: stat
            || matches!(
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
