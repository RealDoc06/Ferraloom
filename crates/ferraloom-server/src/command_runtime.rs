//! Exclusive call-site adapters. Game code owns parsing, feedback policy and command behavior.
use super::*;
#[path = "scoreboard_commands.rs"]
mod scoreboard_commands;
use ferraloom_game::{
    commands::{
        PreparedCommand,
        arguments::ArgumentValue,
        builtins::{self, CommandResult},
        exceptions, feedback,
    },
    gamerules::{GameRuleChange, GameRuleEffect, GameRuleMap, GameRules, RuleFeatures},
};
use ferraloom_nbt::Tag;
use ferraloom_storage::{SavedData, SavedDataStorage};
use std::net::IpAddr;

pub(super) fn load_game_rules(world: &World) -> Result<GameRules> {
    let storage = SavedDataStorage::new(world.access.path().join("data"));
    let map = match storage.load::<Tag>(GameRuleMap::STORAGE_ID) {
        Ok(Some(data)) => {
            let result = GameRuleMap::decode_partial(data.value());
            if let Some(error) = result.error {
                eprintln!("Failed to parse saved data for minecraft:game_rules: {error}");
            }
            result.value.unwrap_or_default()
        }
        Ok(None) => GameRuleMap::default(),
        Err(error) => {
            eprintln!("Error loading saved data minecraft:game_rules: {error}");
            GameRuleMap::default()
        }
    };
    Ok(GameRules::new(rule_features(world), map))
}
/// GameRules and command registration share WorldData#enabledFeatures.
pub(super) fn rule_features(world: &World) -> RuleFeatures {
    RuleFeatures {
        minecart_improvements: world
            .level_data
            .data
            .get_list("enabled_features")
            .is_some_and(|values| {
                values.0.iter().any(|value| {
                    value
                        .as_string()
                        .is_some_and(|id| id.to_string_lossy() == "minecraft:minecart_improvements")
                })
            }),
    }
}
impl MinecraftServer {
    /// Publish DedicatedServer's owned spawn-protection inputs to the live level.
    pub(super) fn publish_spawn_protection(&mut self) {
        let spawn = self.default_spawn_data();
        self.level.level.set_spawn_protection(
            ferraloom_game::level::interaction::SpawnProtectionSnapshot::new(
                self.level
                    .dimension
                    .parse()
                    .expect("registered level dimension"),
                spawn
                    .dimension
                    .parse()
                    .expect("validated shared spawn dimension"),
                spawn.position,
                self.properties.int("spawn-protection"),
                self.access.ops.values().map(|op| {
                    ferraloom_game::commands::arguments::selectors::parse_uuid(&op.uuid)
                        .expect("validated operator UUID")
                }),
            ),
        );
    }

