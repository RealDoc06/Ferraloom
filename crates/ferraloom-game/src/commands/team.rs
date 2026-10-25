//! Port of TeamCommand. Membership mutation and packet ordering belong to ServerScoreboard.
use super::{
    PreparedCommand,
    exceptions::error,
    scoreboard::{Holder, Result, boolean, c, checked, component, enum_value, list, n, s, success},
};
use crate::scoreboard::{CollisionRule, PlayerTeam, ServerScoreboard, TeamColor, Visibility};
use ferraloom_protocol::chat::{Component, TextColor};

/// Port of TeamCommand's sendSuccess Supplier: membership changes are complete
/// before a single holder's feedback display name is read.
pub enum TeamCommandResult {
    Complete(super::builtins::CommandResult),
    Join {
        holder: Box<Holder>,
        team: Box<Component>,
    },
    Leave {
        holder: Box<Holder>,
    },
}
impl TeamCommandResult {
    /// Port of CommandResponseTracker#sendFeedback. This phase only reads;
    /// the adapter has ended its mutable scoreboard borrow before calling it.
    pub fn resolve_feedback(
        self,
        feedback: impl FnOnce(&Holder) -> Result<Component>,
    ) -> Result<super::builtins::CommandResult> {
        Ok(match self {
            Self::Complete(result) => result,
            Self::Join { holder, team } => success(
                1,
                "commands.team.join.success.single",
                vec![c(feedback(&holder)?), c(*team)],
                true,
            ),
            Self::Leave { holder } => success(
                1,
                "commands.team.leave.success.single",
                vec![c(feedback(&holder)?)],
                true,
            ),
        })
    }
}
/// Fake-holder callers need no entity lookup. Live entity callers use the
/// two-phase API so feedback observes the completed membership changes.
pub fn execute(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    members: &[Holder],
) -> Result<super::builtins::CommandResult> {
    let verb = command.literals[1].as_str();
    let single = match verb {
        "join" if members.len() == 1 => members.first(),
        "leave" => {
            let mut seen = std::collections::HashSet::new();
            let mut changed = members.iter().filter(|holder| {
                seen.insert(holder.name.as_str())
                    && board.scoreboard().get_players_team(&holder.name).is_some()
            });
            let first = changed.next();
            if changed.next().is_none() {
                first
            } else {
                None
            }
        }
        _ => None,
    };
    if single.is_some_and(|holder| holder.entity.is_some() || holder.display.is_some()) {
        return Err(error("ferraloom.team.entity_feedback.unsupported", &[]));
    }
    execute_with_feedback(board, command, members)?.resolve_feedback(|holder| Ok(holder.feedback()))
}
/// Preserve the pure scoreboard entry point for non-live callers.
pub fn execute_with_feedback(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    members: &[Holder],
) -> Result<TeamCommandResult> {
    execute_context(&mut TeamContext::Board(board), command, members).map_err(|error| match error {
        TeamError::Command(error) => error,
        TeamError::Level(_) => unreachable!("pure team context has no Level"),
    })
}
/// Execute the same evaluator with synchronous native waypoint mutation hooks.
pub fn execute_live(
    level: &mut crate::level::Level,
    command: &PreparedCommand,
    members: &[Holder],
) -> TeamResult<TeamCommandResult> {
    execute_context(&mut TeamContext::Live(level), command, members)
}
/// Port of TeamCommand#joinTeam/#leaveTeam: all mutations happen here,
/// synchronously, before the returned feedback supplier is evaluated. Component
/// arguments have already passed ComponentArgument resolution in the adapter.
fn execute_context(
    board: &mut TeamContext<'_>,
    command: &PreparedCommand,
    members: &[Holder],
) -> TeamResult<TeamCommandResult> {
    let verb = command.literals[1].as_str();
    let name = command.string("team");
    if verb == "list" && name.is_none() {
        let teams: Vec<_> = board
            .scoreboard()?
            .teams()
            .map(PlayerTeam::formatted_display_name)
            .collect();
        let count = teams.len();
        return Ok(TeamCommandResult::Complete(if count == 0 {
            success(0, "commands.team.list.teams.empty", vec![], false)
        } else {
            success(
                count as i32,
                "commands.team.list.teams.success",
                vec![n(count), c(list(teams))],
                false,
            )
        }));
    }
    if verb == "leave" {
        let mut changed = Vec::new();
        for holder in members {
            if board.remove_player_from_team(&holder.name)? {
                changed.push(holder);
            }
        }
        return Ok(if changed.len() == 1 {
            TeamCommandResult::Leave {
                holder: Box::new(changed[0].clone()),
            }
        } else {
            TeamCommandResult::Complete(success(
                changed.len() as i32,
                "commands.team.leave.success.multiple",
                vec![n(changed.len())],
                true,
            ))
        });
    }

    let name = name.unwrap();
    if verb == "add" {
        if board.scoreboard()?.get_player_team(name).is_some() {
            return Err(error("commands.team.add.duplicate", &[]).into());
        }
        board.add_player_team(name)?;
        let display = component(command, "displayName").unwrap_or_else(|| Component::literal(name));
        board.update_team(name, |team| team.display_name = display)?;
        return Ok(TeamCommandResult::Complete(success(
            board.scoreboard()?.teams().count() as i32,
            "commands.team.add.success",
            vec![c(board
                .scoreboard()?
                .get_player_team(name)
                .unwrap()
                .formatted_display_name())],
            true,
        )));
    }
    let team = board
        .scoreboard()?
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
            Ok(TeamCommandResult::Complete(if count == 0 {
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
            }))
        }
        "remove" => {
            board.remove_player_team(name)?;
            Ok(TeamCommandResult::Complete(success(
                board.scoreboard()?.teams().count() as i32,
                "commands.team.remove.success",
                vec![c(formatted)],
                true,
            )))
        }
        "empty" => {
            let members: Vec<_> = team.members().cloned().collect();
            let count = members.len();
            if count == 0 {
                return Err(error("commands.team.empty.unchanged", &[]).into());
            }
            for member in members {
                board.remove_player_from_team(&member)?;
            }
            Ok(TeamCommandResult::Complete(success(
                count as i32,
                "commands.team.empty.success",
                vec![n(count), c(formatted)],
                true,
            )))
        }
        "join" => {
            for holder in members {
                board.add_player_to_team(&holder.name, name)?;
            }
            Ok(if members.len() == 1 {
                TeamCommandResult::Join {
                    holder: Box::new(members[0].clone()),
                    team: Box::new(formatted),
                }
            } else {
                TeamCommandResult::Complete(success(
                    members.len() as i32,
                    "commands.team.join.success.multiple",
                    vec![n(members.len()), c(formatted)],
                    true,
                ))
            })
        }
        "modify" => modify(board, command, name).map(TeamCommandResult::Complete),
        _ => unreachable!("native team verb"),
    }
}
fn modify(
    board: &mut TeamContext<'_>,
    command: &PreparedCommand,
    name: &str,
) -> TeamResult<super::builtins::CommandResult> {
    let option = command.literals[2].as_str();
    let current = board.scoreboard()?.get_player_team(name).unwrap();
    let (key, extra) = match option {
        "displayName" => {
            let value = component(command, "displayName").unwrap();
            if current.display_name == value {
                return Err(error("commands.team.option.name.unchanged", &[]).into());
            }
            board.update_team(name, |team| team.display_name = value)?;
            ("commands.team.option.name.success".to_owned(), None)
        }
        "color" => {
            let value: Option<TeamColor> = command
                .string("value")
                .map(|value| enum_value(value, super::arguments::scoreboard::COLORS));
            if current.color == value {
                return Err(error("commands.team.option.color.unchanged", &[]).into());
            }
            board.update_team(name, |team| team.color = value)?;
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
                )
                .into());
            }
            board.update_team(name, |team| {
                if option == "friendlyFire" {
                    team.allow_friendly_fire = allowed;
                } else {
                    team.see_friendly_invisibles = allowed;
                }
            })?;
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
                return Err(error(&format!("commands.team.option.{option}.unchanged"), &[]).into());
            }
            board.update_team(name, |team| {
                if option == "nametagVisibility" {
                    team.name_tag_visibility = visibility;
                } else {
                    team.death_message_visibility = visibility;
                }
            })?;
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
                return Err(error("commands.team.option.collisionRule.unchanged", &[]).into());
            }
            board.update_team(name, |team| team.collision_rule = rule)?;
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
            board.update_team(name, |team| {
                if option == "prefix" {
                    team.prefix = value;
                } else {
                    team.suffix = value;
                }
            })?;
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
        .scoreboard()?
        .get_player_team(name)
        .unwrap()
        .formatted_display_name())];
    if let Some(extra) = extra {
        args.push(extra);
    }
    Ok(success(0, &key, args, true))
}

