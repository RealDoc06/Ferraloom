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
                scoreboard::execute(
                    &mut self.shared_mut().scoreboard,
                    command,
                    &targets,
                    &sources,
                )
            }
            "team" => {
                // TeamArgument#getTeam precedes either the member selector or getEntityOrException.
                if command.literals[1] == "join" {
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
                team::execute(&mut self.shared_mut().scoreboard, command, &members)
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
        let info = self
            .info(entity)
            .ok_or_else(|| exceptions::error("argument.entity.notfound.entity", &[]))?;
        Ok(Holder {
            name: if info.player {
                info.name.to_string()
            } else {
                info.uuid.to_string()
            },
            display: Some(self.display_name(entity)?),
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