    /// Port of MinecraftServer#sendDifficultyUpdate; also used when a new player joins.
    pub(super) fn difficulty_packet(&self) -> cb::Packet {
        let settings = self
            .world
            .level_data
            .data
            .get_compound("difficulty_settings");
        let current = settings
            .and_then(|tag| tag.get_string("difficulty"))
            .map(|s| s.to_string_lossy());
        cb::Packet::ChangeDifficulty(cb::ChangeDifficulty {
            difficulty: difficulty(
                current
                    .as_deref()
                    .unwrap_or_else(|| self.properties.get("difficulty")),
            ),
            locked: settings.is_some_and(|tag| tag.get_boolean_or("locked", false)),
        })
    }
    /// Capture owned save data before constructing IO, keeping the live level's
    /// single-owner lazy caches out of the Send future.
    pub(super) fn save_game_rules(
        &self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let storage = SavedDataStorage::new(self.world.access.path().join("data"));
        let mut data = SavedData::new(self.game_rules.map().clone());
        let mut clocks = SavedData::new(self.clocks.states.clone());
        let mut scoreboard = SavedData::new(ferraloom_game::scoreboard::ScoreboardSaveData(
            self.shared().scoreboard.scoreboard().clone(),
        ));
        async move {
            tokio::task::spawn_blocking(move || {
                storage.save(GameRuleMap::STORAGE_ID, &mut data)?;
                storage.save(
                    ferraloom_game::scoreboard::ScoreboardSaveData::STORAGE_ID,
                    &mut scoreboard,
                )?;
                storage.save(PackedClockStates::STORAGE_ID, &mut clocks)
            })
            .await??;
            Ok(())
        }
    }
    /// Capture dirty storage before constructing Send IO, as for rules/clocks.
    pub(super) fn save_command_storage(
        &mut self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let storage = SavedDataStorage::new(self.world.access.path().join("data"));
        let dirty = self.command_storage.take_dirty();
        async move {
            tokio::task::spawn_blocking(move || {
                for (id, mut data) in dirty {
                    storage.save(&id, &mut data)?;
                }
                Ok::<_, ferraloom_storage::Error>(())
            })
            .await??;
            Ok(())
        }
    }
    pub(super) fn on_game_rule_changed(&mut self, change: GameRuleChange) {
        self.game_rules
            .update_level_rules(self.level.level.rules_mut());
        match change.effect {
            Some(GameRuleEffect::ReducedDebugInfo { event }) => {
                let players = self.players.clone();
                for connection in players {
                    let Phase::Play(entity) = self.connections[&connection].phase else {
                        continue;
                    };
                    let Ok(player) = self.player(entity) else {
                        continue;
                    };
                    let network_id = player.network_id;
                    self.connections
                        .get_mut(&connection)
                        .unwrap()
                        .push(cb::Packet::EntityEvent(cb::EntityEvent {
                            entity_id: network_id,
                            event_id: event as i8,
                        }));
                }
            }
            Some(GameRuleEffect::GameEvent { event, enabled }) => {
                self.broadcast(cb::Packet::GameEvent(cb::GameEvent {
                    event,
                    param: if enabled { 1. } else { 0. },
                }))
            }
            Some(GameRuleEffect::SynchronizeClocks) => self.broadcast(self.time_packet()),
            // Waypoint connections and mob spawning have no live content in this foundation.
            Some(GameRuleEffect::LocatorBar { .. } | GameRuleEffect::UpdateMobSpawningFlags)
            | None => {}
        }
    }
    pub(super) fn send_component(&mut self, source: Option<ConnectionId>, content: Component) {
        if let Some(source) = source {
            if let Some(c) = self.connections.get_mut(&source) {
                c.push(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                    content,
                    overlay: false,
                })));
            }
        } else {
            println!("{}", exceptions::plain_text(&content));
        }
    }
    pub(super) fn command_failure(
        &mut self,
        source: Option<ConnectionId>,
        stack: &CommandSourceStack,
        message: Component,
    ) {
        if stack.accepts_failure && !stack.silent {
            self.send_component(source, feedback::failure_message(message));
        }
    }
    pub(super) fn command_syntax_failure(
        &mut self,
        source: Option<ConnectionId>,
        stack: &CommandSourceStack,
        error: &ferraloom_brigadier::CommandSyntaxException,
        text: &str,
    ) {
        self.command_failure(source, stack, exceptions::error_component(error));
        if let Some(context) =
            exceptions::syntax_context(error, Commands::trim_optional_prefix(text))
        {
            self.command_failure(source, stack, context);
        }
    }
    pub(super) fn command_success(
        &mut self,
        source: Option<ConnectionId>,
        stack: &CommandSourceStack,
        message: Component,
        broadcast: bool,
    ) {
        let plan = feedback::success_plan(stack, &self.game_rules, broadcast);
        if !plan.evaluate_message {
            return;
        }
        if plan.source {
            self.send_component(source, message.clone());
        }
        if !plan.operators && !plan.server_log {
            return;
        }
        let name = stack.display_name(|entity| {
            self.player(entity)
                .map_or_else(|_| Component::literal(""), ServerPlayer::display_name)
        });
        let admin = feedback::admin_message(name, message);
        if plan.operators {
            let operators = self
                .players
                .iter()
                .copied()
                .filter(|id| {
                    Some(*id) != source
                        && self.access.ops.values().any(|op| {
                            op.uuid
                                .eq_ignore_ascii_case(&self.connections[id].profile.id.to_string())
                        })
                })
                .collect::<Vec<_>>();
            for operator in operators {
                self.send_component(Some(operator), admin.clone());
            }
        }
        if plan.server_log {
            self.send_component(None, admin);
        }
    }
    pub(super) fn apply_command_result(
        &mut self,
        source: Option<ConnectionId>,
        stack: &CommandSourceStack,
        result: CommandResult,
    ) {
        if let Some(change) = result.rule_change {
            self.on_game_rule_changed(change);
        }
        for feedback in result.feedback {
            self.command_success(source, stack, feedback.message, feedback.broadcast);
        }
        stack.callback.on_success(result.value);
    }
    /// Additional built-ins execute synchronously, using the same live server state as packet commands.
    pub(super) fn run_builtin(
        &mut self,
        source: Option<ConnectionId>,
        command: PreparedCommand,
    ) -> Result<(), ferraloom_brigadier::CommandSyntaxException> {
        let stack = &command.source;
        let result = match command.command.as_str() {
            "gamerule" => builtins::gamerule(&mut self.game_rules, &command)?,
            "scoreboard" | "team" | "trigger" => self.scoreboard_command(&command)?,
            "op" | "deop" => self.operator_command(source, &command)?,
            "kick" => self.kick_command(source, &command)?,
            "whitelist" => self.whitelist_command(source, &command)?,
            "ban" | "pardon" => self.ban_player_command(source, &command)?,
            "ban-ip" | "pardon-ip" => self.ban_ip_command(source, &command)?,
            "banlist" => self.ban_list_command(&command),
            "gamemode" => self.game_mode_command(source, &command)?,
            "setworldspawn" | "spawnpoint" => self.spawn_command(source, &command)?,
            "seed" => builtins::seed(self.world.seed),
            "help" => self.commands.help(&command)?,
            "say" | "me" | "msg" => self.message_command(source, &command)?,
            "teammsg" => self.message_command(source, &command)?,
            "list" => {
                let uuids = command.literals.iter().any(|literal| literal == "uuids");
                let names = self
                    .players
                    .iter()
                    .filter_map(|id| {
                        let Phase::Play(entity) = self.connections[id].phase else {
                            return None;
                        };
                        let player = self.player(entity).ok()?;
                        Some(if uuids {
                            Component::translatable(
                                "commands.list.nameAndId",
                                vec![
                                    builtins::component(Component::literal(&player.name)),
                                    builtins::string(player.uuid.to_string()),
                                ],
                            )
                        } else {
                            player.display_name()
                        })
                    })
                    .collect();
                builtins::list_players(names, self.properties.int("max-players"))
            }
            "difficulty" => {
                let current = self
                    .world
                    .level_data
                    .data
                    .get_compound("difficulty_settings")
                    .and_then(|tag| tag.get_string("difficulty"))
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_else(|| self.properties.get("difficulty").to_owned());
                if let Some(requested) = command.literals.get(1) {
                    if *requested == current {
                        return Err(exceptions::error_with_arguments(
                            "commands.difficulty.failure",
                            vec![builtins::component(Component::translatable(
                                format!("options.difficulty.{current}"),
                                vec![],
                            ))],
                        ));
                    }
                    let mut settings = self
                        .world
                        .level_data
                        .data
                        .get_compound("difficulty_settings")
                        .cloned()
                        .unwrap_or_default();
                    let changed = if settings
                        .get_boolean_or("hardcore", self.properties.boolean("hardcore"))
                    {
                        "hard"
                    } else {
                        requested.as_str()
                    };
                    let locked = settings.get_boolean_or("locked", false);
                    settings.insert("difficulty", Tag::String(changed.into()));
                    self.world
                        .level_data
                        .data
                        .insert("difficulty_settings", Tag::Compound(settings));
                    self.level.level.set_difficulty(difficulty(changed));
                    self.broadcast(cb::Packet::ChangeDifficulty(cb::ChangeDifficulty {
                        difficulty: difficulty(changed),
                        locked,
                    }));
                    CommandResult::success(
                        0,
                        Component::translatable(
                            "commands.difficulty.success",
                            vec![builtins::component(Component::translatable(
                                format!("options.difficulty.{requested}"),
                                vec![],
                            ))],
                        ),
                        true,
                    )
                } else {
                    CommandResult::success(
                        difficulty_id(&current),
                        Component::translatable(
                            "commands.difficulty.query",
                            vec![builtins::component(Component::translatable(
                                format!("options.difficulty.{current}"),
                                vec![],
                            ))],
                        ),
                        false,
                    )
                }
            }
            "save-off" | "save-on" => {
                let enabled = command.command == "save-on";
                if self.saving_enabled == enabled {
                    return Err(exceptions::error(
                        if enabled {
                            "commands.save.alreadyOn"
                        } else {
                            "commands.save.alreadyOff"
                        },
                        &[],
                    ));
                }
                self.saving_enabled = enabled;
                CommandResult::success(
                    1,
                    Component::translatable(
                        if enabled {
                            "commands.save.enabled"
                        } else {
                            "commands.save.disabled"
                        },
                        vec![],
                    ),
                    true,
                )
            }
            "setidletimeout" => {
                let minutes = command.integer("minutes").expect("parsed minutes");
                self.idle_timeout = minutes;
                self.update_command_property("player-idle-timeout", minutes);
                let (key, args) = if minutes > 0 {
                    (
                        "commands.setidletimeout.success",
                        vec![builtins::integer(minutes)],
                    )
                } else {
                    ("commands.setidletimeout.success.disabled", vec![])
                };
                CommandResult::success(minutes, Component::translatable(key, args), true)
            }
            "defaultgamemode" => {
                let Some(ferraloom_game::commands::arguments::ArgumentValue::GameMode(mode)) =
                    command.argument("gamemode")
                else {
                    unreachable!()
                };
                self.default_game_mode = *mode;
                self.world
                    .level_data
                    .data
                    .insert("GameType", Tag::Int(mode.id()));
                let mut count = 0;
                if self.properties.boolean("force-gamemode") {
                    for player in self.players.clone() {
                        count += i32::from(self.change_player_game_mode(player, *mode).is_some());
                    }
                }
                CommandResult::success(
                    count,
                    Component::translatable(
                        "commands.defaultgamemode.success",
                        vec![builtins::component(Component::translatable(
                            format!("gameMode.{}", game_mode_name(*mode)),
                            vec![],
                        ))],
                    ),
                    true,
                )
            }
            _ => {
                return Err(exceptions::error(
                    "ferraloom.command.pending",
                    std::slice::from_ref(&command.command),
                ));
            }
        };
        self.apply_command_result(source, stack, result);
        Ok(())
    }
}
fn difficulty(value: &str) -> play::Difficulty {
    match value {
        "peaceful" => play::Difficulty::Peaceful,
        "normal" => play::Difficulty::Normal,
        "hard" => play::Difficulty::Hard,
        _ => play::Difficulty::Easy,
    }
}
fn difficulty_id(value: &str) -> i32 {
    match value {
        "peaceful" => 0,
        "normal" => 2,
        "hard" => 3,
        _ => 1,
    }
}

fn game_mode_name(mode: GameType) -> &'static str {
    match mode {
        GameType::Survival => "survival",
        GameType::Creative => "creative",
        GameType::Adventure => "adventure",
        GameType::Spectator => "spectator",
    }
}

