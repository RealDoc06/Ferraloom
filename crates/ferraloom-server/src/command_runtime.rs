//! Exclusive call-site adapters. Game code owns parsing, feedback policy and command behavior.
use super::*;
use ferraloom_game::{
    commands::{
        PreparedCommand,
        builtins::{self, CommandResult},
        exceptions, feedback,
    },
    gamerules::{GameRuleChange, GameRuleEffect, GameRuleMap, GameRules, RuleFeatures},
};
use ferraloom_nbt::Tag;
use ferraloom_storage::{SavedData, SavedDataStorage};

pub(super) fn load_game_rules(world: &World) -> Result<GameRules> {
    let storage = SavedDataStorage::new(world.access.path().join("data"));
    let map = storage
        .load::<GameRuleMap>(GameRuleMap::STORAGE_ID)?
        .map(|data| data.value().clone())
        .unwrap_or_default();
    Ok(GameRules::new(RuleFeatures::default(), map))
}
impl MinecraftServer {
    pub(super) async fn save_game_rules(&self) -> Result<()> {
        let storage = SavedDataStorage::new(self.world.access.path().join("data"));
        let mut data = SavedData::new(self.game_rules.map().clone());
        let mut clocks = SavedData::new(self.clocks.states.clone());
        tokio::task::spawn_blocking(move || {
            storage.save(GameRuleMap::STORAGE_ID, &mut data)?;
            storage.save(PackedClockStates::STORAGE_ID, &mut clocks)
        })
        .await??;
        Ok(())
    }
    pub(super) fn on_game_rule_changed(&mut self, change: GameRuleChange) {
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
    fn send_component(&mut self, source: Option<ConnectionId>, content: Component) {
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
                        && self.access.ops.iter().any(|op| {
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
    fn apply_command_result(
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
            "seed" => builtins::seed(self.world.seed),
            "say" | "me" | "msg" => self.message_command(source, &command)?,
            "teammsg" => {
                return Err(exceptions::error(
                    if stack.entity.is_none() {
                        "permissions.requires.entity"
                    } else {
                        "commands.teammsg.failed.noteam"
                    },
                    &[],
                ));
            }
            "time" => {
                let (result, changed) = self.clocks.command(&command, self.level.game_time);
                if let Some(clock) = changed {
                    self.broadcast(self.clock_packet(Some(&clock)));
                }
                result?
            }
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
                        return Err(exceptions::error(
                            "commands.difficulty.failure",
                            &[difficulty_name(&current)],
                        ));
                    }
                    let mut settings = self
                        .world
                        .level_data
                        .data
                        .get_compound("difficulty_settings")
                        .cloned()
                        .unwrap_or_default();
                    settings.insert("difficulty", Tag::String(requested.as_str().into()));
                    self.world
                        .level_data
                        .data
                        .insert("difficulty_settings", Tag::Compound(settings));
                    self.broadcast(cb::Packet::ChangeDifficulty(cb::ChangeDifficulty {
                        difficulty: difficulty(requested),
                        locked: false,
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
                CommandResult::success(
                    0,
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
                    &[command.command.clone()],
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
fn difficulty_name(value: &str) -> String {
    match value {
        "peaceful" => "Peaceful",
        "normal" => "Normal",
        "hard" => "Hard",
        _ => "Easy",
    }
    .into()
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
    EntityQuery, EntitySelector, SelectorEntity, SelectorPredicate, rotation_matches,
};
impl EntityQuery for MinecraftServer {
    fn candidates(&self, _players_only: bool) -> Vec<EntityRef> {
        self.players
            .iter()
            .filter_map(|id| match self.connections[id].phase {
                Phase::Play(entity) => Some(entity),
                _ => None,
            })
            .collect()
    }
    fn info(&self, entity: EntityRef) -> Option<SelectorEntity<'_>> {
        let player = self.player(entity).ok()?;
        Some(SelectorEntity {
            name: &player.name,
            uuid: player.uuid,
            player: true,
            dimension: "minecraft:overworld",
            position: player.position,
            width: 0.6,
            height: 1.8,
        })
    }
    fn display_name(&self, entity: EntityRef) -> Component {
        self.player(entity)
            .map_or_else(|_| Component::literal(""), ServerPlayer::display_name)
    }
    fn matches(
        &mut self,
        entity: EntityRef,
        predicate: &SelectorPredicate,
    ) -> Result<bool, ferraloom_brigadier::CommandSyntaxException> {
        let player = self
            .player(entity)
            .map_err(|_| exceptions::error("argument.entity.notfound.entity", &[]))?;
        Ok(match predicate {
            SelectorPredicate::Alive => player.parent.health > 0.,
            SelectorPredicate::Name { value, inverted } => (player.name == *value) != *inverted,
            SelectorPredicate::GameMode { value, inverted } => {
                (player.game_mode == *value) != *inverted
            }
            SelectorPredicate::Team { value, inverted } => value.is_empty() != *inverted,
            SelectorPredicate::Type {
                value,
                tag,
                inverted,
            } => {
                let matched = if *tag {
                    self.command_entity_tags
                        .get(value)
                        .is_some_and(|types| types.contains(&EntityType::PLAYER))
                } else {
                    value == "minecraft:player"
                };
                matched != *inverted
            }
            SelectorPredicate::Tag { value, inverted } => {
                let tags = player.saved.get_list("Tags");
                let matched = if value.is_empty() {
                    tags.is_none_or(|tags| tags.is_empty())
                } else {
                    tags.is_some_and(|tags| {
                        tags.iter().any(|tag| {
                            tag.as_string()
                                .is_some_and(|tag| tag.to_string_lossy() == *value)
                        })
                    })
                };
                matched != *inverted
            }
            SelectorPredicate::Nbt { value, inverted } => {
                ferraloom_nbt::compare_nbt(Some(value), Some(&Tag::Compound(player.save())), true)
                    != *inverted
            }
            SelectorPredicate::Level(range) => {
                range.contains(f64::from(player.saved.get_int_or("XpLevel", 0)))
            }
            SelectorPredicate::Pitch(range) => rotation_matches(*range, player.pitch),
            SelectorPredicate::Yaw(range) => rotation_matches(*range, player.yaw),
            SelectorPredicate::Scores(scores) => scores.is_empty(),
            SelectorPredicate::Advancements(_) | SelectorPredicate::Predicate { .. } => {
                return Err(exceptions::error("ferraloom.selector.pending", &[]));
            }
        })
    }
    fn shuffle(&mut self, entities: &mut [(EntityRef, f64)]) {
        use rand::seq::SliceRandom;
        entities.shuffle(&mut rand::thread_rng());
    }
}
impl MinecraftServer {
    pub(super) fn command_targets(
        &mut self,
        source: Option<ConnectionId>,
        selector: Option<&str>,
    ) -> Result<Vec<ConnectionId>> {
        let mut stack = CommandSourceStack::server(4, Vec3::ZERO);
        if let Some(connection) = source {
            let Phase::Play(entity) = self.connections[&connection].phase else {
                bail!("command source not playing");
            };
            stack = CommandSourceStack::server(
                self.access
                    .permission(self.connections[&connection].profile.id),
                self.player(entity)?.position,
            )
            .with_entity(entity);
        }
        let Some(text) = selector else {
            return source
                .map(|id| vec![id])
                .context("permissions.requires.player");
        };
        let selector =
            EntitySelector::parse(&mut ferraloom_brigadier::StringReader::new(text), true)?;
        let entities = selector.find(&stack, self, true)?;
        if entities.is_empty() {
            return Err(exceptions::error("argument.entity.notfound.player", &[]).into());
        }
        Ok(entities
            .into_iter()
            .filter_map(|entity| {
                self.players.iter().copied().find(
                    |id| matches!(self.connections[id].phase,Phase::Play(handle) if handle==entity),
                )
            })
            .collect())
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
        let result = self.execute_command_inner(source, text).await;
        if let Some(connection) = source {
            if let Some(c) = self.connections.get_mut(&connection) {
                c.command_message = None;
            }
        }
        if let Err(error) = result {
            if error.to_string() == "disconnect.spam" {
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
pub(super) fn command_build_context(
    registries: &RegistrySet,
    resources: &ResourceManager,
) -> CommandBuildContext {
    let mut context = CommandBuildContext {
        registries: registries.entries.clone(),
        tags: BTreeMap::new(),
    };
    for registry in ["block", "item", "entity_type"] {
        let key = format!("minecraft:{registry}");
        let tags = ferraloom_data::datapack::load_tags(resources, registry, |id, _| {
            let id = id.to_string();
            context.contains(&key, &id).then_some(id)
        });
        context.tags.insert(
            key,
            tags.values
                .into_iter()
                .map(|(id, values)| (id.to_string(), values))
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
        let signed = source
            .and_then(|connection| self.connections.get_mut(&connection))
            .and_then(|c| c.command_message.take())
            .filter(|(name, _)| name == argument)
            .map(|(_, message)| message);
        let name = command.source.display_name(|entity| {
            self.player(entity)
                .map_or_else(|_| Component::literal(""), ServerPlayer::display_name)
        });
        let count = if let Some(targets) = targets {
            for entity in &targets {
                let target_name = self
                    .player(*entity)
                    .map_err(|_| exceptions::error("argument.entity.notfound.player", &[]))?
                    .display_name();
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
            let chat_type = if command.command == "me" {
                "emote_command"
            } else {
                "say_command"
            };
            let receivers = self.players.clone();
            for receiver in receivers {
                self.send_command_chat_to(
                    receiver,
                    signed.as_ref(),
                    &content,
                    ChatTypeBound {
                        chat_type: ChatTypeHolder(Holder::Reference(id(chat_type))),
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
            if let Some(signature) = &message.signature {
                if let Err(error) = c.last_seen.add_pending(signature) {
                    c.disconnect(error.to_string());
                    return;
                }
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
