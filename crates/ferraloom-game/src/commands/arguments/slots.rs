//! Port of SlotRanges, SlotArgument and SlotsArgument.
use super::word;
use crate::commands::exceptions::error_at;
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotRange {
    pub name: String,
    pub slots: Vec<i32>,
}
pub fn parse(reader: &mut StringReader, single: bool) -> Result<SlotRange, CommandSyntaxException> {
    let name = word(reader);
    let slots = lookup(&name)
        .ok_or_else(|| error_at(reader, "slot.unknown", std::slice::from_ref(&name)))?;
    if single && slots.len() != 1 {
        return Err(error_at(reader, "slot.only_single_allowed", &[name]));
    }
    Ok(SlotRange { name, slots })
}
pub fn lookup(name: &str) -> Option<Vec<i32>> {
    let one = match name {
        "contents" => Some(0),
        "weapon" | "weapon.mainhand" => Some(98),
        "weapon.offhand" => Some(99),
        "armor.head" => Some(103),
        "armor.chest" => Some(102),
        "armor.legs" => Some(101),
        "armor.feet" => Some(100),
        "armor.body" => Some(105),
        "saddle" => Some(106),
        "horse.chest" | "player.cursor" => Some(499),
        _ => None,
    };
    if let Some(slot) = one {
        return Some(vec![slot]);
    }
    if name == "weapon.*" {
        return Some(vec![98, 99]);
    }
    if name == "armor.*" {
        return Some(vec![103, 102, 101, 100, 105]);
    }
    for (prefix, offset, size) in [
        ("container.", 0, 54),
        ("hotbar.", 0, 9),
        ("inventory.", 9, 27),
        ("enderchest.", 200, 27),
        ("mob.inventory.", 300, 8),
        ("horse.", 500, 15),
        ("player.crafting.", 500, 4),
    ] {
        if let Some(index) = name.strip_prefix(prefix) {
            if index == "*" {
                return Some((offset..offset + size).collect());
            }
            let index = index.parse::<i32>().ok()?;
            if index >= 0 && index < size && format!("{prefix}{index}") == name {
                return Some(vec![offset + index]);
            }
            return None;
        }
    }
    None
}