use ferraloom_game::commands::arguments::selectors::{
    EntityQuery, SelectorEntity, SelectorPredicate, selector_read_error,
};
impl EntityQuery for MinecraftServer {
    fn candidates(&self, players_only: bool) -> Vec<EntityRef> {
        if !players_only {
            return self.level.level.all_entities();
        }
        self.players
            .iter()
            .filter_map(|id| match self.connections[id].phase {
                Phase::Play(entity) => Some(entity),
                _ => None,
            })
            .collect()
    }
    fn candidates_in(
        &self,
        players_only: bool,
        bounds: Option<ferraloom_util::Aabb>,
    ) -> Vec<EntityRef> {
        if players_only {
            return self.candidates(true);
        }
        bounds.map_or_else(
            || self.level.level.all_entities(),
            |bounds| self.level.level.entities_in(bounds, |_, _| true),
        )
    }
    fn entity_by_uuid(&self, uuid: Uuid, players_only: bool) -> Option<EntityRef> {
        if players_only {
            self.candidates(true)
                .into_iter()
                .find(|entity| self.player(*entity).is_ok_and(|player| player.uuid == uuid))
        } else {
            self.level.level.entity_by_uuid(uuid)
        }
    }
    fn players_in_level(&self, dimension: &str) -> Vec<EntityRef> {
        if dimension != "minecraft:overworld" {
            return Vec::new();
        }
        // Players are always tracked; the canonical lookup retains level join order.
        self.level
            .level
            .all_entities()
            .into_iter()
            .filter(|entity| self.player(*entity).is_ok())
            .collect()
    }
    fn bounding_box(&self, entity: EntityRef) -> Option<ferraloom_util::Aabb> {
        self.level
            .level
            .entity_state(entity)
            .ok()
            .map(|state| state.as_entity().bounding_box)
    }
    fn validate_candidate(
        &self,
        entity: EntityRef,
    ) -> std::result::Result<(), ferraloom_brigadier::CommandSyntaxException> {
        self.level
            .level
            .entity_plain_text_name(entity)
            .map(|_| ())
            .map_err(selector_read_error)
    }
    fn info(&self, entity: EntityRef) -> Option<SelectorEntity<'_>> {
        let state = self.level.level.entity_state(entity).ok()?;
        let base = state.as_entity();
        let player = matches!(state, ferraloom_game::entity::EntityState::Player(_));
        let name = if let ferraloom_game::entity::EntityState::Player(player) = state {
            std::borrow::Cow::Borrowed(player.name.as_str())
        } else {
            std::borrow::Cow::Owned(self.level.level.entity_plain_text_name(entity).ok()?)
        };
        Some(SelectorEntity {
            name,
            uuid: base.uuid,
            player,
            dimension: &self.level.dimension,
            position: base.position,
            width: f64::from(base.dimensions.width),
            height: f64::from(base.dimensions.height),
        })
    }
    fn display_name(
        &self,
        entity: EntityRef,
    ) -> std::result::Result<Component, ferraloom_brigadier::CommandSyntaxException> {
        self.level
            .level
            .entity_display_name_with_scoreboard(entity, self.shared().scoreboard.scoreboard())
            .map_err(selector_read_error)
    }
    fn matches(
        &mut self,
        entity: EntityRef,
        predicate: &SelectorPredicate,
    ) -> Result<bool, ferraloom_brigadier::CommandSyntaxException> {
        ferraloom_game::commands::arguments::selectors::live_entity_matches(
            &self.level.level,
            self.shared().scoreboard.scoreboard(),
            &self.command_entity_tags,
            entity,
            predicate,
        )
    }
    fn shuffle(&mut self, entities: &mut [(EntityRef, f64)]) {
        // Collections.shuffle retains one java.util.Random; it does not consume Level#random.
        let random = self
            .selector_random
            .get_or_insert_with(|| ferraloom_util::random::LegacyRandomSource::new(rand::random()));
        ferraloom_game::commands::arguments::selectors::shuffle_with_random(entities, random);
    }
}

impl MinecraftServer {
    /// Command failures produce feedback; only ingress violations propagate to disconnect logic.
    pub(super) async fn execute_command(
        &mut self,
        source: Option<ConnectionId>,
        text: &str,
    ) -> Result<()> {
        let mut stack = CommandSourceStack::server(4, Vec3::ZERO);
        if let Some(connection) = source {
            let c = self
                .connections
                .get(&connection)
                .context("unknown command source")?;
            if let Phase::Play(entity) = c.phase {
                stack = CommandSourceStack::server(
                    self.access.permission(c.profile.id),
                    self.player(entity)?.position,
                )
                .with_entity(entity)
                .with_source(
                    ferraloom_game::commands::source::CommandSource::Player(entity),
                );
            }
        }
        let lent = self.lend_server_shared();
        let result = self.execute_command_inner(source, text).await;
        self.return_server_shared(lent);
        if let Some(connection) = source
            && let Some(c) = self.connections.get_mut(&connection)
        {
            c.command_message = None;
        }
        if let Err(error) = result {
            if error.to_string() == "disconnect.spam"
                || error.is::<super::command_level::CommandLevelFailure>()
            {
                return Err(error);
            }
            if let Some(syntax) =
                error.downcast_ref::<ferraloom_brigadier::CommandSyntaxException>()
            {
                self.command_syntax_failure(source, &stack, syntax, text);
            } else {
                eprintln!("Command failed: {error:#}");
                self.command_failure(
                    source,
                    &stack,
                    Component::translatable("command.failed", vec![]),
                );
            }
        }
        Ok(())
    }
}

use ferraloom_game::commands::{
    clocks::{PackedClockStates, ServerClockManager},
    context::CommandBuildContext,
};
pub(super) fn load_clocks(
    world: &World,
    resources: &ResourceManager,
) -> Result<ServerClockManager> {
    let storage = SavedDataStorage::new(world.access.path().join("data"));
    let states = storage
        .load::<PackedClockStates>(PackedClockStates::STORAGE_ID)?
        .map(|data| data.value().clone())
        .unwrap_or_default();
    ServerClockManager::load(resources, states).map_err(|error| anyhow::anyhow!("{error}"))
}
/// Capture whole ticks from the sole mutable server clock manager. Loaded clock
/// keys come from validated resource identifiers; saved unknown keys are discarded.
pub(super) fn environment_clock_snapshot(
    clocks: &ServerClockManager,
) -> ferraloom_game::level::environment::ClockSnapshot {
    ferraloom_game::level::environment::ClockSnapshot::new(clocks.states.0.iter().map(
        |(id, state)| {
            (
                id.parse().expect("registered world-clock identifier"),
                state.total_ticks,
            )
        },
    ))
}
pub(super) fn command_build_context(
    registries: &RegistrySet,
    _resources: &ResourceManager,
    features: RuleFeatures,
) -> CommandBuildContext {
    let mut context = CommandBuildContext {
        features,
        registries: registries.entries.clone(),
        tags: BTreeMap::new(),
    };
    // Share the resolved registry/tag view sent to clients, including dynamic
    // registries such as biomes; reparsing a few built-in tag families loses them.
    use ferraloom_protocol::codec::RegistryContext;
    for registry in &registries.tags.tags {
        context.tags.insert(
            registry.registry.to_string(),
            registry
                .tags
                .iter()
                .map(|tag| {
                    (
                        tag.name.to_string(),
                        tag.ids
                            .iter()
                            .map(|id| {
                                registries
                                    .entry_key(&registry.registry, id.0)
                                    .expect("resolved registry tag contains a registered entry")
                                    .to_string()
                            })
                            .collect(),
                    )
                })
                .collect(),
        );
    }
    context
}
impl MinecraftServer {
    pub(super) fn clock_packet(&self, only: Option<&str>) -> cb::Packet {
        let advance = self
            .game_rules
            .boolean(ferraloom_game::gamerules::GameRule::AdvanceTime);
        cb::Packet::SetTime(cb::SetTime {
            game_time: self.level.game_time,
            clock_updates: self
                .clocks
                .states
                .0
                .iter()
                .filter(|(id, _)| only.is_none_or(|only| id.as_str() == only))
                .map(|(id, state)| ClockUpdate {
                    clock: WorldClockId(self.registries.numeric("world_clock", id)),
                    state: state.network(advance),
                })
                .collect(),
        })
    }
}