/// A world failure remains a required error, distinct from Brigadier feedback.
#[derive(Debug)]
pub enum TeamError {
    Command(ferraloom_brigadier::CommandSyntaxException),
    Level(crate::level::LevelError),
}
impl From<ferraloom_brigadier::CommandSyntaxException> for TeamError {
    fn from(error: ferraloom_brigadier::CommandSyntaxException) -> Self {
        Self::Command(error)
    }
}
impl From<crate::level::LevelError> for TeamError {
    fn from(error: crate::level::LevelError) -> Self {
        Self::Level(error)
    }
}
impl std::fmt::Display for TeamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command(error) => std::fmt::Display::fmt(error, f),
            Self::Level(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for TeamError {}
type TeamResult<T> = std::result::Result<T, TeamError>;
pub(crate) fn check_scoreboard<T>(result: crate::scoreboard::Result<T>) -> TeamResult<T> {
    checked(result).map_err(Into::into)
}
/// Reads and writes use the same canonical board. Live primitives end that
/// borrow before invoking waypoint callbacks; the command evaluator is shared.
enum TeamContext<'a> {
    Board(&'a mut ServerScoreboard),
    Live(&'a mut crate::level::Level),
}
impl TeamContext<'_> {
    fn scoreboard(&self) -> TeamResult<&crate::scoreboard::Scoreboard> {
        Ok(match self {
            Self::Board(board) => board.scoreboard(),
            Self::Live(level) => level.shared()?.scoreboard.scoreboard(),
        })
    }
    fn add_player_team(&mut self, name: &str) -> TeamResult<()> {
        match self {
            Self::Board(board) => board.add_player_team(name),
            Self::Live(level) => {
                level.shared_mut()?.scoreboard.add_player_team(name);
                level.emit_scoreboard_packets()?;
            }
        }
        Ok(())
    }
    fn update_team(&mut self, name: &str, update: impl FnOnce(&mut PlayerTeam)) -> TeamResult<()> {
        match self {
            Self::Board(board) => check_scoreboard(board.update_team(name, update)),
            Self::Live(level) => level.update_team_live(name, update),
        }
    }
    fn remove_player_team(&mut self, name: &str) -> TeamResult<()> {
        match self {
            Self::Board(board) => check_scoreboard(board.remove_player_team(name)),
            Self::Live(level) => level.remove_team_live(name),
        }
    }
    fn add_player_to_team(&mut self, holder: &str, name: &str) -> TeamResult<bool> {
        match self {
            Self::Board(board) => check_scoreboard(board.add_player_to_team(holder, name)),
            Self::Live(level) => level.add_team_member_live(holder, name),
        }
    }
    fn remove_player_from_team(&mut self, holder: &str) -> TeamResult<bool> {
        match self {
            Self::Board(board) => Ok(board.remove_player_from_team(holder)),
            Self::Live(level) => level.remove_team_member_live(holder),
        }
    }
}
