//! Server-exclusive scoreboard command adapters. Selectors resolve before each native command call.
use super::*;
use ferraloom_game::commands::{
    scoreboard::{self, Holder},
    team, trigger,
};
impl MinecraftServer {
    pub(super) fn scoreboard_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let result = self.execute_scoreboard_command(command);
        // Failed operations may already have changed earlier targets, exactly as vanilla's synchronous loop.
        self.drain_scoreboard_packets();
        result
    }
    fn execute_scoreboard_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        // ScoreboardCommand#addNumberFormats resolves fixed contents before invoking
        // its callback, which then reads the objective and holder arguments.
        let fixed = command.argument("contents").is_some();
        let resolved_fixed;
        let command = if fixed {
            resolved_fixed = self.resolve_scoreboard_components(command)?;
            &resolved_fixed
        } else {
            command
        };
        match command.command.as_str() {
            "scoreboard" => {
                let targets = if command.argument("targets").is_some() {
                    self.score_holders(command, "targets", true)?
                } else if command.argument("target").is_some() {
                    self.score_holders(command, "target", false)?
                } else {
                    Vec::new()
                };
                // Native argument getter order: targets, writable target objective, then sources.
                if command
                    .literals
                    .get(2)
                    .is_some_and(|verb| verb == "operation")
                {
                    scoreboard::objective(
                        self.shared().scoreboard.scoreboard(),
                        command.string("targetObjective").unwrap(),
                        true,
                    )?;
                }
                let sources = if command.argument("source").is_some() {
                    self.score_holders(command, "source", true)?
                } else {
                    Vec::new()
                };
                // Native getObjective precedes these ComponentArgument getters.
                if (command.literals[1] == "objectives"
                    && command.literals[2] == "modify"
                    && command.argument("displayName").is_some())
                    || command.argument("name").is_some()
                {
                    scoreboard::objective(
                        self.shared().scoreboard.scoreboard(),
                        command.string("objective").unwrap(),
                        false,
                    )?;
                }
                let resolved = if fixed {
                    command.clone()
                } else {
                    self.resolve_scoreboard_components(command)?
                };
                scoreboard::execute(
                    &mut self.shared_mut().scoreboard,
                    &resolved,
                    &targets,
                    &sources,
                )
            }
            "team" => {
                // TeamArgument#getTeam precedes either the member selector or getEntityOrException.
                if matches!(command.literals[1].as_str(), "join" | "modify") {
                    let name = command.string("team").unwrap();
                    if self
                        .shared()
                        .scoreboard
                        .scoreboard()
                        .get_player_team(name)
                        .is_none()
                    {
                        return Err(exceptions::error("team.notFound", &[name.to_owned()]));
                    }
                }
                let members =
                    if command.argument("members").is_some() {
                        self.score_holders(command, "members", true)?
                    } else if command.literals[1] == "join" {
                        vec![self.score_holder(command.source.entity.ok_or_else(|| {
                            exceptions::error("permissions.requires.entity", &[])
                        })?)?]
                    } else {
                        Vec::new()
                    };
                let resolved = self.resolve_scoreboard_components(command)?;
                let pending = team::execute_with_feedback(
                    &mut self.shared_mut().scoreboard,
                    &resolved,
                    &members,
                )?;
                // The mutable owner borrow ends before the native sendSuccess supplier reads names.
                pending.resolve_feedback(|holder| match holder.entity {
                    Some(entity) => {
                        let display = self
                            .level
                            .level
                            .entity_display_name_with_scoreboard(
                                entity,
                                self.shared().scoreboard.scoreboard(),
                            )
                            .map_err(scoreboard_entity_error)?;
                        Ok(holder.feedback_with_display(display))
                    }
                    None => Ok(holder.feedback()),
                })
            }
            "trigger" => {
                let entity = command
                    .source
                    .entity
                    .ok_or_else(|| exceptions::error("permissions.requires.player", &[]))?;
                self.player(entity)
                    .map_err(|_| exceptions::error("permissions.requires.player", &[]))?;
                let holder = self.score_holder(entity)?;
                trigger::execute(&mut self.shared_mut().scoreboard, command, &holder)
            }
            _ => unreachable!("scoreboard command root"),
        }
    }
    fn score_holder(
        &self,
        entity: EntityRef,
    ) -> Result<Holder, ferraloom_brigadier::CommandSyntaxException> {
        Ok(Holder {
            entity: Some(entity),
            name: self
                .level
                .level
                .entity_scoreboard_name(entity)
                .map_err(scoreboard_entity_error)?
                .into_owned(),
            display: Some(
                self.level
                    .level
                    .entity_display_name_with_scoreboard(
                        entity,
                        self.shared().scoreboard.scoreboard(),
                    )
                    .map_err(scoreboard_entity_error)?,
            ),
        })
    }
    /// Port of ComponentArgument#getResolvedComponent for these command roots.
    fn resolve_scoreboard_components(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<PreparedCommand, ferraloom_brigadier::CommandSyntaxException> {
        use ferraloom_game::commands::components::ResolutionError;
        scoreboard::resolve_components(command, self).map_err(|error| match error {
            ResolutionError::Syntax(error) => error,
            ResolutionError::EntityData(error) => scoreboard_entity_error(error),
            ResolutionError::Unsupported(kind) => {
                exceptions::error("ferraloom.component.unsupported", &[format!("{kind:?}")])
            }
            ResolutionError::Storage(error) => {
                exceptions::error("ferraloom.command_storage.invalid", &[error.to_string()])
            }
            ResolutionError::InvalidContent(message) => {
                exceptions::error("argument.component.invalid", &[message])
            }
        })
    }

    fn score_holders(
        &mut self,
        command: &PreparedCommand,
        argument: &str,
        wildcard: bool,
    ) -> Result<Vec<Holder>, ferraloom_brigadier::CommandSyntaxException> {
        match command.argument(argument).expect("score holder argument") {
            ArgumentValue::Entity(selector) => {
                let entities = selector.find(&command.source, self, false)?;
                if entities.is_empty() {
                    return Err(exceptions::error("argument.entity.notfound.entity", &[]));
                }
                entities
                    .into_iter()
                    .map(|entity| self.score_holder(entity))
                    .collect()
            }
            ArgumentValue::String(name) if name == "*" => {
                let holders: Vec<_> = if wildcard {
                    self.shared()
                        .scoreboard
                        .scoreboard()
                        .tracked_players()
                        .map(Holder::for_name)
                        .collect()
                } else {
                    Vec::new()
                };
                if holders.is_empty() {
                    return Err(exceptions::error("argument.scoreHolder.empty", &[]));
                }
                Ok(holders)
            }
            ArgumentValue::String(name) => {
                if !name.starts_with('#') {
                    let uuid = ferraloom_game::commands::arguments::selectors::parse_uuid(name);
                    let entities: Vec<_> = self
                        .candidates(false)
                        .into_iter()
                        .filter(|entity| {
                            self.info(*entity).is_some_and(|info| {
                                if let Some(uuid) = uuid {
                                    info.uuid == uuid
                                } else {
                                    info.player && info.name.eq_ignore_ascii_case(name)
                                }
                            })
                        })
                        .collect();
                    if !entities.is_empty() {
                        return entities
                            .into_iter()
                            .map(|entity| self.score_holder(entity))
                            .collect();
                    }
                }
                Ok(vec![Holder::for_name(name)])
            }
            _ => unreachable!("score holder argument type"),
        }
    }
}

fn scoreboard_entity_error(
    error: ferraloom_game::level::LevelError,
) -> ferraloom_brigadier::CommandSyntaxException {
    exceptions::error("ferraloom.scoreboard.entity_read", &[format!("{error:?}")])
}