impl MinecraftServer {
    fn message_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        use ferraloom_game::commands::arguments::ArgumentValue;
        let argument = if command.command == "me" {
            "action"
        } else {
            "message"
        };
        let team_receivers = if command.command == "teammsg" {
            let entity = command
                .source
                .entity
                .ok_or_else(|| exceptions::error("permissions.requires.entity", &[]))?;
            let player = self
                .player(entity)
                .map_err(|_| exceptions::error("ferraloom.selector.pending", &[]))?;
            let scoreboard = self.shared().scoreboard.scoreboard();
            let team = scoreboard
                .get_players_team(&player.name)
                .ok_or_else(|| exceptions::error("commands.teammsg.failed.noteam", &[]))?;
            let team_name = team.name().to_owned();
            let target =
                ferraloom_game::commands::message::team_chat_target(team.formatted_display_name());
            let receivers = self
                .players
                .iter()
                .copied()
                .filter(|id| {
                    let Phase::Play(receiver) = self.connections[id].phase else {
                        return false;
                    };
                    receiver == entity
                        || self
                            .player(receiver)
                            .ok()
                            .and_then(|player| scoreboard.get_players_team(&player.name))
                            .is_some_and(|team| team.name() == team_name)
                })
                .collect::<Vec<_>>();
            Some((entity, receivers, target))
        } else {
            None
        };
        let targets = if command.command == "msg" {
            let Some(ArgumentValue::Entity(selector)) = command.argument("targets") else {
                unreachable!()
            };
            let targets = selector.find(&command.source, self, true)?;
            if targets.is_empty() {
                return Err(exceptions::error("argument.entity.notfound.player", &[]));
            }
            Some(targets)
        } else {
            None
        };
        let Some(ArgumentValue::Message(message)) = command.argument(argument) else {
            unreachable!()
        };
        let content = message.to_component(&command.source, self)?;
        // Signing context belongs to the owned source. Reading it never consumes
        // the argument; forks and source transformations reuse the same message.
        let signed = command
            .source
            .signing_context
            .argument(argument)
            .filter(|message| message.sender != Uuid::NIL)
            .map(|message| ValidatedMessage {
                sender: message.sender,
                index: message.index,
                signature: message.signature.clone(),
                content: message.content.clone(),
                timestamp: message.timestamp,
                salt: message.salt,
                last_seen: message.last_seen.to_vec(),
            });
        let name = &command.source;
        let name = match &name.names {
            ferraloom_game::commands::source::NamesProvider::Constant(name) => {
                name.as_ref().clone()
            }
            ferraloom_game::commands::source::NamesProvider::Entity => match name.entity {
                Some(entity) => self.display_name(entity)?,
                None => Component::default(),
            },
        };
        let count = if let Some((entity, receivers, target)) = team_receivers {
            for receiver in &receivers {
                let outgoing = matches!(self.connections[receiver].phase, Phase::Play(handle) if handle == entity);
                self.send_command_chat_to(
                    *receiver,
                    signed.as_ref(),
                    &content,
                    ChatTypeBound {
                        chat_type: ChatTypeHolder(Holder::Reference(id(if outgoing {
                            "team_msg_command_outgoing"
                        } else {
                            "team_msg_command_incoming"
                        }))),
                        name: name.clone(),
                        target_name: Some(target.clone()),
                    },
                );
            }
            receivers.len() as i32
        } else if let Some(targets) = targets {
            for entity in &targets {
                let target_name = self.display_name(*entity)?;
                let target=self.players.iter().copied().find(|id|matches!(self.connections[id].phase,Phase::Play(handle) if handle==*entity)).unwrap();
                let outgoing = ChatTypeBound {
                    chat_type: ChatTypeHolder(Holder::Reference(id("msg_command_outgoing"))),
                    name: name.clone(),
                    target_name: Some(target_name.clone()),
                };
                if let Some(source) = source {
                    if !command.source.silent {
                        self.send_command_chat_to(source, signed.as_ref(), &content, outgoing);
                    }
                } else if !command.source.silent {
                    let mut decorated = Component::translatable(
                        "commands.message.display.outgoing",
                        vec![
                            builtins::component(target_name),
                            builtins::component(content.clone()),
                        ],
                    );
                    decorated.style.color = Some(protocol::chat::TextColor::parse("gray").unwrap());
                    decorated.style.italic = Some(true);
                    self.send_component(None, decorated);
                }
                self.send_command_chat_to(
                    target,
                    signed.as_ref(),
                    &content,
                    ChatTypeBound {
                        chat_type: ChatTypeHolder(Holder::Reference(id("msg_command_incoming"))),
                        name: name.clone(),
                        target_name: None,
                    },
                );
            }
            targets.len() as i32
        } else {
            use ferraloom_game::commands::message::BroadcastChatType;
            let chat_type = if command.command == "me" {
                BroadcastChatType::Emote
            } else {
                BroadcastChatType::Say
            };
            println!(
                "{}",
                chat_type.console_line(
                    name.clone(),
                    content.clone(),
                    signed
                        .as_ref()
                        .filter(|message| message.signature.is_some())
                        .map(|message| message.timestamp),
                    time::OffsetDateTime::now_utc().unix_timestamp_nanos(),
                )
            );
            let receivers = self.players.clone();
            for receiver in receivers {
                self.send_command_chat_to(
                    receiver,
                    signed.as_ref(),
                    &content,
                    ChatTypeBound {
                        chat_type: ChatTypeHolder(Holder::Reference(id(chat_type.registry_key()))),
                        name: name.clone(),
                        target_name: None,
                    },
                );
            }
            1
        };
        Ok(CommandResult {
            value: count,
            feedback: vec![],
            rule_change: None,
        })
    }
    fn send_command_chat_to(
        &mut self,
        receiver: ConnectionId,
        signed: Option<&ValidatedMessage>,
        content: &Component,
        chat_type: ChatTypeBound,
    ) {
        let Some(c) = self.connections.get_mut(&receiver) else {
            return;
        };
        if c.close
            || !matches!(c.phase, Phase::Play(_))
            || c.information.chat_visibility != common::serverbound::ChatVisibility::Full
        {
            return;
        }
        if let Some(message) = signed {
            if let Some(signature) = &message.signature
                && let Err(error) = c.last_seen.add_pending(signature)
            {
                c.disconnect(error.to_string());
                return;
            }
            let global_index = c.next_chat_index;
            c.next_chat_index += 1;
            let body = c.signature_cache.pack(message);
            c.push(cb::Packet::PlayerChat(Box::new(cb::PlayerChat {
                global_index: VarInt(global_index),
                sender: message.sender,
                index: VarInt(message.index),
                signature: message.signature.clone(),
                body,
                unsigned_content: (*content != Component::literal(&message.content))
                    .then(|| content.clone()),
                filter_mask: FilterMask::PassThrough,
                chat_type,
            })));
        } else {
            c.push(cb::Packet::DisguisedChat(Box::new(cb::DisguisedChat {
                message: content.clone(),
                chat_type,
            })));
        }
    }
}

