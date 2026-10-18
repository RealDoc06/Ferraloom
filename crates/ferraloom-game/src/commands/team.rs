//! Port of TeamCommand. Membership mutation and packet ordering belong to ServerScoreboard.
use super::{
    PreparedCommand,
    exceptions::error,
    scoreboard::{
        self, Holder, Result, boolean, c, checked, component, enum_value, list, n, s, success,
    },
};
use crate::scoreboard::{CollisionRule, PlayerTeam, ServerScoreboard, TeamColor, Visibility};
use ferraloom_protocol::chat::{Component, TextColor};

pub fn execute(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    members: &[Holder],
) -> Result<super::builtins::CommandResult> {
    scoreboard::validate_components(command)?;
    let verb = command.literals[1].as_str();
    // Entity feedback must resolve against the post-mutation team, via the canonical entity naming API.
    if matches!(verb, "join" | "leave") && members.iter().any(|holder| holder.display.is_some()) {
        return Err(error("ferraloom.team.entity_feedback.unsupported", &[]));
    }
    let name = command.string("team");
    if verb == "list" && name.is_none() {
        let teams: Vec<_> = board
            .scoreboard()
            .teams()
            .map(PlayerTeam::formatted_display_name)
            .collect();
        let count = teams.len();
        return Ok(if count == 0 {
            success(0, "commands.team.list.teams.empty", vec![], false)
        } else {
            success(
                count as i32,
                "commands.team.list.teams.success",
                vec![n(count), c(list(teams))],
                false,
            )
        });
    }
    if verb == "leave" {
        let changed: Vec<_> = members
            .iter()
            .filter(|holder| board.remove_player_from_team(&holder.name))
            .collect();
        return Ok(if changed.len() == 1 {
            success(
                1,
                "commands.team.leave.success.single",
                vec![c(changed[0].feedback())],
                true,
            )
        } else {
            success(
                changed.len() as i32,
                "commands.team.leave.success.multiple",
                vec![n(changed.len())],
                true,
            )
        });
    }
    let name = name.unwrap();
    if verb == "add" {
        if board.scoreboard().get_player_team(name).is_some() {
            return Err(error("commands.team.add.duplicate", &[]));
        }
        board.add_player_team(name);
        let display = component(command, "displayName").unwrap_or_else(|| Component::literal(name));
        checked(board.update_team(name, |team| team.display_name = display))?;
        return Ok(success(
            board.scoreboard().teams().count() as i32,
            "commands.team.add.success",
            vec![c(board
                .scoreboard()
                .get_player_team(name)
                .unwrap()
                .formatted_display_name())],
            true,
        ));
    }
    let team = board
        .scoreboard()
        .get_player_team(name)
        .ok_or_else(|| error("team.notFound", &[name.to_owned()]))?;
    let formatted = team.formatted_display_name();
    match verb {
        "list" => {
            let mut members: Vec<_> = team.members().cloned().collect();
            members.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let count = members.len();
            let members = members.into_iter().map(|name| {
                let mut component = Component::literal(name);
                component.style.color = Some(TextColor::parse("green").unwrap());
                component
            });
            Ok(if count == 0 {
                success(
                    0,
                    "commands.team.list.members.empty",
                    vec![c(formatted)],
                    false,
                )
            } else {
                success(
                    count as i32,
                    "commands.team.list.members.success",
                    vec![c(formatted), n(count), c(list(members))],
                    false,
                )
            })
        }
        "remove" => {
            checked(board.remove_player_team(name))?;
            Ok(success(
                board.scoreboard().teams().count() as i32,
                "commands.team.remove.success",
                vec![c(formatted)],
                true,
            ))
        }
        "empty" => {
            let members: Vec<_> = team.members().cloned().collect();
            let count = members.len();
            if count == 0 {
                return Err(error("commands.team.empty.unchanged", &[]));
            }
            for member in members {
                board.remove_player_from_team(&member);
            }
            Ok(success(
                count as i32,
                "commands.team.empty.success",
                vec![n(count), c(formatted)],
                true,
            ))
        }
        "join" => {
            for holder in members {
                checked(board.add_player_to_team(&holder.name, name))?;
            }
            Ok(if members.len() == 1 {
                success(
                    1,
                    "commands.team.join.success.single",
                    vec![c(members[0].feedback()), c(formatted)],
                    true,
                )
            } else {
                success(
                    members.len() as i32,
                    "commands.team.join.success.multiple",
                    vec![n(members.len()), c(formatted)],
                    true,
                )
            })
        }
        "modify" => modify(board, command, name),
        _ => unreachable!("native team verb"),
    }
}
fn modify(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    name: &str,
) -> Result<super::builtins::CommandResult> {
    let option = command.literals[2].as_str();
    let current = board.scoreboard().get_player_team(name).unwrap();
    let (key, extra) = match option {
        "displayName" => {
            let value = component(command, "displayName").unwrap();
            if current.display_name == value {
                return Err(error("commands.team.option.name.unchanged", &[]));
            }
            checked(board.update_team(name, |team| team.display_name = value))?;
            ("commands.team.option.name.success".to_owned(), None)
        }
        "color" => {
            let value: Option<TeamColor> = command
                .string("value")
                .map(|value| enum_value(value, super::arguments::scoreboard::COLORS));
            if current.color == value {
                return Err(error("commands.team.option.color.unchanged", &[]));
            }
            checked(board.update_team(name, |team| team.color = value))?;
            if let Some(color) = value {
                (
                    "commands.team.option.color.success".into(),
                    Some(s(super::arguments::scoreboard::COLORS[color as usize])),
                )
            } else {
                ("commands.team.option.color.clear.success".into(), None)
            }
        }
        "friendlyFire" | "seeFriendlyInvisibles" => {
            let allowed = boolean(command, "allowed");
            let current = if option == "friendlyFire" {
                current.allow_friendly_fire
            } else {
                current.see_friendly_invisibles
            };
            let key = if option == "friendlyFire" {
                "friendlyfire"
            } else {
                "seeFriendlyInvisibles"
            };
            if current == allowed {
                return Err(error(
                    &format!(
                        "commands.team.option.{key}.already{}",
                        if allowed { "Enabled" } else { "Disabled" }
                    ),
                    &[],
                ));
            }
            checked(board.update_team(name, |team| {
                if option == "friendlyFire" {
                    team.allow_friendly_fire = allowed;
                } else {
                    team.see_friendly_invisibles = allowed;
                }
            }))?;
            (
                format!(
                    "commands.team.option.{key}.{}",
                    if allowed { "enabled" } else { "disabled" }
                ),
                None,
            )
        }
        "nametagVisibility" | "deathMessageVisibility" => {
            let value = command.literals[3].as_str();
            let visibility: Visibility = enum_value(
                value,
                &["always", "never", "hideForOtherTeams", "hideForOwnTeam"],
            );
            let old = if option == "nametagVisibility" {
                current.name_tag_visibility
            } else {
                current.death_message_visibility
            };
            if old == visibility {
                return Err(error(
                    &format!("commands.team.option.{option}.unchanged"),
                    &[],
                ));
            }
            checked(board.update_team(name, |team| {
                if option == "nametagVisibility" {
                    team.name_tag_visibility = visibility;
                } else {
                    team.death_message_visibility = visibility;
                }
            }))?;
            (
                format!("commands.team.option.{option}.success"),
                Some(c(Component::translatable(
                    format!("team.visibility.{value}"),
                    vec![],
                ))),
            )
        }
        "collisionRule" => {
            let value = command.literals[3].as_str();
            let rule: CollisionRule =
                enum_value(value, &["always", "never", "pushOtherTeams", "pushOwnTeam"]);
            if current.collision_rule == rule {
                return Err(error("commands.team.option.collisionRule.unchanged", &[]));
            }
            checked(board.update_team(name, |team| team.collision_rule = rule))?;
            (
                "commands.team.option.collisionRule.success".into(),
                Some(c(Component::translatable(
                    format!("team.collision.{value}"),
                    vec![],
                ))),
            )
        }
        "prefix" | "suffix" => {
            let value = component(command, option).unwrap();
            let feedback = value.clone();
            checked(board.update_team(name, |team| {
                if option == "prefix" {
                    team.prefix = value;
                } else {
                    team.suffix = value;
                }
            }))?;
            return Ok(success(
                1,
                &format!("commands.team.option.{option}.success"),
                vec![c(feedback)],
                false,
            ));
        }
        _ => unreachable!("native team option"),
    };
    let mut args = vec![c(board
        .scoreboard()
        .get_player_team(name)
        .unwrap()
        .formatted_display_name())];
    if let Some(extra) = extra {
        args.push(extra);
    }
    Ok(scoreboard::success(0, &key, args, true))
}