impl MinecraftServer {
    fn command_suggestion_data(&self) -> ferraloom_game::commands::suggestions::SuggestionData {
        let mut data = ferraloom_game::commands::suggestions::SuggestionData {
            operators: self
                .access
                .ops
                .values()
                .map(|entry| entry.name.clone())
                .collect(),
            whitelist: self
                .access
                .whitelist
                .values()
                .map(|entry| entry.name.clone())
                .collect(),
            banned_players: self
                .access
                .banned_players
                .values()
                .filter_map(|entry| entry.name.clone())
                .collect(),
            banned_ips: self
                .access
                .banned_ips
                .values()
                .filter_map(|entry| entry.ip.clone())
                .collect(),
            ..Default::default()
        };
        data.default_clocks
            .insert("minecraft:overworld".into(), "minecraft:overworld".into());
        for clock in self.clocks.states.0.keys() {
            data.clock_markers.insert(
                clock.clone(),
                self.clocks.time_markers(clock).map(str::to_owned).collect(),
            );
            data.clock_timelines.insert(
                clock.clone(),
                self.clocks.timelines(clock).map(str::to_owned).collect(),
            );
        }
        for connection in &self.players {
            let Phase::Play(entity) = self.connections[connection].phase else {
                continue;
            };
            let Ok(player) = self.player(entity) else {
                continue;
            };
            data.players.push(player.name.clone());
            let uuid = player.uuid.to_string();
            if !self
                .access
                .ops
                .values()
                .any(|op| op.uuid.eq_ignore_ascii_case(&uuid))
            {
                data.op_candidates.push(player.name.clone());
            }
            if !self
                .access
                .whitelist
                .values()
                .any(|entry| entry.uuid.eq_ignore_ascii_case(&uuid))
            {
                data.whitelist_candidates.push(player.name.clone());
            }
        }
        data
    }
    pub(super) fn process_command_suggestions(&mut self) -> Result<()> {
        let players = self.players.clone();
        for connection in players {
            let Some(request) = self
                .connections
                .get_mut(&connection)
                .and_then(|c| c.command_suggestion.take())
            else {
                continue;
            };
            let Phase::Play(entity) = self.connections[&connection].phase else {
                continue;
            };
            let player = self.player(entity)?;
            let source =
                CommandSourceStack::server(self.access.permission(player.uuid), player.position)
                    .with_entity(entity);
            let suggestions = self.commands.suggestions_with(
                &request.command.0,
                source,
                &self.command_suggestion_data(),
            );
            let entries = suggestions
                .list
                .into_iter()
                .map(|suggestion| {
                    let tooltip = suggestion.tooltip.map(|message| {
                        (message.as_ref() as &dyn std::any::Any)
                            .downcast_ref::<exceptions::TranslatedMessage>()
                            .map_or_else(
                                || Component::literal(message.text()),
                                |translated| translated.component.clone(),
                            )
                    });
                    play::CommandSuggestion {
                        text: suggestion.text,
                        tooltip,
                    }
                })
                .collect();
            self.connections
                .get_mut(&connection)
                .unwrap()
                .push(cb::Packet::CommandSuggestions(cb::CommandSuggestions {
                    id: request.id,
                    start: VarInt(suggestions.range.start as i32),
                    length: VarInt(suggestions.range.length() as i32),
                    suggestions: entries,
                }));
        }
        Ok(())
    }
}

impl MinecraftServer {
    fn selected_connections(
        &mut self,
        command: &PreparedCommand,
        argument: &str,
    ) -> Result<Vec<ConnectionId>, ferraloom_brigadier::CommandSyntaxException> {
        let Some(ArgumentValue::Entity(selector)) = command.argument(argument) else {
            return Err(exceptions::error("argument.entity.notfound.player", &[]));
        };
        let handles = selector.find(&command.source, self, true)?;
        let connections = handles.into_iter().filter_map(|entity| {
            self.players.iter().copied().find(|id| matches!(self.connections[id].phase, Phase::Play(handle) if handle == entity))
        }).collect::<Vec<_>>();
        if connections.is_empty() {
            return Err(exceptions::error("argument.entity.notfound.player", &[]));
        }
        Ok(connections)
    }
    /// GameProfileArgument resolves names or selectors at the command's execution step.
    fn command_profiles(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<Vec<GameProfile>, ferraloom_brigadier::CommandSyntaxException> {
        if matches!(command.argument("targets"), Some(ArgumentValue::Entity(_))) {
            return self.selected_connections(command, "targets").map(|ids| {
                ids.into_iter()
                    .map(|id| self.connections[&id].profile.clone())
                    .collect()
            });
        }
        let name = command.string("targets").expect("game profile argument");
        if let Some(profile) = self
            .players
            .iter()
            .map(|id| &self.connections[id].profile)
            .find(|profile| profile.name.eq_ignore_ascii_case(name))
        {
            return Ok(vec![profile.clone()]);
        }
        if let Some(profile) = self.access.cached_profile(name) {
            return Ok(vec![profile]);
        }
        if !self.properties.boolean("online-mode") {
            // CachedUserNameToIdResolver#get lowercases before querying its offline fallback.
            let name = name.to_lowercase();
            let profile = GameProfile {
                id: Uuid::offline_player(&name),
                name,
                properties: Vec::new(),
            };
            if let Err(error) = self.access.remember(&profile) {
                eprintln!("Failed to save profile cache: {error:#}");
            }
            return Ok(vec![profile]);
        }
        Err(exceptions::error("argument.player.unknown", &[]))
    }
    fn update_command_permissions(&mut self, profile: Uuid) {
        let level = self.access.permission(profile);
        let targets = self
            .players
            .iter()
            .copied()
            .filter(|id| self.connections[id].profile.id == profile)
            .collect::<Vec<_>>();
        for id in targets {
            let Phase::Play(entity) = self.connections[&id].phase else {
                continue;
            };
            let Ok(player) = self.player(entity) else {
                continue;
            };
            let network_id = player.network_id;
            let tree = self.commands.packet(level);
            let connection = self.connections.get_mut(&id).unwrap();
            connection.push(cb::Packet::EntityEvent(cb::EntityEvent {
                entity_id: network_id,
                event_id: (24 + level.clamp(0, 4)) as i8,
            }));
            connection.push(tree);
        }
    }
    fn kick_unlisted_players(&mut self) {
        if !self.properties.boolean("enforce-whitelist") || !self.properties.boolean("white-list") {
            return;
        }
        let targets = self
            .players
            .iter()
            .copied()
            .filter(|id| {
                let uuid = self.connections[id].profile.id.to_string();
                !self.access.ops.contains_key(&uuid) && !self.access.whitelist.contains_key(&uuid)
            })
            .collect::<Vec<_>>();
        for id in targets {
            self.connections
                .get_mut(&id)
                .unwrap()
                .disconnect_component(Component::translatable(
                    "multiplayer.disconnect.not_whitelisted",
                    vec![],
                ));
        }
    }
    fn operator_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let profiles = self.command_profiles(command)?;
        let grant = command.command == "op";
        let mut count = 0;
        for profile in profiles {
            if self.access.ops.contains_key(&profile.id.to_string()) == grant {
                continue;
            }
            let level = grant.then(|| self.properties.int("op-permission-level"));
            if let Err(error) = self.access.set_operator(&profile, level) {
                eprintln!("Could not save operator list: {error:#}");
            }
            self.publish_spawn_protection();
            self.update_command_permissions(profile.id);
            count += 1;
            self.command_success(
                source,
                &command.source,
                Component::translatable(
                    format!("commands.{}.success", command.command),
                    vec![builtins::string(&profile.name)],
                ),
                true,
            );
        }
        if count == 0 {
            return Err(exceptions::error(
                &format!("commands.{}.failed", command.command),
                &[],
            ));
        }
        if !grant {
            self.kick_unlisted_players();
        }
        Ok(CommandResult {
            value: count,
            feedback: Vec::new(),
            rule_change: None,
        })
    }
    fn kick_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let targets = self.selected_connections(command, "targets")?;
        let reason = if let Some(ArgumentValue::Message(message)) = command.argument("reason") {
            message.to_component(&command.source, self)?
        } else {
            Component::translatable("multiplayer.disconnect.kicked", vec![])
        };
        for id in &targets {
            let name = if let Phase::Play(entity) = self.connections[id].phase {
                self.player(entity)
                    .map_or_else(|_| Component::literal(""), ServerPlayer::display_name)
            } else {
                continue;
            };
            self.connections
                .get_mut(id)
                .unwrap()
                .disconnect_component(reason.clone());
            self.command_success(
                source,
                &command.source,
                Component::translatable(
                    "commands.kick.success",
                    vec![
                        builtins::component(name),
                        builtins::component(reason.clone()),
                    ],
                ),
                true,
            );
        }
        Ok(CommandResult {
            value: targets.len() as i32,
            feedback: Vec::new(),
            rule_change: None,
        })
    }
}

impl MinecraftServer {
    /// Port of ServerPlayer#setGameMode's currently available player state and packets.
    fn change_player_game_mode(
        &mut self,
        id: ConnectionId,
        mode: GameType,
    ) -> Option<(EntityRef, Component)> {
        let Phase::Play(entity) = self.connections[&id].phase else {
            return None;
        };
        let Ok(player) = self.level.level.entity_mut::<ServerPlayer>(entity) else {
            return None;
        };
        if player.game_mode == mode {
            return None;
        }
        player.game_mode = mode;
        player.update_game_mode_abilities();
        let packet = abilities(player);
        let name = player.display_name();
        let uuid = player.uuid;
        let c = self.connections.get_mut(&id).unwrap();
        c.push(cb::Packet::GameEvent(cb::GameEvent {
            event: 3,
            param: mode.id() as f32,
        }));
        c.push(packet);
        self.broadcast(cb::Packet::PlayerInfoUpdate(cb::PlayerInfoUpdate {
            actions: 4,
            entries: vec![PlayerInfoEntry {
                profile_id: uuid,
                actions: vec![PlayerInfoAction::UpdateGameMode {
                    game_mode: protocol_game_type(mode),
                }],
            }],
        }));
        Some((entity, name))
    }
    fn game_mode_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let Some(ArgumentValue::GameMode(mode)) = command.argument("gamemode") else {
            unreachable!()
        };
        let mode = *mode;
        let targets = if command.argument("target").is_some() {
            self.selected_connections(command, "target")?
        } else {
            vec![source.ok_or_else(|| exceptions::error("permissions.requires.player", &[]))?]
        };
        let mode_name =
            Component::translatable(format!("gameMode.{}", game_mode_name(mode)), vec![]);
        let mut count = 0;
        for id in targets {
            let Some((entity, name)) = self.change_player_game_mode(id, mode) else {
                continue;
            };
            let own = command.source.entity == Some(entity);
            if !own
                && self
                    .game_rules
                    .boolean(ferraloom_game::gamerules::GameRule::SendCommandFeedback)
            {
                self.send_component(
                    Some(id),
                    Component::translatable(
                        "gameMode.changed",
                        vec![builtins::component(mode_name.clone())],
                    ),
                );
            }
            let message = if own {
                Component::translatable(
                    "commands.gamemode.success.self",
                    vec![builtins::component(mode_name.clone())],
                )
            } else {
                Component::translatable(
                    "commands.gamemode.success.other",
                    vec![
                        builtins::component(name),
                        builtins::component(mode_name.clone()),
                    ],
                )
            };
            self.command_success(source, &command.source, message, true);
            count += 1;
        }
        Ok(CommandResult {
            value: count,
            feedback: Vec::new(),
            rule_change: None,
        })
    }
}

impl MinecraftServer {
    /// Port of DedicatedServerSettings#update for command-controlled properties.
    fn update_command_property(&mut self, key: &str, value: impl ToString) {
        let properties = Arc::make_mut(&mut self.properties);
        properties.set(key, value);
        if let Err(error) = properties.store(&self.root.join("server.properties")) {
            eprintln!("Could not save server properties: {error:#}");
        }
        if key == "spawn-protection" {
            self.publish_spawn_protection();
        }
    }
    fn whitelist_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let action = command.literals[1].as_str();
        match action {
            "on" | "off" => {
                let enabled = action == "on";
                if self.properties.boolean("white-list") == enabled {
                    return Err(exceptions::error(
                        if enabled {
                            "commands.whitelist.alreadyOn"
                        } else {
                            "commands.whitelist.alreadyOff"
                        },
                        &[],
                    ));
                }
                self.update_command_property("white-list", enabled);
                self.command_success(
                    source,
                    &command.source,
                    Component::translatable(
                        if enabled {
                            "commands.whitelist.enabled"
                        } else {
                            "commands.whitelist.disabled"
                        },
                        vec![],
                    ),
                    true,
                );
                if enabled {
                    self.kick_unlisted_players();
                }
                Ok(CommandResult {
                    value: 1,
                    feedback: Vec::new(),
                    rule_change: None,
                })
            }
            "list" => {
                let names = self
                    .access
                    .whitelist
                    .values()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>();
                let count = names.len() as i32;
                Ok(CommandResult::success(
                    count,
                    if names.is_empty() {
                        Component::translatable("commands.whitelist.none", vec![])
                    } else {
                        Component::translatable(
                            "commands.whitelist.list",
                            vec![builtins::integer(count), builtins::string(names.join(", "))],
                        )
                    },
                    false,
                ))
            }
            "reload" => {
                if let Err(error) = self.access.reload_whitelist() {
                    eprintln!("Failed to reload white-list: {error:#}");
                }
                self.command_success(
                    source,
                    &command.source,
                    Component::translatable("commands.whitelist.reloaded", vec![]),
                    true,
                );
                self.kick_unlisted_players();
                Ok(CommandResult {
                    value: 1,
                    feedback: Vec::new(),
                    rule_change: None,
                })
            }
            "add" | "remove" => {
                let profiles = self.command_profiles(command)?;
                let add = action == "add";
                let mut count = 0;
                for profile in profiles {
                    let key = profile.id.to_string();
                    if self.access.whitelist.contains_key(&key) == add {
                        continue;
                    }
                    if add {
                        self.access.whitelist.insert(
                            key.clone(),
                            crate::access::WhiteListEntry {
                                uuid: key,
                                name: profile.name.clone(),
                            },
                        );
                    } else {
                        self.access.whitelist.remove(&key);
                    }
                    if let Err(error) = self.access.save_whitelist() {
                        eprintln!("Could not save white-list: {error:#}");
                    }
                    self.command_success(
                        source,
                        &command.source,
                        Component::translatable(
                            format!("commands.whitelist.{action}.success"),
                            vec![builtins::component(Component::literal(&profile.name))],
                        ),
                        true,
                    );
                    count += 1;
                }
                if count == 0 {
                    return Err(exceptions::error(
                        &format!("commands.whitelist.{action}.failed"),
                        &[],
                    ));
                }
                if !add {
                    self.kick_unlisted_players();
                }
                Ok(CommandResult {
                    value: count,
                    feedback: Vec::new(),
                    rule_change: None,
                })
            }
            _ => unreachable!(),
        }
    }
    fn command_reason(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<Option<String>, ferraloom_brigadier::CommandSyntaxException> {
        match command.argument("reason") {
            Some(ArgumentValue::Message(message)) => message
                .to_component(&command.source, self)
                .map(|component| Some(exceptions::plain_text(&component))),
            _ => Ok(None),
        }
    }
    fn command_text_name(&self, source: &CommandSourceStack) -> String {
        source.text_name(|entity| {
            self.player(entity)
                .map_or_else(|_| String::new(), |player| player.name.clone())
        })
    }
    fn ban_player_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let profiles = self.command_profiles(command)?;
        let reason = self.command_reason(command)?;
        let ban = command.command == "ban";
        let mut count = 0;
        for profile in profiles {
            let key = profile.id.to_string();
            if self.access.banned_players.contains_key(&key) == ban {
                continue;
            }
            let mut args = vec![builtins::component(Component::literal(&profile.name))];
            if ban {
                let entry = crate::access::Ban::permanent(
                    Some(key.clone()),
                    Some(profile.name.clone()),
                    None,
                    self.command_text_name(&command.source),
                    reason.clone(),
                );
                args.push(builtins::component(entry.reason_message()));
                self.access.banned_players.insert(key, entry);
            } else {
                self.access.banned_players.remove(&key);
            }
            if let Err(error) = self.access.save_bans(false) {
                eprintln!("Could not save ban list: {error:#}");
            }
            self.command_success(
                source,
                &command.source,
                Component::translatable(format!("commands.{}.success", command.command), args),
                true,
            );
            if ban {
                let online = self
                    .players
                    .iter()
                    .copied()
                    .find(|id| self.connections[id].profile.id == profile.id);
                if let Some(id) = online {
                    self.connections.get_mut(&id).unwrap().disconnect_component(
                        Component::translatable("multiplayer.disconnect.banned", vec![]),
                    );
                }
            }
            count += 1;
        }
        if count == 0 {
            return Err(exceptions::error(
                &format!("commands.{}.failed", command.command),
                &[],
            ));
        }
        Ok(CommandResult {
            value: count,
            feedback: Vec::new(),
            rule_change: None,
        })
    }
    fn ban_ip_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let input = command.string("target").expect("IP target argument");
        let ban = command.command == "ban-ip";
        let ip = if input.parse::<IpAddr>().is_ok() {
            input.to_owned()
        } else if ban {
            self.players
                .iter()
                .find_map(|id| {
                    let connection = &self.connections[id];
                    connection
                        .profile
                        .name
                        .eq_ignore_ascii_case(input)
                        .then(|| connection.address.ip().to_string())
                })
                .ok_or_else(|| exceptions::error("commands.banip.invalid", &[]))?
        } else {
            return Err(exceptions::error("commands.pardonip.invalid", &[]));
        };
        if self.access.banned_ips.contains_key(&ip) == ban {
            return Err(exceptions::error(
                if ban {
                    "commands.banip.failed"
                } else {
                    "commands.pardonip.failed"
                },
                &[],
            ));
        }
        if !ban {
            self.access.banned_ips.remove(&ip);
            if let Err(error) = self.access.save_bans(true) {
                eprintln!("Could not save IP ban list: {error:#}");
            }
            return Ok(CommandResult::success(
                1,
                Component::translatable("commands.pardonip.success", vec![builtins::string(ip)]),
                true,
            ));
        }
        let reason = self.command_reason(command)?;
        let entry = crate::access::Ban::permanent(
            None,
            None,
            Some(ip.clone()),
            self.command_text_name(&command.source),
            reason,
        );
        let message = Component::translatable(
            "commands.banip.success",
            vec![
                builtins::string(&ip),
                builtins::component(entry.reason_message()),
            ],
        );
        self.access.banned_ips.insert(ip.clone(), entry);
        if let Err(error) = self.access.save_bans(true) {
            eprintln!("Could not save IP ban list: {error:#}");
        }
        let targets = self
            .players
            .iter()
            .copied()
            .filter(|id| self.connections[id].address.ip().to_string() == ip)
            .collect::<Vec<_>>();
        self.command_success(source, &command.source, message, true);
        if !targets.is_empty() {
            let names = targets
                .iter()
                .filter_map(|id| {
                    let Phase::Play(entity) = self.connections[id].phase else {
                        return None;
                    };
                    self.player(entity).ok().map(ServerPlayer::display_name)
                })
                .collect::<Vec<_>>();
            self.command_success(
                source,
                &command.source,
                Component::translatable(
                    "commands.banip.info",
                    vec![
                        builtins::integer(targets.len() as i32),
                        builtins::component(builtins::format_list(names)),
                    ],
                ),
                true,
            );
        }
        for id in &targets {
            self.connections
                .get_mut(id)
                .unwrap()
                .disconnect_component(Component::translatable(
                    "multiplayer.disconnect.ip_banned",
                    vec![],
                ));
        }
        Ok(CommandResult {
            value: targets.len() as i32,
            feedback: Vec::new(),
            rule_change: None,
        })
    }
    fn ban_list_command(&self, command: &PreparedCommand) -> CommandResult {
        let mut entries = Vec::new();
        let kind = command.literals.get(1).map(String::as_str);
        if kind != Some("ips") {
            entries.extend(self.access.banned_players.values());
        }
        if kind != Some("players") {
            entries.extend(self.access.banned_ips.values());
        }
        let count = entries.len() as i32;
        let mut feedback = vec![builtins::Feedback {
            broadcast: false,
            message: Component::translatable(
                if count == 0 {
                    "commands.banlist.none"
                } else {
                    "commands.banlist.list"
                },
                if count == 0 {
                    vec![]
                } else {
                    vec![builtins::integer(count)]
                },
            ),
        }];
        feedback.extend(entries.into_iter().map(|entry| builtins::Feedback {
            broadcast: false,
            message: Component::translatable(
                "commands.banlist.entry",
                vec![
                        builtins::component(Component::literal(
                            entry
                                .name
                                .as_deref()
                                .or(entry.ip.as_deref())
                                .unwrap_or("(Unknown)"),
                        )),
                        builtins::string(entry.source.as_deref().unwrap_or("(Unknown)")),
                        builtins::component(entry.reason_message()),
                    ],
            ),
        }));
        CommandResult {
            value: count,
            feedback,
            rule_change: None,
        }
    }
}

impl MinecraftServer {
    fn command_entities(
        &mut self,
        command: &PreparedCommand,
        argument: &str,
    ) -> Result<Vec<EntityRef>, ferraloom_brigadier::CommandSyntaxException> {
        let Some(ArgumentValue::Entity(selector)) = command.argument(argument) else {
            unreachable!()
        };
        let entities = selector.find(&command.source, self, false)?;
        if entities.is_empty() {
            return Err(exceptions::error("argument.entity.notfound.entity", &[]));
        }
        Ok(entities)
    }
    pub(super) fn teleport_command(&mut self, command: &PreparedCommand) -> Result<CommandResult> {
        use ferraloom_game::commands::{
            source::{EntityAnchor, Rotation},
            teleport::TeleportPlan,
        };
        let targets = if command.argument("targets").is_some() {
            self.command_entities(command, "targets")?
        } else {
            vec![
                command
                    .source
                    .entity
                    .ok_or_else(|| exceptions::error("permissions.requires.entity", &[]))?,
            ]
        };
        let destination = if command.argument("destination").is_some() {
            self.command_entities(command, "destination")?
                .first()
                .copied()
        } else {
            None
        };
        let facing_entity = if command.argument("facingEntity").is_some() {
            self.command_entities(command, "facingEntity")?
                .first()
                .copied()
        } else {
            None
        };
        let facing_location = command
            .coordinates("facingLocation")
            .map(|coordinates| coordinates.position(&command.source, None));
        let mut names = Vec::new();
        let mut last_position = Vec3::ZERO;
        for target in &targets {
            let player = self
                .player(*target)
                .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?;
            let current_position = player.position;
            let current_rotation = Rotation {
                pitch: player.pitch,
                yaw: player.yaw,
            };
            names.push(player.display_name());
            let plan = if let Some(destination) = destination {
                let destination = self
                    .player(destination)
                    .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?;
                TeleportPlan::to_entity(
                    destination.position,
                    Rotation {
                        pitch: destination.pitch,
                        yaw: destination.yaw,
                    },
                    current_position,
                    current_rotation,
                )?
            } else {
                TeleportPlan::to_position(
                    &command.source,
                    command
                        .coordinates("location")
                        .expect("teleport coordinates"),
                    command.coordinates("rotation"),
                    current_position,
                    current_rotation,
                )?
            };
            last_position = plan.position;
            self.teleport_player(*target, plan.change, plan.relatives)?;
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(*target)
                .map_err(game_error)?;
            if !player.shared_flag(7) {
                player.velocity.y = 0.;
                player.on_ground = true;
            }
            let connection = self.players.iter().copied().find(|id| matches!(self.connections[id].phase, Phase::Play(entity) if entity == *target)).expect("selected connected player");
            let facing = if let Some(entity) = facing_entity {
                let player = self
                    .player(entity)
                    .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?;
                let eyes = command.string("facingAnchor") == Some("eyes");
                let position = Vec3::new(
                    player.position.x,
                    player.position.y
                        + if eyes {
                            f64::from(ferraloom_data::EntityType::PLAYER.data().eye_height)
                        } else {
                            0.
                        },
                    player.position.z,
                );
                Some((
                    position,
                    Some(cb::LookAtEntity {
                        id: VarInt(player.network_id),
                        anchor: if eyes {
                            play::EntityAnchor::Eyes
                        } else {
                            play::EntityAnchor::Feet
                        },
                    }),
                ))
            } else {
                facing_location.map(|position| (position, None))
            };
            if let Some((position, entity)) = facing {
                let facing_source = command
                    .source
                    .clone()
                    .with_entity(*target)
                    .with_position(plan.position);
                let rotation = facing_source
                    .facing(
                        position,
                        Some(f64::from(
                            ferraloom_data::EntityType::PLAYER.data().eye_height,
                        )),
                    )
                    .rotation;
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(*target)
                    .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?;
                player.yaw = rotation.yaw;
                player.pitch = rotation.pitch;
                self.connections
                    .get_mut(&connection)
                    .unwrap()
                    .push(cb::Packet::PlayerLookAt(cb::PlayerLookAt {
                        from_anchor: if command.source.anchor == EntityAnchor::Eyes {
                            play::EntityAnchor::Eyes
                        } else {
                            play::EntityAnchor::Feet
                        },
                        x: position.x,
                        y: position.y,
                        z: position.z,
                        entity,
                    }));
            }
        }
        let single = names.len() == 1;
        let mut args = vec![if single {
            builtins::component(names.remove(0))
        } else {
            builtins::integer(targets.len() as i32)
        }];
        let kind = if let Some(destination) = destination {
            args.push(builtins::component(
                self.player(destination)
                    .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?
                    .display_name(),
            ));
            "entity"
        } else {
            args.extend(
                [last_position.x, last_position.y, last_position.z]
                    .map(|value| builtins::string(format!("{value:.6}"))),
            );
            "location"
        };
        Ok(CommandResult::success(
            targets.len() as i32,
            Component::translatable(
                format!(
                    "commands.teleport.success.{kind}.{}",
                    if single { "single" } else { "multiple" }
                ),
                args,
            ),
            true,
        ))
    }
}

impl MinecraftServer {
    pub(super) fn default_spawn_data(&self) -> ferraloom_game::commands::spawn::SpawnData {
        let data = self.world.level_data.data.get_compound("spawn");
        ferraloom_game::commands::spawn::SpawnData {
            dimension: data
                .and_then(|data| data.get_string("dimension"))
                .map_or_else(|| "minecraft:overworld".into(), |id| id.to_string_lossy()),
            position: self.world.spawn,
            rotation: ferraloom_game::commands::source::Rotation {
                yaw: data.map_or(0., |data| data.get_float_or("yaw", 0.)),
                pitch: data.map_or(0., |data| data.get_float_or("pitch", 0.)),
            },
        }
    }
    fn spawn_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        use ferraloom_game::commands::spawn::SpawnData;
        // Player lists are captured before resolving the coordinates, as in SetSpawnCommand's executor.
        let targets = if command.command == "spawnpoint" {
            if command.argument("targets").is_some() {
                self.selected_connections(command, "targets")?
            } else {
                vec![source.ok_or_else(|| exceptions::error("permissions.requires.player", &[]))?]
            }
        } else {
            Vec::new()
        };
        let spawn = SpawnData::command(command)?;
        if command.command == "setworldspawn" {
            if self.default_spawn_data() != spawn {
                self.world.spawn = spawn.position;
                self.world
                    .level_data
                    .data
                    .insert("spawn", Tag::Compound(spawn.to_nbt()));
                self.publish_spawn_protection();
                self.broadcast(cb::Packet::SetDefaultSpawnPosition(
                    cb::SetDefaultSpawnPosition {
                        respawn_data: spawn.packet_data(),
                    },
                ));
            }
            return Ok(CommandResult::success(1, spawn.world_feedback(), true));
        }
        let mut only_name = None;
        for id in &targets {
            let Phase::Play(entity) = self.connections[id].phase else {
                continue;
            };
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(|_| exceptions::error("argument.entity.notfound.player", &[]))?;
            let mut config = spawn.to_nbt();
            config.insert("forced", Tag::Byte(1));
            player.saved.insert("respawn", Tag::Compound(config));
            only_name = Some(player.display_name());
        }
        let mut args = spawn.feedback_arguments();
        args.push(if targets.len() == 1 {
            builtins::component(only_name.unwrap())
        } else {
            builtins::integer(targets.len() as i32)
        });
        Ok(CommandResult::success(
            targets.len() as i32,
            Component::translatable(
                if targets.len() == 1 {
                    "commands.spawnpoint.success.single"
                } else {
                    "commands.spawnpoint.success.multiple"
                },
                args,
            ),
            true,
        ))
    }
}

impl ferraloom_game::commands::components::ComponentResolver for MinecraftServer {
    fn formatted_score(
        &mut self,
        holder: &str,
        objective: &str,
    ) -> std::result::Result<Component, ferraloom_game::commands::components::ResolutionError> {
        Ok(self
            .shared()
            .scoreboard
            .scoreboard()
            .formatted_score(holder, objective))
    }

    fn nbt_data(
        &mut self,
        source: &CommandSourceStack,
        data: &protocol::chat::DataSource,
    ) -> std::result::Result<
        Vec<ferraloom_nbt::CompoundTag>,
        ferraloom_game::commands::components::ResolutionError,
    > {
        use ferraloom_game::commands::components::{ResolutionError, UnsupportedContent};
        match data {
            protocol::chat::DataSource::Entity(pattern) => {
                let selector =
                    ferraloom_game::commands::arguments::selectors::EntitySelector::parse(
                        &mut ferraloom_brigadier::StringReader::new(pattern),
                        true,
                    )?;
                let entities = selector.find(source, self, false)?;
                entities
                    .into_iter()
                    .map(|entity| {
                        self.level
                            .level
                            .entity_nbt_to_compare(entity)
                            .map_err(ResolutionError::EntityData)
                    })
                    .collect()
            }
            protocol::chat::DataSource::Storage(id) => {
                let storage = self
                    .level
                    .level
                    .command_storage()
                    .ok_or(ResolutionError::Unsupported(UnsupportedContent::NbtData))?;
                Ok(vec![storage.get(id)?])
            }
            _ => Err(ResolutionError::Unsupported(UnsupportedContent::NbtData)),
        }
    }
}

#[cfg(test)]
mod command_context_tests {
    use super::*;
    use ferraloom_protocol::{
        Identifier,
        codec::VarInt,
        common::clientbound::{RegistryTags, TagEntry, UpdateTags},
    };
    #[test]
    fn command_context_shares_static_dynamic_and_empty_resolved_tags() {
        let id = |name: &str| Identifier::parse(name).unwrap();
        let registries = RegistrySet {
            entries: BTreeMap::from([(
                "minecraft:worldgen/biome".into(),
                vec!["minecraft:plains".into(), "minecraft:desert".into()],
            )]),
            tags: UpdateTags {
                tags: vec![
                    RegistryTags {
                        registry: id("minecraft:block"),
                        tags: vec![TagEntry {
                            name: id("minecraft:air_tag"),
                            ids: vec![VarInt(0)],
                        }],
                    },
                    RegistryTags {
                        registry: id("minecraft:worldgen/biome"),
                        tags: vec![
                            TagEntry {
                                name: id("minecraft:ordered"),
                                ids: vec![VarInt(1), VarInt(0)],
                            },
                            TagEntry {
                                name: id("minecraft:empty"),
                                ids: vec![],
                            },
                        ],
                    },
                ],
            },
        };
        let context = command_build_context(
            &registries,
            &ResourceManager::new(vec![]),
            RuleFeatures::default(),
        );
        assert_eq!(
            context.tag("minecraft:block", "minecraft:air_tag"),
            Some(["minecraft:air".into()].as_slice())
        );
        assert_eq!(
            context.tag("minecraft:worldgen/biome", "minecraft:ordered"),
            Some(["minecraft:desert".into(), "minecraft:plains".into()].as_slice())
        );
        assert_eq!(
            context.tag("minecraft:worldgen/biome", "minecraft:empty"),
            Some([].as_slice())
        );
    }
}
