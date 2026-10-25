//! Single-region MinecraftServer phase driver. Game state has one mutable owner.
#[path = "command_level.rs"]
mod command_level;
#[path = "command_runtime.rs"]
mod command_runtime;
#[path = "generation_completion.rs"]
pub(crate) mod generation_completion;
#[path = "interaction.rs"]
mod interaction;
use crate::{
    access::AccessLists,
    assets,
    chat::{
        ChatSession, LastSeenMessagesValidator, MessageSignatureCache, ServicesKeySet,
        ValidatedMessage,
    },
    commands::{CommandAction, CommandSourceStack, Commands},
    network::{self, ConnectionId, Incoming, Ingress, OutboundBatch, Outgoing},
    properties::{self, ServerProperties},
    registries::{LootRegistryLookup, RegistrySet, core_pack, id},
    world::{ChunkLight, World, heightmap},
};
use anyhow::{Context, Result, bail, ensure};
use ferraloom_data::{
    Block, EntityType,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_game::{
    Behaviors,
    block::{UnportedBehavior, containers::runtime::ContainerResources},
    crafting::{RecipeManager, RecipeSynchronization},
    entity::{
        EntityRef, EntityState,
        player::{
            GameType, MovementChecks, MovementResult, PlayerBehavior, PlayerMovement, ServerPlayer,
            move_player_with_checks,
        },
        teleport::{self as entity_teleport, position_move_rotation},
    },
    inventory::{
        MenuContext, PendingCraftingCallbacks, RecipeMenuHooks, RuntimeCraftingContext,
        UnsupportedMenuHooks,
    },
    level::{
        LevelError, Region,
        chunk_holder::ChunkStatus,
        distance_manager::tracking_view,
        server_level::ServerLevel,
        tick_rate::{ServerTickRateManager, TickRateEvent},
    },
    loot::LootData,
};
use ferraloom_protocol::{
    self as protocol, Uuid,
    chat::Component,
    codec::{BitSet, Holder, VarInt},
    common::{self, clientbound as common_cb},
    configuration as cfg,
    login::GameProfile,
    play::{self, clientbound as cb, serverbound as sb, *},
    status,
};
use ferraloom_util::{BlockPos, ChunkPos, Vec3};
use std::{
    collections::{BTreeMap, HashSet},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
    task::JoinHandle,
};

static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::PLAYER, &PlayerBehavior);
#[derive(Clone, Debug)]
pub struct ServerOptions {
    pub root: PathBuf,
    pub cache: PathBuf,
    pub port: Option<u16>,
    pub ip: Option<String>,
    pub console: bool,
    pub unported_behavior: UnportedBehavior,
}
impl ServerOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let cache = root.join(".cache");
        Self {
            root,
            cache,
            port: None,
            ip: None,
            console: true,
            unported_behavior: UnportedBehavior::Strict,
        }
    }
}
pub struct RunningServer {
    pub address: SocketAddr,
    pub ingress: mpsc::Sender<Ingress>,
    task: JoinHandle<Result<()>>,
}
impl RunningServer {
    pub async fn wait(self) -> Result<()> {
        self.task.await?
    }
    pub async fn command(&self, command: impl Into<String>) -> Result<()> {
        self.ingress.send(Ingress::Console(command.into())).await?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigTask {
    KnownPacks,
    Conduct,
    ResourcePack,
    Preparing,
    Finishing,
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Configuration(ConfigTask),
    Play(EntityRef),
}
struct Connection {
    profile: GameProfile,
    address: SocketAddr,
    outbound: mpsc::Sender<OutboundBatch>,
    phase: Phase,
    incoming_sequence: u64,
    outgoing_sequence: u64,
    pending: Vec<Outgoing>,
    close: bool,
    information: common::serverbound::ClientInformation,
    player: Option<ServerPlayer>,
    center: Option<ChunkPos>,
    last_keepalive: Instant,
    keepalive: Option<i64>,
    last_action: Instant,
    chat_spam: i32,
    command_spam: i32,
    session: Option<ChatSession>,
    last_seen: LastSeenMessagesValidator,
    signature_cache: MessageSignatureCache,
    command_message: Option<(String, ValidatedMessage)>,
    command_suggestion: Option<sb::CommandSuggestion>,
    next_chat_index: i32,
    network_ticks: u64,
    rate_packets: u64,
    average_packets: f32,
    received_position_this_tick: bool,
    received_movement_this_tick: bool,
    ack_block_changes_up_to: i32,
}
impl Connection {
    /// Port of ServerGamePacketListenerImpl#ackBlockChangesUpTo.
    fn ack_block_changes_up_to(&mut self, sequence: i32) -> Result<()> {
        ensure!(sequence >= 0, "Expected packet sequence nr >= 0");
        self.ack_block_changes_up_to = self.ack_block_changes_up_to.max(sequence);
        Ok(())
    }
    fn push(&mut self, packet: impl Into<Outgoing>) {
        self.pending.push(packet.into());
    }
    fn disconnect(&mut self, reason: impl Into<String>) {
        self.disconnect_component(Component::literal(reason));
    }
    fn disconnect_component(&mut self, reason: Component) {
        match self.phase {
            Phase::Configuration(_) => {
                self.push(cfg::ClientboundPacket::Disconnect(common_cb::Disconnect {
                    reason,
                }))
            }
            Phase::Play(_) => self.push(play::clientbound::Packet::Disconnect(Box::new(
                common_cb::Disconnect { reason },
            ))),
        };
        self.close = true;
    }
}
pub struct MinecraftServer {
    properties: Arc<ServerProperties>,
    root: PathBuf,
    registries: RegistrySet,
    recipe_manager: Arc<RecipeManager>,
    recipe_synchronization: Arc<RecipeSynchronization>,
    crafting_context: Arc<RuntimeCraftingContext>,
    access: AccessLists,
    level: crate::generation::runtime::LiveLevel,
    commands: Commands,
    game_rules: ferraloom_game::gamerules::GameRules,
    shared: Option<Box<ferraloom_game::ServerShared>>,
    clocks: ferraloom_game::commands::clocks::ServerClockManager,
    command_storage: ferraloom_game::commands::storage::CommandStorage,
    saving_enabled: bool,
    idle_timeout: i32,
    default_game_mode: GameType,
    command_entity_tags: BTreeMap<String, Vec<EntityType>>,
    selector_random: Option<ferraloom_util::random::LegacyRandomSource>,
    connections: BTreeMap<ConnectionId, Connection>,
    players: Vec<ConnectionId>,
    ingress: mpsc::Receiver<Ingress>,
    status: watch::Sender<status::ServerStatus>,
    shutdown: watch::Sender<bool>,
    tick: u64,
    pub(crate) tick_rate_manager: ServerTickRateManager,
    tick_times_nanos: [i64; 100],
    aggregated_tick_times_nanos: i64,
    stop: bool,
    empty_ticks: i32,
    was_paused: bool,
    autosave: i32,
    code_of_conduct: BTreeMap<String, String>,
    favicon: Option<Vec<u8>>,
    services: ServicesKeySet,
    enforces_secure_chat: bool,
    listener_task: Option<JoinHandle<Result<()>>>,
    pending_level_hooks: PendingLevelHooks,
}
/// Subsystems with explicit pending effects retain visible counters. Their
/// eventual ports consume the ordered effects at these same native slots.
#[derive(Default, Debug)]
struct PendingLevelHooks {
    natural_spawn_state: u64,
    natural_spawn_chunks: u64,
    custom_spawners: u64,
    poi: u64,
    navigation: u64,
    particles: u64,
    game_events: u64,
}
impl MinecraftServer {
    pub async fn start(options: ServerOptions) -> Result<RunningServer> {
        std::fs::create_dir_all(&options.root)?;
        let mut properties = ServerProperties::load(&options.root.join("server.properties"))?;
        properties::check_eula(&options.root)?;
        if let Some(port) = options.port {
            properties.set("server-port", port);
        }
        if let Some(ip) = options.ip {
            properties.set("server-ip", ip);
        }
        ensure!(
            !properties.boolean("management-server-enabled")
                && !properties.boolean("enable-rcon")
                && !properties.boolean("enable-query"),
            "Management, RCON and query listeners are not yet implemented"
        );
        let jar = assets::vanilla_jar(&options.cache).await?;
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(&jar)?]);
        let registries = RegistrySet::load(&resources)?;
        let recipe_manager = Arc::new(RecipeManager::load(&resources, &TagSet::load(&resources)?)?);
        let recipe_synchronization =
            Arc::new(RecipeSynchronization::new(&recipe_manager, |_| true));
        let loot = Arc::new(LootData::load(
            &resources,
            Some(&LootRegistryLookup::new(&registries, &resources)),
        )?);
        let crafting_context = Arc::new(RuntimeCraftingContext::load(&resources, &registries)?);

        let world = World::open(
            &options.root,
            &properties,
            &resources,
            &registries,
            options.unported_behavior,
        )?;
        let game_rules = command_runtime::load_game_rules(&world)?;
        // Older scoreboard paths are migrated by the datafix workstream, not runtime loading.
        let scoreboard = {
            use ferraloom_game::scoreboard::{ScoreboardSaveData, ServerScoreboard};
            let storage =
                ferraloom_storage::SavedDataStorage::new(world.access.path().join("data"));
            ServerScoreboard::new(
                storage
                    .load::<ScoreboardSaveData>(ScoreboardSaveData::STORAGE_ID)?
                    .map(|data| data.value().0.clone())
                    .unwrap_or_default(),
            )
        };
        let clocks = command_runtime::load_clocks(&world, &resources)?;
        let command_storage_folder = world.access.path().join("data");
        let command_storage = tokio::task::spawn_blocking(move || {
            ferraloom_game::commands::storage::CommandStorage::preload(&command_storage_folder)
        })
        .await?;
        let command_context = command_runtime::command_build_context(
            &registries,
            &resources,
            command_runtime::rule_features(&world),
        );
        let command_entity_tags =
            ferraloom_data::datapack::load_tags(&resources, "entity_type", |id, _| {
                EntityType::from_name(&id.to_string())
            })
            .values
            .into_iter()
            .map(|(id, types)| (id.to_string(), types))
            .collect();

        let game_time = world.level_data.data.get_long_or("Time", 0);
        let (access, ops) = AccessLists::load(&options.root)?;
        let mut level = ServerLevel::new(
            Region::new(&BEHAVIORS),
            "minecraft:overworld",
            world.level_context(&resources)?,
        );
        level
            .level
            .install_shared(Box::new(ferraloom_game::ServerShared { scoreboard, ops }));
        level.game_time = game_time;
        level.level.set_command_storage(command_storage.snapshot());
        initialize_live_level(
            &mut level,
            &world,
            &resources,
            &properties,
            options.unported_behavior,
        )?;
        let loaded_recipe_tags =
            ferraloom_data::datapack::load_tags(&resources, "recipe", |id, _| {
                recipe_manager.by_key(id).map(|recipe| {
                    protocol::Identifier::parse(&recipe.id.to_string())
                        .expect("validated recipe identifier")
                })
            });
        for diagnostic in loaded_recipe_tags.diagnostics {
            eprintln!("Recipe tag: {diagnostic:?}");
        }
        let recipe_tags = loaded_recipe_tags
            .values
            .into_iter()
            .map(|(id, values)| {
                (
                    protocol::Identifier::parse(&id.to_string())
                        .expect("validated recipe tag identifier"),
                    values,
                )
            })
            .collect();
        let advancements = Arc::new(
            ferraloom_game::advancements::ServerAdvancementManager::load(
                &resources,
                &ferraloom_game::advancements::AdvancementRegistryContext {
                    loot: &loot,
                    recipes: &recipe_manager,
                    recipe_tags: &recipe_tags,
                    lookup: Some(&LootRegistryLookup::new(&registries, &resources)),
                },
            )?,
        );
        for warning in &advancements.validation_problems {
            eprintln!(
                "Advancement {} criterion {} {}: {}",
                warning.advancement,
                warning.criterion,
                warning.problem.path,
                warning.problem.message
            );
        }
        level.level.set_container_resources(
            Arc::new(ContainerResources {
                recipes: Arc::clone(&recipe_manager),
                loot,
                crafting: Arc::clone(&crafting_context),
                advancements,
                recipe_tags,
                recipe_synchronization: Arc::clone(&recipe_synchronization),
            }),
            world.seed,
        );
        game_rules.update_level_rules(level.level.rules_mut());
        world.install_live_environment(
            &mut level.level,
            command_runtime::environment_clock_snapshot(&clocks),
        )?;
        level
            .distance_manager
            .set_view_distance(properties.int("view-distance"));
        level.level.configure_entity_tracking(
            properties.int("view-distance"),
            properties.int("entity-broadcast-range-percentage"),
        );
        level.distance_manager.set_simulation_distance(
            properties.int("simulation-distance").clamp(2, 32),
            &mut level.tickets,
        );
        let favicon = std::fs::read(options.root.join("server-icon.png")).ok();
        if let Some(icon) = &favicon {
            ensure!(
                icon.len() >= 24
                    && &icon[..8] == b"\x89PNG\r\n\x1a\n"
                    && icon[16..24] == [0, 0, 0, 64, 0, 0, 0, 64],
                "server-icon.png must be a 64x64 PNG"
            );
        }
        let conduct = load_conduct(&options.root, &properties)?;
        let services = if properties.boolean("online-mode") {
            match ServicesKeySet::fetch().await {
                Ok(keys) => keys,
                Err(e) => {
                    eprintln!("Failed to request Minecraft Services public key: {e}");
                    ServicesKeySet::default()
                }
            }
        } else {
            ServicesKeySet::default()
        };
        let enforces_secure_chat = properties.boolean("online-mode")
            && properties.boolean("enforce-secure-profile")
            && services.can_validate();
        let initial_status = make_status(
            &properties,
            0,
            Vec::new(),
            favicon.clone(),
            enforces_secure_chat,
        );
        let (status, status_rx) = watch::channel(initial_status);
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (send, ingress) = mpsc::channel(8192);
        let ip = if properties.get("server-ip").is_empty() {
            "0.0.0.0"
        } else {
            properties.get("server-ip")
        };
        let port = u16::try_from(properties.int("server-port")).context("invalid server-port")?;
        let listener = TcpListener::bind((ip, port)).await?;
        let address = listener.local_addr()?;
        let properties = Arc::new(properties);
        let options_net = Arc::new(network::NetworkOptions {
            properties: properties.clone(),
            keys: protocol::transport::LoginKeyPair::generate()?,
            session_id: random_uuid(),
            status: status_rx,
            registries: Arc::new(registries.clone()),
            encoding: Arc::new(crate::jobs::JobPool::new("ferraloom-packet-encode", 2)?),
        });
        let net_send = send.clone();
        let listener_task = tokio::spawn(network::listen(
            listener,
            options_net,
            net_send,
            shutdown_rx,
        ));
        if options.console {
            let console = send.clone();
            std::thread::Builder::new()
                .name("ferraloom-console".into())
                .spawn(move || {
                    use std::io::BufRead;
                    for line in std::io::stdin().lock().lines() {
                        match line {
                            Ok(line) => {
                                if console.blocking_send(Ingress::Console(line)).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })?;
        }
        let mut level =
            crate::generation::runtime::LiveLevel::install(world, level, properties.clone())?;
        let shared = Some(level.level.take_shared().map_err(game_error)?);
        let mut server = Self {
            properties: properties.clone(),
            root: options.root,
            registries,
            recipe_manager,
            recipe_synchronization,
            crafting_context,
            access,
            level,
            game_rules,
            shared,
            clocks,
            command_storage,
            command_entity_tags,
            selector_random: None,
            saving_enabled: true,
            idle_timeout: properties.int("player-idle-timeout"),
            default_game_mode: game_type(properties.get("gamemode")),
            commands: Commands::new(command_context),
            connections: BTreeMap::new(),
            players: Vec::new(),
            ingress,
            status,
            shutdown,
            tick: 0,
            tick_rate_manager: ServerTickRateManager::default(),
            tick_times_nanos: [0; 100],
            aggregated_tick_times_nanos: 0,
            stop: false,
            empty_ticks: 0,
            was_paused: false,
            autosave: 6000,
            code_of_conduct: conduct,
            favicon,
            services,
            enforces_secure_chat,
            listener_task: Some(listener_task),
            pending_level_hooks: PendingLevelHooks::default(),
        };
        server.publish_spawn_protection();
        let task = tokio::spawn(server.run());
        Ok(RunningServer {
            address,
            ingress: send,
            task,
        })
    }
    /// Port of MinecraftServer#runServer's tick-rate deadline and sprint slots.
    async fn run(mut self) -> Result<()> {
        let lent = self.lend_server_shared();
        let result = self.prepare_command_tickets().await;
        self.return_server_shared(lent);
        result?;
        println!("Ferraloom 26.3 running; single-region mode.");
        let origin = tokio::time::Instant::now();
        let mut next = 0_i64;
        let mut last_overload = 0_i64;
        while !self.stop {
            let now = origin.elapsed().as_nanos() as i64;
            let this_tick_nanos = if !self.was_paused
                && self.tick_rate_manager.is_sprinting()
                && self.tick_rate_manager.check_should_sprint_this_tick(now)
            {
                next = now;
                last_overload = next;
                0
            } else {
                let nanos = self.tick_rate_manager.nanoseconds_per_tick();
                let behind = now.wrapping_sub(next);
                if behind > 1_000_000_000 + 20 * nanos
                    && next.wrapping_sub(last_overload) >= 10_000_000_000 + 100 * nanos
                {
                    let skipped = behind / nanos;
                    eprintln!(
                        "Can't keep up! Running {}ms or {skipped} ticks behind",
                        behind / 1_000_000
                    );
                    next = next.wrapping_add(skipped * nanos);
                    last_overload = next;
                }
                nanos
            };
            next = next.wrapping_add(this_tick_nanos);
            self.apply_tick_rate_events()?;
            self.process_packets().await?;
            self.tick_server().await?;
            tokio::time::sleep_until(origin + Duration::from_nanos(next.max(0) as u64)).await;
            if this_tick_nanos == 0 {
                self.tick_rate_manager
                    .end_tick_work(origin.elapsed().as_nanos() as i64);
            }
        }
        self.stop_server().await
    }
    fn shared(&self) -> &ferraloom_game::ServerShared {
        self.shared.as_deref().unwrap_or_else(|| {
            self.level
                .level
                .shared()
                .expect("server shared owner is installed for the phase")
        })
    }
    fn shared_mut(&mut self) -> &mut ferraloom_game::ServerShared {
        match self.shared.as_deref_mut() {
            Some(shared) => shared,
            None => self
                .level
                .level
                .shared_mut()
                .expect("server shared owner is installed for the phase"),
        }
    }
    /// Only outer phase boundaries move ownership. Nested commands keep the installed owner.
    fn lend_server_shared(&mut self) -> bool {
        self.level.generation().enter_owner_thread();
        match self.shared.take() {
            Some(shared) => {
                self.level.level.install_shared(shared);
                true
            }
            None => {
                self.level
                    .level
                    .shared()
                    .expect("nested phase requires the installed owner");
                false
            }
        }
    }
    fn drain_scoreboard_packets(&mut self) {
        for packet in self.shared_mut().scoreboard.drain_packets() {
            self.broadcast(packet);
        }
    }
    fn return_server_shared(&mut self, lent: bool) {
        self.drain_scoreboard_packets();
        if lent {
            self.shared = Some(
                self.level
                    .level
                    .take_shared()
                    .expect("phase must return its shared owner"),
            );
        }
    }
    pub fn average_tick_time_nanos(&self) -> i64 {
        self.aggregated_tick_times_nanos / self.tick.clamp(1, 100) as i64
    }
    pub fn tick_times_nanos(&self) -> &[i64; 100] {
        &self.tick_times_nanos
    }
    fn next_autosave_interval(&self) -> i32 {
        let tps = if self.tick_rate_manager.is_sprinting() {
            1_000_000_000_f32 / self.average_tick_time_nanos().wrapping_add(1) as f32
        } else {
            self.tick_rate_manager.tickrate()
        };
        (tps * 300.0_f32).max(100.0) as i32
    }
    /// Drain immediately before command feedback and at the outer sprint slot.
    pub(crate) fn apply_tick_rate_events(&mut self) -> Result<()> {
        for event in self.tick_rate_manager.take_events() {
            match event {
                TickRateEvent::State {
                    tick_rate,
                    is_frozen,
                } => {
                    self.broadcast(cb::Packet::TickingState(cb::TickingState {
                        tick_rate,
                        is_frozen,
                    }));
                }
                TickRateEvent::Step { ticks } => {
                    self.broadcast(cb::Packet::TickingStep(cb::TickingStep {
                        tick_steps: VarInt(ticks),
                    }));
                }
                TickRateEvent::RateChanged => {
                    self.autosave = self.autosave.min(self.next_autosave_interval())
                }
                TickRateEvent::SprintReport {
                    completed_ticks: _,
                    ticks_per_second,
                    milliseconds_per_tick,
                } => {
                    let report = ferraloom_game::commands::tick::sprint_report(
                        ticks_per_second,
                        milliseconds_per_tick,
                    );
                    self.command_success(
                        None,
                        &CommandSourceStack::server(4, Vec3::ZERO),
                        report.message,
                        report.broadcast,
                    );
                }
            }
        }
        Ok(())
    }
    /// Port of MinecraftServer#processPacketsAndTick. No IO task applies play state.
    async fn process_packets(&mut self) -> Result<()> {
        let lent = self.lend_server_shared();
        let result = self.process_packets_inner().await;
        self.return_server_shared(lent);
        result
    }
    async fn process_packets_inner(&mut self) -> Result<()> {
        let count = self.ingress.len();
        for _ in 0..count {
            let Ok(event) = self.ingress.try_recv() else {
                break;
            };
            match event {
                Ingress::Admission {
                    id,
                    profile,
                    address,
                    reply,
                } => {
                    let full =
                        self.players.len() >= self.properties.int("max-players").max(0) as usize;
                    let reason = self.access.can_join(
                        &profile,
                        address.ip(),
                        self.properties.boolean("white-list"),
                        full,
                        &self.shared().ops,
                    );
                    if reason.is_none() {
                        let duplicates = self
                            .connections
                            .iter()
                            .filter(|(other, c)| **other != id && c.profile.id == profile.id)
                            .map(|(id, _)| *id)
                            .collect::<Vec<_>>();
                        for id in duplicates {
                            self.connections
                                .get_mut(&id)
                                .unwrap()
                                .disconnect("You logged in from another location");
                        }
                    }
                    let _ = reply.send(reason);
                }
                Ingress::Connected {
                    id,
                    profile,
                    address,
                    outbound,
                } => {
                    let mut c = Connection {
                        profile,
                        address,
                        outbound,
                        phase: Phase::Configuration(ConfigTask::KnownPacks),
                        incoming_sequence: 0,
                        outgoing_sequence: 0,
                        pending: Vec::new(),
                        close: false,
                        information: client_information(),
                        player: None,
                        center: None,
                        last_keepalive: Instant::now(),
                        keepalive: None,
                        last_action: Instant::now(),
                        chat_spam: 0,
                        command_spam: 0,
                        session: None,
                        last_seen: LastSeenMessagesValidator::default(),
                        signature_cache: MessageSignatureCache::default(),
                        command_message: None,
                        command_suggestion: None,
                        next_chat_index: 0,
                        network_ticks: 0,
                        rate_packets: 0,
                        average_packets: 0.,
                        received_position_this_tick: false,
                        received_movement_this_tick: false,
                        ack_block_changes_up_to: -1,
                    };
                    c.push(cfg::ClientboundPacket::CustomPayload(
                        common_cb::CustomPayload {
                            payload: common::CustomPayload::Brand("Ferraloom".into()),
                        },
                    ));
                    if !self.properties.get("bug-report-link").is_empty() {
                        c.push(cfg::ClientboundPacket::ServerLinks(
                            common_cb::ServerLinks {
                                links: vec![common_cb::ServerLink {
                                    link_type: protocol::codec::Either::Left(
                                        common_cb::KnownLinkType::BugReport,
                                    ),
                                    link: self.properties.get("bug-report-link").into(),
                                }],
                            },
                        ));
                    }
                    c.push(cfg::ClientboundPacket::UpdateEnabledFeatures(
                        cfg::UpdateEnabledFeatures {
                            features: vec![crate::registries::id("vanilla")],
                        },
                    ));
                    c.push(cfg::ClientboundPacket::SelectKnownPacks(
                        cfg::ClientboundSelectKnownPacks {
                            known_packs: vec![core_pack()],
                        },
                    ));
                    self.connections.insert(id, c);
                }
                Ingress::Packet {
                    id,
                    sequence,
                    packet,
                } => {
                    let Some(c) = self.connections.get_mut(&id) else {
                        continue;
                    };
                    if c.close {
                        continue;
                    }
                    ensure!(
                        sequence == c.incoming_sequence,
                        "connection {id} ingress sequence mismatch"
                    );
                    c.incoming_sequence += 1;
                    let result = match packet {
                        Incoming::Configuration(p) => self.configuration(id, p).await,
                        Incoming::Play(p) => self.play_packet(id, *p).await,
                    };
                    if let Err(e) = result {
                        eprintln!("Packet rejected for {id}: {e:#}");
                        if let Some(c) = self.connections.get_mut(&id) {
                            c.disconnect(e.to_string());
                        }
                    }
                }
                Ingress::Closed { id } => self.remove_player(id).await?,
                Ingress::Console(command) => {
                    if let Err(e) = self.execute_command(None, &command).await {
                        eprintln!("{e:#}");
                    }
                }
            }
        }
        Ok(())
    }
    /// TickChildren retains level-before-connection and chunk-send-after-player order.
    async fn tick_server(&mut self) -> Result<()> {
        let started = Instant::now();
        self.empty_ticks = if self.players.is_empty() && !self.tick_rate_manager.is_sprinting() {
            self.empty_ticks.saturating_add(1)
        } else {
            0
        };
        let threshold = self.properties.int("pause-when-empty-seconds");
        let paused = threshold > 0 && self.empty_ticks >= threshold.saturating_mul(20);
        if paused && !self.was_paused {
            self.save_all(false, false).await?;
        }
        self.was_paused = paused;
        let lent = self.lend_server_shared();
        let level_result = (|| -> Result<()> {
            if !paused {
                self.tick = self.tick.wrapping_add(1);
                self.tick_rate_manager.tick();
                // Functions precede the canonical server clocks in this serial slot.
                if self.tick_rate_manager.runs_normally() {
                    self.clocks.tick(
                        self.game_rules
                            .boolean(ferraloom_game::gamerules::GameRule::AdvanceTime),
                    );
                }
                if self.tick.is_multiple_of(20) {
                    let packet = self.time_packet();
                    for c in self.connections.values_mut() {
                        if matches!(c.phase, Phase::Play(_)) {
                            c.push(packet.clone());
                        }
                    }
                }
                self.level.level.set_handling_tick(true);
                self.level
                    .level
                    .refresh_environment_attributes(command_runtime::environment_clock_snapshot(
                        &self.clocks,
                    ))
                    .map_err(game_error)?;
                if self.tick_rate_manager.runs_normally() {
                    self.level.level.tick_world_border();
                    self.level.level.advance_weather_cycle(true);
                    self.apply_level_effects()?;
                }
                // Native sky brightness updates even on frozen ticks, after weather.
                self.level
                    .level
                    .update_sky_brightness()
                    .map_err(game_error)?;
                if self.tick_rate_manager.runs_normally() {
                    self.level.game_time = self.level.game_time.wrapping_add(1);
                    self.level.level.set_game_time(self.level.game_time);
                    if !self.level.level.rules().debug {
                        self.level
                            .level
                            .tick_block_phase(self.level.game_time, 65536)
                            .map_err(game_error)?;
                        self.level
                            .level
                            .tick_fluid_phase(self.level.game_time, 65536)
                            .map_err(game_error)?;
                    }
                    self.level.generation().purge_stale_tickets(self.tick);
                }
                self.integrate_jobs()?;
                self.update_chunk_tickets()?;
                if !self.level.level.rules().debug {
                    if self.tick_rate_manager.runs_normally() {
                        self.tick_live_chunks()?;
                    }
                    self.broadcast_block_changes()?;
                }
                // ChunkMap tracking precedes block events and entities.
                self.track_players()?;
                self.level
                    .level
                    .tick_entity_tracker()
                    .map_err(anyhow::Error::new)?;
                self.apply_level_effects()?;
                if self.tick_rate_manager.runs_normally() {
                    self.level.level.run_block_events().map_err(game_error)?;
                }
                self.level.level.set_handling_tick(false);
                let active = self
                    .level
                    .generation()
                    .with_tickets(|tickets| tickets.should_keep_dimension_active());
                if self
                    .level
                    .level
                    .update_empty_time(active, self.tick_rate_manager.runs_normally())
                {
                    self.level
                        .level
                        .tick_entities_with_tick_rate(&self.tick_rate_manager)
                        .map_err(game_error)?;
                    self.broadcast_container_changes()?;
                    self.level
                        .level
                        .tick_block_entities(self.tick_rate_manager.runs_normally())
                        .map_err(game_error)?;
                }
                self.apply_level_effects()?;
                self.level
                    .generation()
                    .poll_entity_progress(&self.level.level)?;
                self.level.level.tick_entity_management()?;
                let requests = self.level.level.take_entity_storage_requests();
                self.level.generation().with_world_mut(|world| {
                    world.submit_entity_requests(requests, &self.properties)
                })?;
                self.level.level.collect_detached_block_entities();
                if self.tick.is_multiple_of(1200) {
                    eprintln!(
                        "Live level: unported={:?}, pending={:?}, precipitation={}",
                        self.level.level.unported_behavior_stats(),
                        self.pending_level_hooks,
                        self.level.level.pending_precipitation_calls()
                    );
                }
            } else {
                self.integrate_jobs()?;
            }
            Ok(())
        })();
        self.return_server_shared(lent);
        level_result?;
        let lent = self.lend_server_shared();
        let connection_result = async {
            self.tick_connections().await?;
            self.send_chunks()
        }
        .await;
        self.return_server_shared(lent);
        connection_result?;
        self.flush_outbound();
        self.refresh_status();
        if !paused {
            self.autosave -= 1;
            if self.autosave <= 0 {
                self.save_all(false, false).await?;
                self.autosave = self.next_autosave_interval();
            }
        }
        if !paused {
            let index = (self.tick % 100) as usize;
            let elapsed = started.elapsed().as_nanos() as i64;
            self.aggregated_tick_times_nanos = self
                .aggregated_tick_times_nanos
                .wrapping_sub(self.tick_times_nanos[index])
                .wrapping_add(elapsed);
            self.tick_times_nanos[index] = elapsed;
        }
        Ok(())
    }
    async fn configuration(
        &mut self,
        connection: ConnectionId,
        packet: cfg::ServerboundPacket,
    ) -> Result<()> {
        let phase = self.connections[&connection].phase;
        ensure!(
            matches!(phase, Phase::Configuration(_)),
            "unexpected configuration packet"
        );
        match packet {
            cfg::ServerboundPacket::ClientInformation(information) => {
                self.connections.get_mut(&connection).unwrap().information = information;
            }
            cfg::ServerboundPacket::SelectKnownPacks(p) => {
                ensure!(
                    matches!(phase, Phase::Configuration(ConfigTask::KnownPacks)),
                    "unexpected pack selection"
                );
                let packets = self.registries.synchronize(&p.known_packs)?;
                let c = self.connections.get_mut(&connection).unwrap();
                for packet in packets {
                    c.push(packet);
                }
                self.optional_tasks(connection, false).await?;
            }
            cfg::ServerboundPacket::AcceptCodeOfConduct(_) => {
                ensure!(
                    matches!(phase, Phase::Configuration(ConfigTask::Conduct)),
                    "unexpected code of conduct response"
                );
                self.optional_tasks(connection, true).await?;
            }
            cfg::ServerboundPacket::ResourcePack(p) => {
                ensure!(
                    matches!(phase, Phase::Configuration(ConfigTask::ResourcePack)),
                    "unexpected resource pack response"
                );
                use common::serverbound::ResourcePackAction as A;
                if !matches!(p.action, A::Accepted | A::Downloaded) {
                    if self.properties.boolean("require-resource-pack")
                        && p.action != A::SuccessfullyLoaded
                    {
                        bail!("You must accept the server resource pack");
                    }
                    self.prepare_player(connection).await?;
                }
            }
            cfg::ServerboundPacket::FinishConfiguration(_) => {
                ensure!(
                    matches!(phase, Phase::Configuration(ConfigTask::Finishing)),
                    "unexpected configuration completion"
                );
                let c = &self.connections[&connection];
                if let Some(reason) = self.access.can_join(
                    &c.profile,
                    c.address.ip(),
                    self.properties.boolean("white-list"),
                    self.players.len() >= self.properties.int("max-players").max(0) as usize,
                    &self.shared().ops,
                ) {
                    bail!("{reason}");
                }
                self.place_player(connection)?;
            }
            cfg::ServerboundPacket::KeepAlive(p) => self.keepalive_reply(connection, p.id)?,
            cfg::ServerboundPacket::CookieResponse(_) => {
                bail!("multiplayer.disconnect.unexpected_query_response")
            }
            cfg::ServerboundPacket::CustomPayload(_)
            | cfg::ServerboundPacket::Pong(_)
            | cfg::ServerboundPacket::CustomClickAction(_) => {}
        }
        Ok(())
    }
    async fn optional_tasks(&mut self, connection: ConnectionId, conduct_done: bool) -> Result<()> {
        let c = self.connections.get_mut(&connection).unwrap();
        if !conduct_done && !self.code_of_conduct.is_empty() {
            let language = c.information.language.0.to_lowercase();
            let text = self
                .code_of_conduct
                .get(&language)
                .or_else(|| self.code_of_conduct.get("en_us"))
                .or_else(|| self.code_of_conduct.values().next())
                .unwrap()
                .clone();
            c.push(cfg::ClientboundPacket::CodeOfConduct(cfg::CodeOfConduct {
                code_of_conduct: text,
            }));
            c.phase = Phase::Configuration(ConfigTask::Conduct);
            return Ok(());
        }
        let url = self.properties.get("resource-pack");
        if !url.is_empty() {
            let pack_id = if self.properties.get("resource-pack-id").is_empty() {
                Uuid::from_name(url.as_bytes())
            } else {
                match protocol::codec::uuid_from_string(self.properties.get("resource-pack-id")) {
                    Ok(id) => id,
                    Err(error) => {
                        eprintln!("Invalid resource-pack-id; skipping resource pack: {error}");
                        return self.prepare_player(connection).await;
                    }
                }
            };
            c.push(cfg::ClientboundPacket::ResourcePackPush(
                common_cb::ResourcePackPush {
                    id: pack_id,
                    url: url.into(),
                    hash: common::Utf8(self.properties.get("resource-pack-sha1").into()),
                    required: self.properties.boolean("require-resource-pack"),
                    prompt: if self.properties.get("resource-pack-prompt").is_empty() {
                        None
                    } else {
                        Component::from_json(self.properties.get("resource-pack-prompt")).ok()
                    },
                },
            ));
            c.phase = Phase::Configuration(ConfigTask::ResourcePack);
            return Ok(());
        }
        self.prepare_player(connection).await
    }
    async fn prepare_player(&mut self, connection: ConnectionId) -> Result<()> {
        let profile = self.connections[&connection].profile.clone();
        let access = self
            .level
            .generation()
            .with_world(|world| world.access.clone());
        let uuid = profile.id.to_string();
        let saved = tokio::task::spawn_blocking(move || access.player_data().load(&uuid)).await??;
        let respawn = self.default_spawn_data();
        let (yaw, pitch) = ServerPlayer::saved_spawn_rotation(saved.as_ref())
            .unwrap_or((respawn.rotation.yaw, respawn.rotation.pitch));
        let mode = self.default_game_mode;
        let mut player = ServerPlayer::new(
            profile.id,
            profile.name,
            self.level.level.next_entity_id(),
            Vec3::new(
                f64::from(self.level.generation().with_world(|world| world.spawn.x)) + 0.5,
                f64::from(self.level.generation().with_world(|world| world.spawn.y)),
                f64::from(self.level.generation().with_world(|world| world.spawn.z)) + 0.5,
            ),
            mode,
        );
        if let Some(saved) = saved {
            player.try_load_with_recipes(saved, &self.recipe_manager)?;
        }
        let advancement_manager = self
            .level
            .level
            .container_resources()
            .map_err(game_error)?
            .advancements
            .clone();
        let progress_path = self
            .level
            .generation()
            .with_world(|world| world.access.path().to_path_buf());
        let progress_uuid = profile.id;
        let progress = tokio::task::spawn_blocking(move || {
            ferraloom_game::advancements::load_player_progress(
                &progress_path,
                progress_uuid,
                &advancement_manager,
            )
        })
        .await??;
        for diagnostic in progress.diagnostics {
            eprintln!("{diagnostic}");
        }
        player.advancements = progress.advancements;
        player.stats = progress.stats;
        if self.properties.boolean("force-gamemode") {
            player.game_mode = mode;
            player.update_game_mode_abilities();
        }
        let spawn_position = player.position;
        player.snap_to(spawn_position, yaw, pitch);
        player.view_distance = i32::from(self.connections[&connection].information.view_distance);
        player.text_filtering_enabled = self.connections[&connection]
            .information
            .text_filtering_enabled;
        let center = player.chunk_position();
        self.level
            .generation()
            .with_tickets_mut(|tickets| tickets.add_player_spawn(center, self.tick));
        self.update_chunk_tickets()?;
        self.request_area(center, 3)?;
        let c = self.connections.get_mut(&connection).unwrap();
        c.player = Some(player);
        c.phase = Phase::Configuration(ConfigTask::Preparing);
        Ok(())
    }
    fn request_area(&mut self, center: ChunkPos, radius: i32) -> Result<()> {
        ensure!(radius <= 34, "chunk request radius too large");
        let mut chunks = Vec::new();
        for x in center.x - radius..=center.x + radius {
            for z in center.z - radius..=center.z + radius {
                let pos = ChunkPos::new(x, z);
                let dx = x - center.x;
                let dz = z - center.z;
                chunks.push((dx * dx + dz * dz, pos));
            }
        }
        chunks.sort_by_key(|(distance, pos)| (*distance, pos.pack()));
        for (_, pos) in chunks {
            self.level
                .generation()
                .request_full(&self.level.level, pos)
                .map_err(game_error)?;
        }
        Ok(())
    }
    /// ServerChunkCache's spawning shuffle and random-tick slots. The two
    /// primitive trackers, rather than holder iteration, select traversal order.
    fn tick_live_chunks(&mut self) -> Result<()> {
        self.pending_level_hooks.natural_spawn_state += 1;
        let mut spawning = Vec::new();
        for pos in self.level.generation().natural_spawn_chunks() {
            if !self
                .level
                .level
                .chunk(pos)
                .is_some_and(|chunk| chunk.ticking_ready())
            {
                continue;
            }
            let mut close = false;
            for &connection in &self.players {
                let c = &self.connections[&connection];
                let Phase::Play(entity) = c.phase else {
                    continue;
                };
                let player = self.player(entity)?;
                if player.game_mode != GameType::Spectator {
                    let dx = f64::from(pos.x * 16 + 8) - player.position.x;
                    let dz = f64::from(pos.z * 16 + 8) - player.position.z;
                    if dx * dx + dz * dz < 16384.0 {
                        close = true;
                        break;
                    }
                }
            }
            if close {
                spawning.push(pos);
            }
        }
        self.level.level.shuffle_spawning_chunks(&mut spawning);
        for pos in spawning {
            self.level
                .level
                .increment_chunk_inhabited_time(pos)
                .map_err(game_error)?;
            if self.level.level.is_entity_ticking_chunk(pos) {
                self.level.level.tick_thunder(pos).map_err(game_error)?;
            }
            // SPAWN_MOBS defaults true until the commands GameRules adapter lands.
            self.pending_level_hooks.natural_spawn_chunks += 1;
        }
        let ticking: Vec<_> = self
            .level
            .level
            .entity_ticking_chunks()
            .into_iter()
            .filter(|&pos| {
                self.level
                    .level
                    .chunk(pos)
                    .is_some_and(|chunk| chunk.ticking_ready())
            })
            .collect();
        for pos in ticking {
            self.level
                .level
                .tick_random_chunk(pos)
                .map_err(game_error)?;
        }
        self.pending_level_hooks.custom_spawners += 1;
        Ok(())
    }
    /// Broadcast precedes block events; event-induced changes wait one tick.
    fn broadcast_block_changes(&mut self) -> Result<()> {
        let mut positions = self.level.level.take_block_update_chunks();
        let mut seen: HashSet<_> = positions.iter().copied().collect();
        while let Some(key) = self
            .level
            .generation()
            .with_world_mut(|world| world.light_chunks_to_broadcast.pop_first())
        {
            let pos = ChunkPos::unpack(key);
            if seen.insert(pos) {
                positions.push(pos);
            }
        }
        for pos in positions {
            if !self
                .level
                .level
                .chunk(pos)
                .is_some_and(|chunk| chunk.ticking_ready())
            {
                continue;
            }
            let packets = self
                .level
                .level
                .take_block_updates(pos)
                .map_err(game_error)?;
            let light = self.level.generation().with_world_mut(|world| {
                world.pending_light_updates.remove(&pos).map(|mask| {
                    cb::Packet::LightUpdate(cb::LightUpdate {
                        x: VarInt(pos.x),
                        z: VarInt(pos.z),
                        light_data: light_packet_with_masks(
                            world.lights.get(&pos.pack()),
                            mask.sky,
                            mask.block,
                        ),
                    })
                })
            });
            for c in self.connections.values_mut() {
                let Phase::Play(entity) = c.phase else {
                    continue;
                };
                if !c.close
                    && self
                        .level
                        .level
                        .entity::<ServerPlayer>(entity)
                        .map_err(game_error)?
                        .chunk_sender
                        .sent
                        .contains(&pos)
                {
                    if let Some(light) = &light {
                        c.push(light.clone());
                    }
                    for packet in &packets {
                        c.push(packet.clone());
                    }
                }
            }
        }
        Ok(())
    }
    fn apply_level_effects(&mut self) -> Result<()> {
        use ferraloom_game::level::effects::LevelEffect;
        let mut lighting = Vec::new();
        for effect in self.level.level.take_effects() {
            match effect {
                effect @ (LevelEffect::CheckLight(_) | LevelEffect::SectionEmptiness { .. }) => {
                    lighting.push(effect)
                }
                LevelEffect::WeatherEvent { event, param } => {
                    self.broadcast(cb::Packet::GameEvent(cb::GameEvent { event, param }))
                }
                LevelEffect::LevelEvent { pos, id, data } => {
                    self.broadcast(cb::Packet::LevelEvent(cb::LevelEvent {
                        event_type: id,
                        pos,
                        data,
                        global_event: false,
                    }))
                }
                LevelEffect::PoiChanged { .. } => self.pending_level_hooks.poi += 1,
                LevelEffect::BlockUpdated { .. } => self.pending_level_hooks.navigation += 1,
                LevelEffect::DestroyParticles { .. } => self.pending_level_hooks.particles += 1,
                LevelEffect::BlockDestroyed { .. } => self.pending_level_hooks.game_events += 1,
                LevelEffect::PendingPrecipitation(_) => {}
                LevelEffect::BroadcastAll(packet) => self.broadcast(*packet),
                LevelEffect::PlayerPacket { recipients, packet } => {
                    for entity in recipients {
                        for &connection in &self.players {
                            let c = self.connections.get_mut(&connection).unwrap();
                            if matches!(c.phase, Phase::Play(id) if id == entity) && !c.close {
                                c.push(*packet.clone());
                                break;
                            }
                        }
                    }
                }
                LevelEffect::PlayerSectionChange { player, section } => {
                    self.level.generation().update_player(player, Some(section));
                }
            }
        }
        if let Some(changes) = World::capture_block_light_changes(&self.level.level, lighting)? {
            self.level
                .generation()
                .with_world(|world| world.submit_block_light_changes(changes))?;
        }
        Ok(())
    }
    /// Apply accepted section transitions immediately, including Y-only moves.
    /// The commands branch supplies the current SpectatorsGenerateChunks rule
    /// when its canonical GameRules state lands (vanilla's default is true).
    pub(super) fn update_player_tickets(&mut self, entity: EntityRef) -> Result<()> {
        for (player, section) in self.level.level.take_player_section_changes() {
            self.level.generation().update_player(player, Some(section));
        }
        let section = ferraloom_util::SectionPos::from_position(self.player(entity)?.position);
        self.level.generation().update_player(entity, Some(section));
        Ok(())
    }
    fn update_chunk_tickets(&mut self) -> Result<()> {
        use ferraloom_game::level::acquisition::AcquireDriver;
        self.level
            .generation()
            .run_distance_manager_updates(&self.level.level)
            .map_err(game_error)?;
        Ok(())
    }
    fn integrate_jobs(&mut self) -> Result<()> {
        self.progress_chunk_jobs()?;
        self.level.finish_promotions()?;
        self.level.evict_unloaded_chunks()?;
        self.level.level.collect_chunk_structure_versions();
        self.level.level.finish_chunk_source_tick();
        Ok(())
    }
    fn progress_chunk_jobs(&mut self) -> Result<()> {
        self.level.generation().progress_jobs(&self.level.level)
    }
    fn place_player(&mut self, connection: ConnectionId) -> Result<()> {
        let mut c = self
            .connections
            .remove(&connection)
            .context("missing joining connection")?;
        let mut player = c.player.take().context("spawn not prepared")?;
        if player.saved.is_empty() {
            let position = Vec3::new(
                player.position.x,
                f64::from(self.level.generation().with_world(|world| world.spawn.y)),
                player.position.z,
            );
            player.set_pos(position);
            player.first_good = player.position;
            player.last_good = player.position;
        }
        let entity = self.level.level.next_entity_ref();
        let pos = player.position;
        let yaw = player.yaw;
        let pitch = player.pitch;
        let center = player.chunk_position();
        let mode = protocol_game_type(player.game_mode);
        let entity_id = player.network_id;
        let permission = self.shared().ops.permission_level(player.uuid).id();
        self.access.remember(&c.profile)?;
        c.phase = Phase::Play(entity);
        c.push(cb::Packet::Login(cb::Login {
            player_id: entity_id,
            hardcore: self.properties.boolean("hardcore"),
            levels: vec![id("overworld"), id("the_nether"), id("the_end")],
            max_players: VarInt(self.properties.int("max-players")),
            chunk_radius: VarInt(self.properties.int("view-distance").clamp(2, 32)),
            simulation_distance: VarInt(self.properties.int("simulation-distance").clamp(2, 32)),
            reduced_debug_info: self
                .game_rules
                .boolean(ferraloom_game::gamerules::GameRule::ReducedDebugInfo),
            show_death_screen: !self
                .game_rules
                .boolean(ferraloom_game::gamerules::GameRule::ImmediateRespawn),
            do_limited_crafting: self
                .game_rules
                .boolean(ferraloom_game::gamerules::GameRule::LimitedCrafting),
            common_player_spawn_info: CommonPlayerSpawnInfo {
                dimension_type: DimensionTypeId(
                    self.registries.numeric("dimension_type", "overworld"),
                ),
                dimension: id("overworld"),
                seed: ferraloom_worldgen::biome_manager::obfuscate_seed(
                    self.level.generation().with_world(|world| world.seed),
                ),
                game_type: mode,
                previous_game_type: OptionalGameType(None),
                is_debug: false,
                is_flat: false,
                last_death_location: None,
                portal_cooldown: VarInt(0),
                sea_level: VarInt(63),
            },
            online_mode: self.properties.boolean("online-mode"),
            enforces_secure_chat: self.enforces_secure_chat,
        }));
        c.push(self.difficulty_packet());
        c.push(abilities(&player));
        c.push(cb::Packet::SetHeldSlot(cb::SetHeldSlot {
            slot: VarInt(player.inventory.selected_slot() as i32),
        }));
        c.push(cb::Packet::UpdateRecipes(
            self.recipe_synchronization.update.clone(),
        ));
        c.push(cb::Packet::EntityEvent(cb::EntityEvent {
            entity_id,
            event_id: 24 + permission.clamp(0, 4) as i8,
        }));
        c.push(self.commands.packet(permission));
        player.stats.mark_all_dirty();
        let (settings, recipes) = player
            .recipe_book
            .initial_packets(&self.recipe_synchronization);
        c.push(cb::Packet::RecipeBookSettings(settings));
        c.push(cb::Packet::RecipeBookAdd(recipes));
        for packet in self.shared().scoreboard.join_packets() {
            c.push(packet);
        }
        self.broadcast(cb::Packet::SystemChat(Box::new(cb::SystemChat {
            content: Component::translatable(
                "multiplayer.player.joined",
                vec![protocol::chat::Argument::Component(Box::new(
                    player.display_name(),
                ))],
            ),
            overlay: false,
        })));
        c.push(
            player.teleport_before_registration(
                position_move_rotation(pos, Vec3::ZERO, yaw, pitch),
                0,
            ),
        );
        c.push(cb::Packet::ServerData(Box::new(cb::ServerData {
            motd: Component::literal(self.properties.get("motd")),
            icon_bytes: self.favicon.clone().map(common::ByteArray),
        })));
        let existing = self
            .players
            .iter()
            .filter_map(|id| self.connections.get(id))
            .filter_map(|c| match c.phase {
                Phase::Play(handle) => self
                    .level
                    .level
                    .entity::<ServerPlayer>(handle)
                    .ok()
                    .map(|p| player_info(&c.profile, p)),
                _ => None,
            })
            .collect();
        c.push(cb::Packet::PlayerInfoUpdate(cb::PlayerInfoUpdate {
            actions: 255,
            entries: existing,
        }));
        let initialize = cb::Packet::PlayerInfoUpdate(cb::PlayerInfoUpdate {
            actions: 255,
            entries: vec![player_info(&c.profile, &player)],
        });
        self.broadcast(initialize.clone());
        c.push(initialize);
        c.push(cb::Packet::InitializeBorder(
            self.level
                .level
                .entity_world_border()
                .initialization_packet(),
        ));
        c.push(self.clock_packet(None));
        c.push(cb::Packet::SetDefaultSpawnPosition(
            cb::SetDefaultSpawnPosition {
                respawn_data: self.default_spawn_data().packet_data(),
            },
        ));
        c.push(cb::Packet::GameEvent(cb::GameEvent {
            event: 13,
            param: 0.,
        }));
        c.push(cb::Packet::TickingState(cb::TickingState {
            tick_rate: self.tick_rate_manager.tickrate(),
            is_frozen: self.tick_rate_manager.is_frozen(),
        }));
        c.push(cb::Packet::TickingStep(cb::TickingStep {
            tick_steps: VarInt(self.tick_rate_manager.frozen_ticks_to_run()),
        }));
        c.push(cb::Packet::SetChunkCacheCenter(cb::SetChunkCacheCenter {
            x: VarInt(center.x),
            z: VarInt(center.z),
        }));
        c.center = Some(center);
        c.push(cb::Packet::PostEffects(common_cb::PostEffects {
            post_effects: Vec::new(),
        }));
        self.level
            .level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .map_err(game_error)?;
        ferraloom_game::inventory::initialize_inventory_menu(entity, &mut self.level.level)?;
        use protocol::entity_data::{DataValue, EntityDataItems, EntityDataValue};
        let post_initialization = {
            let player = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?;
            [
                cb::Packet::SetEntityData(cb::SetEntityData {
                    id: VarInt(entity_id),
                    packed_items: EntityDataItems(vec![
                        DataValue {
                            id: 9,
                            value: EntityDataValue::Float(player.parent.health()),
                        },
                        DataValue {
                            id: 16,
                            value: EntityDataValue::Byte(c.information.model_customisation as i8),
                        },
                    ]),
                }),
                cb::Packet::SetHealth(cb::SetHealth {
                    health: player.parent.health(),
                    food: VarInt(player.food_data.food_level()),
                    saturation: player.food_data.saturation_level(),
                }),
                cb::Packet::SetExperience(cb::SetExperience {
                    experience_progress: 0.,
                    experience_level: VarInt(0),
                    total_experience: VarInt(0),
                }),
            ]
        };
        ferraloom_game::inventory::send_container_packets(
            entity,
            post_initialization,
            &mut self.level.level,
        )?;
        println!(
            "{} joined at {}, {}, {}",
            c.profile.name, pos.x, pos.y, pos.z
        );
        self.update_player_tickets(entity)?;
        self.players.push(connection);
        self.connections.insert(connection, c);
        self.apply_level_effects()
    }
    async fn play_packet(&mut self, connection: ConnectionId, packet: sb::Packet) -> Result<()> {
        let Phase::Play(entity) = self.connections[&connection].phase else {
            bail!("play packet before configuration finished");
        };
        if matches!(
            &packet,
            sb::Packet::MovePlayerPos(_) | sb::Packet::MovePlayerPosRot(_)
        ) {
            let c = self.connections.get_mut(&connection).unwrap();
            ensure!(
                !c.received_position_this_tick,
                "multiplayer.disconnect.invalid_player_movement"
            );
            c.received_position_this_tick = true;
        }
        match packet {
            sb::Packet::ContainerClick(packet) => {
                self.handle_container_click(connection, entity, packet)?
            }
            sb::Packet::ContainerClose(_) => self.handle_container_close(connection, entity)?,
            sb::Packet::PlaceRecipe(packet) => {
                self.handle_place_recipe(connection, entity, packet)?
            }
            sb::Packet::RecipeBookChangeSettings(packet) => {
                self.handle_recipe_book_change_settings(entity, packet)?
            }
            sb::Packet::RecipeBookSeenRecipe(packet) => {
                self.handle_recipe_book_seen_recipe(entity, packet)?
            }
            sb::Packet::ContainerButtonClick(packet) => {
                self.handle_container_button_click(connection, entity, packet)?
            }
            sb::Packet::SetCreativeModeSlot(packet) => {
                self.handle_creative_slot(connection, entity, packet)?
            }
            sb::Packet::SetCarriedItem(packet) => {
                self.handle_set_carried_item(connection, entity, packet)?
            }
            sb::Packet::PlayerAction(packet)
                if matches!(
                    packet.action,
                    PlayerAction::DropItem
                        | PlayerAction::DropAllItems
                        | PlayerAction::SwapItemWithOffhand
                ) =>
            {
                self.handle_inventory_action(connection, entity, packet.action)?;
            }
            sb::Packet::PickItemFromBlock(packet) => {
                self.handle_pick_item_from_block(connection, entity, packet)?
            }
            sb::Packet::PickItemFromEntity(packet) => {
                self.handle_pick_item_from_entity(connection, entity, packet)?
            }
            sb::Packet::AcceptTeleportation(p) => {
                if p.id.0 == self.player(entity)?.teleport.awaiting_teleport {
                    if self.player(entity)?.teleport.awaiting_position.is_none()
                        || [p.x, p.y, p.z].iter().any(|v| v.is_nan())
                        || !p.y_rot.is_finite()
                        || !p.x_rot.is_finite()
                    {
                        bail!("multiplayer.disconnect.invalid_player_movement");
                    }
                    entity_teleport::accept_teleport(entity, p.id.0, &mut self.level.level)
                        .map_err(game_error)?;
                    self.update_player_tickets(entity)?;
                    self.movement(
                        connection,
                        entity,
                        Vec3::new(p.x, p.y, p.z),
                        (p.y_rot, p.x_rot),
                        false,
                        false,
                    )
                    .await?;
                }
            }
            sb::Packet::MovePlayerPos(p) => {
                let player = self.player(entity)?;
                let (yaw, pitch) = (player.yaw, player.pitch);
                self.movement(
                    connection,
                    entity,
                    Vec3::new(p.x, p.y, p.z),
                    (yaw, pitch),
                    p.flags.on_ground,
                    p.flags.horizontal_collision,
                )
                .await?;
            }
            sb::Packet::MovePlayerPosRot(p) => {
                self.movement(
                    connection,
                    entity,
                    Vec3::new(p.x, p.y, p.z),
                    (p.y_rot, p.x_rot),
                    p.flags.on_ground,
                    p.flags.horizontal_collision,
                )
                .await?
            }
            sb::Packet::MovePlayerRot(p) => {
                let position = self.player(entity)?.position;
                self.movement(
                    connection,
                    entity,
                    position,
                    (p.y_rot, p.x_rot),
                    p.flags.on_ground,
                    p.flags.horizontal_collision,
                )
                .await?;
            }
            sb::Packet::MovePlayerStatusOnly(p) => {
                self.level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .on_ground = p.flags.on_ground
            }
            sb::Packet::ChunkBatchReceived(p) => self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?
                .chunk_sender
                .acknowledge(p.desired_chunks_per_tick),
            sb::Packet::KeepAlive(p) => self.keepalive_reply(connection, p.id)?,
            sb::Packet::PlayerLoaded(_) => {
                self.level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .loaded = true
            }
            sb::Packet::UseItemOn(packet) => {
                self.handle_use_item_on(connection, entity, packet)?;
            }
            sb::Packet::SignUpdate(packet) => {
                self.handle_sign_update(connection, entity, packet)?;
            }
            sb::Packet::ClientInformation(p) => {
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                player.view_distance = i32::from(p.view_distance);
                player.text_filtering_enabled = p.text_filtering_enabled;
                self.connections.get_mut(&connection).unwrap().information = p;
            }
            sb::Packet::Chat(p) => {
                ensure!(
                    !p.message
                        .0
                        .chars()
                        .any(|c| c < ' ' || c == '§' || c == '\u{7f}'),
                    "multiplayer.disconnect.illegal_characters"
                );
                let is_operator = self.shared().ops.contains(self.player(entity)?.uuid);
                let c = self.connections.get_mut(&connection).unwrap();
                let last_seen = c
                    .last_seen
                    .apply(&p.last_seen_messages)
                    .context("multiplayer.disconnect.chat_validation_failed")?;
                c.last_action = Instant::now();
                c.chat_spam += 20;
                ensure!(
                    c.chat_spam
                        <= self
                            .properties
                            .int("chat-spam-threshold-seconds")
                            .saturating_mul(20)
                        || is_operator,
                    "disconnect.spam"
                );
                let message = if let Some(session) = &mut c.session {
                    session.unpack(
                        c.profile.id,
                        p.message.0,
                        p.timestamp,
                        p.salt,
                        p.signature,
                        last_seen,
                    )
                } else if self.enforces_secure_chat {
                    Err(anyhow::anyhow!("chat.disabled.missingProfileKey"))
                } else {
                    Ok(ValidatedMessage::unsigned(c.profile.id, p.message.0))
                };
                match message {
                    Ok(message) => self.broadcast_chat(message, id("chat"))?,
                    Err(e) => {
                        self.send_component(Some(connection), Component::literal(e.to_string()))
                    }
                }
            }
            sb::Packet::CommandSuggestion(request) => {
                self.connections
                    .get_mut(&connection)
                    .unwrap()
                    .command_suggestion = Some(request);
            }
            sb::Packet::ChatCommand(p) => {
                self.connections
                    .get_mut(&connection)
                    .unwrap()
                    .command_message = None;
                self.execute_command(Some(connection), &p.command).await?
            }
            sb::Packet::ChatCommandSigned(p) => {
                let source = self.player_command_source(entity).map_err(game_error)?;
                let c = self.connections.get_mut(&connection).unwrap();
                let last_seen = c
                    .last_seen
                    .apply(&p.last_seen_messages)
                    .context("multiplayer.disconnect.chat_validation_failed")?;
                let prepared = match self.commands.prepare(&p.command, source.clone()) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        self.command_syntax_failure(Some(connection), &source, &error, &p.command);
                        return Ok(());
                    }
                };
                if let Some((argument_name, message)) = prepared.message_argument() {
                    let content = message.text.clone();
                    let signature = p
                        .argument_signatures
                        .0
                        .iter()
                        .find(|argument| argument.name.0 == argument_name)
                        .map(|argument| argument.signature.clone());
                    if !p.argument_signatures.0.is_empty()
                        && (p.argument_signatures.0.len() != 1 || signature.is_none())
                    {
                        let mut message = Component::translatable(
                            "chat.disabled.invalid_command_signature",
                            vec![],
                        );
                        message.style.color =
                            Some(protocol::chat::TextColor::parse("red").unwrap());
                        c.push(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                            content: message,
                            overlay: false,
                        })));
                        return Ok(());
                    }
                    let validated = if let Some(session) = &mut c.session {
                        session.unpack(
                            c.profile.id,
                            content,
                            p.timestamp,
                            p.salt,
                            signature,
                            last_seen,
                        )
                    } else if self.enforces_secure_chat {
                        Err(anyhow::anyhow!("chat.disabled.missingProfileKey"))
                    } else {
                        Ok(ValidatedMessage::unsigned(c.profile.id, content))
                    };
                    let validated = match validated {
                        Ok(message) => message,
                        Err(error) => {
                            let mut message = Component::translatable(error.to_string(), vec![]);
                            message.style.color =
                                Some(protocol::chat::TextColor::parse("red").unwrap());
                            c.push(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                                content: message,
                                overlay: false,
                            })));
                            return Ok(());
                        }
                    };
                    c.command_message = Some((argument_name.to_owned(), validated));
                } else {
                    ensure!(
                        p.argument_signatures.0.is_empty(),
                        "unexpected signed command arguments"
                    );
                }
                self.execute_command(Some(connection), &p.command).await?;
            }
            sb::Packet::ChatSessionUpdate(p) => {
                if self.services.can_validate() {
                    let c = self.connections.get_mut(&connection).unwrap();
                    if c.session.as_ref().is_none_or(|session| {
                        session.data.profile_public_key != p.chat_session.profile_public_key
                    }) {
                        if let Some(old) = &c.session {
                            ensure!(
                                p.chat_session.profile_public_key.expires_at
                                    >= old.data.profile_public_key.expires_at,
                                "multiplayer.disconnect.expired_public_key"
                            );
                        }
                        let session =
                            ChatSession::new(c.profile.id, p.chat_session, &self.services)?;
                        let data = session.data.clone();
                        let profile_id = c.profile.id;
                        c.session = Some(session);
                        self.broadcast(cb::Packet::PlayerInfoUpdate(cb::PlayerInfoUpdate {
                            actions: 2,
                            entries: vec![PlayerInfoEntry {
                                profile_id,
                                actions: vec![PlayerInfoAction::InitializeChat {
                                    session: Some(data),
                                }],
                            }],
                        }));
                    }
                }
            }
            sb::Packet::ChatAck(p) => self
                .connections
                .get_mut(&connection)
                .unwrap()
                .last_seen
                .offset(p.offset.0)?,
            sb::Packet::PlayerAbilities(p) => {
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                if player.abilities.mayfly {
                    player.abilities.flying = p.is_flying;
                }
            }
            sb::Packet::PingRequest(p) => {
                self.connections
                    .get_mut(&connection)
                    .unwrap()
                    .push(cb::Packet::PongResponse(common_cb::PongResponse {
                        time: p.time,
                    }))
            }
            sb::Packet::CookieResponse(_) => {
                bail!("multiplayer.disconnect.unexpected_query_response")
            }
            sb::Packet::ClientTickEnd(_) => {
                let c = self.connections.get_mut(&connection).unwrap();
                if !c.received_movement_this_tick {
                    self.level
                        .level
                        .entity_mut::<ServerPlayer>(entity)
                        .map_err(game_error)?
                        .known_movement = Vec3::ZERO;
                }
                c.received_position_this_tick = false;
                c.received_movement_this_tick = false;
            }
            sb::Packet::PlayerInput(p) => {
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                player.last_client_input = p.input;
                if player.has_client_loaded() {
                    self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
                    player.set_shared_flag(1, p.input.shift);
                }
            }
            sb::Packet::SeenAdvancements(packet) => {
                ferraloom_game::advancements::seen_advancements(
                    &mut self.level.level,
                    entity,
                    &packet,
                )
                .map_err(game_error)?;
            }
            sb::Packet::ClientCommand(packet)
                if packet.action == play::ClientCommandAction::RequestStats =>
            {
                ferraloom_game::stats::request_stats(&mut self.level.level, entity)
                    .map_err(game_error)?;
            }
            sb::Packet::CustomPayload(_) | sb::Packet::Pong(_) | sb::Packet::PlayerCommand(_) => {}
            // These gameplay ports are outside the terrain/join milestone.
            _ => {}
        }
        Ok(())
    }
    /// Drain ServerPlayer#tick container output at the entity phase. The game
    /// callback already broadcast changes; this only routes its ordered packets.
    fn broadcast_container_changes(&mut self) -> Result<()> {
        for connection in self.players.clone() {
            let Some(c) = self.connections.get(&connection) else {
                continue;
            };
            let Phase::Play(entity) = c.phase else {
                continue;
            };
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            let packets = std::mem::take(&mut player.menus.pending_packets);
            for packet in packets {
                self.connections.get_mut(&connection).unwrap().push(packet);
            }
        }
        Ok(())
    }
    /// Port of ServerGamePacketListenerImpl#handleContainerClick.
    fn handle_container_click(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::ContainerClick,
    ) -> Result<()> {
        use ferraloom_game::inventory::{self, LiveRecipeMenuHooks, MenuSynchronization};
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let (menu, spectator, valid_slot) = {
            let player = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?;
            let menu = player.menus.current();
            (
                player.menus.current_ref(),
                player.game_mode == GameType::Spectator || player.parent.health() <= 0.0,
                menu.is_valid_slot_index(i32::from(packet.slot_num)),
            )
        };
        if menu.container_id != packet.container_id.0 {
            return Ok(());
        }
        if spectator {
            inventory::synchronize_menu(
                entity,
                menu,
                MenuSynchronization::Contents,
                &mut self.level.level,
            )?;
        } else {
            if !inventory::live_menu_still_valid(entity, menu, &self.level.level)? || !valid_slot {
                return Ok(());
            }
            let mismatch = {
                let receiver = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .menus
                    .menu_mut(menu)?;
                let mismatch = receiver.state_id() != packet.state_id.0;
                receiver.suppress_remote_updates();
                mismatch
            };
            let resources = self.level.level.container_resources().map_err(game_error)?;
            let kind = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .menus
                .menu(menu)?
                .kind;
            let mut hooks = LiveRecipeMenuHooks {
                resources,
                kind,
                limited_crafting: self
                    .game_rules
                    .boolean(ferraloom_game::gamerules::GameRule::LimitedCrafting),
            };
            inventory::clicked_live_menu(
                entity,
                menu,
                i32::from(packet.slot_num),
                i32::from(packet.button_num),
                packet.container_input,
                &mut hooks,
                &mut self.level.level,
            )?;
            // Java re-reads player.containerMenu after clicked: a callback may
            // have replaced it. The captured old receiver was rooted only for
            // clicked itself, and must never receive the new remote state.
            let current = {
                let menus = &mut self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .menus;
                let current = menus.current_ref();
                let receiver = menus.menu_mut(current)?;
                for changed in packet.changed_slots.0 {
                    receiver.set_remote_slot_unsafe(i32::from(changed.slot), changed.stack);
                }
                receiver.set_remote_carried(packet.carried_item);
                receiver.resume_remote_updates();
                current
            };
            inventory::synchronize_menu(
                entity,
                current,
                if mismatch {
                    MenuSynchronization::FullState
                } else {
                    MenuSynchronization::Changes
                },
                &mut self.level.level,
            )?;
        }
        self.apply_level_effects()
    }
    /// Port of ServerGamePacketListenerImpl#handleContainerClose and ServerPlayer#doCloseContainer.
    fn handle_container_close(
        &mut self,
        _connection: ConnectionId,
        entity: EntityRef,
    ) -> Result<()> {
        ferraloom_game::inventory::close_container_live(
            entity,
            &mut self.level.level,
            false,
            false,
        )?;
        self.apply_level_effects()
    }
    /// Port of ServerGamePacketListenerImpl#handleContainerButtonClick.
    fn handle_container_button_click(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::ContainerButtonClick,
    ) -> Result<()> {
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let may_build = {
            let player = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if player.game_mode == GameType::Spectator
                || player.menus.current().container_id != packet.container_id.0
            {
                return Ok(());
            }
            player.game_mode != GameType::Adventure
        };
        let (accepted, packets, drops) = ferraloom_game::inventory::with_block_menu_context(
            entity,
            &mut self.level.level,
            |menus, _book, context| {
                let menu = menus.current_mut();
                if !menu.still_valid(context) {
                    return Ok((false, Vec::new(), Vec::new()));
                }
                let accepted = menu.click_menu_button(
                    context,
                    packet.button_id.0,
                    may_build,
                    &mut UnsupportedMenuHooks,
                )?;
                Ok((
                    accepted,
                    menu.drain_packets(),
                    std::mem::take(&mut context.drops),
                ))
            },
        )?;
        ferraloom_game::inventory::send_container_packets(entity, packets, &mut self.level.level)?;
        for drop in drops {
            self.drop_inventory_stack(entity, drop.stack, drop.thrown_from_hand)?;
        }
        if accepted {
            ferraloom_game::inventory::broadcast_container_menu(entity, &mut self.level.level)
                .map_err(game_error)?;
        }
        self.apply_level_effects()
    }
    /// Port of ServerGamePacketListenerImpl#handlePlaceRecipe. Slot deltas are
    /// broadcast by ServerPlayer#tick; only direct result and ghost packets go now.
    fn handle_place_recipe(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::PlaceRecipe,
    ) -> Result<()> {
        use ferraloom_game::inventory::{self, LiveRecipeMenuHooks, MenuKind};
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let (menu, allow_dropping) = {
            let player = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if player.game_mode == GameType::Spectator
                || player.menus.current().container_id != packet.container_id.0
            {
                return Ok(());
            }
            (
                player.menus.current_ref(),
                player.game_mode == GameType::Creative,
            )
        };
        if !inventory::live_menu_still_valid(entity, menu, &self.level.level)? {
            return Ok(());
        }
        let Some(display) = self
            .recipe_synchronization
            .get_recipe_from_display(packet.recipe)
        else {
            return Ok(());
        };
        let recipe = self
            .recipe_manager
            .by_key(&display.parent)
            .context("recipe display without parent")?;
        let kind = {
            let player = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if !player.recipe_book.contains(&recipe.id) {
                return Ok(());
            }
            player.menus.menu(menu)?.kind
        };
        if !matches!(
            kind,
            MenuKind::Inventory
                | MenuKind::Crafting
                | MenuKind::Furnace
                | MenuKind::Smoker
                | MenuKind::BlastFurnace
        ) || recipe.value.placement_info().is_impossible_to_place()
        {
            return Ok(());
        }
        let mut hooks = LiveRecipeMenuHooks {
            resources: self.level.level.container_resources().map_err(game_error)?,
            kind,
            limited_crafting: self
                .game_rules
                .boolean(ferraloom_game::gamerules::GameRule::LimitedCrafting),
        };
        let action = inventory::place_live_menu_recipe(
            entity,
            menu,
            recipe,
            packet.use_max_items,
            allow_dropping,
            &mut hooks,
            &mut self.level.level,
        )?;
        if action == ferraloom_game::crafting::PostPlaceAction::PlaceGhostRecipe {
            let container_id = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .menus
                .current()
                .container_id;
            inventory::send_container_packets(
                entity,
                [cb::Packet::PlaceGhostRecipe(cb::PlaceGhostRecipe {
                    container_id: VarInt(container_id),
                    recipe_display: display.display.display.clone(),
                })],
                &mut self.level.level,
            )?;
        }
        self.apply_level_effects()
    }
    /// Port of ServerGamePacketListenerImpl#handleRecipeBookChangeSettingsPacket.
    fn handle_recipe_book_change_settings(
        &mut self,
        entity: EntityRef,
        packet: sb::RecipeBookChangeSettings,
    ) -> Result<()> {
        self.level
            .level
            .entity_mut::<ServerPlayer>(entity)
            .map_err(game_error)?
            .recipe_book
            .change_settings(packet.book_type, packet.is_open, packet.is_filtering);
        Ok(())
    }
    /// Port of ServerGamePacketListenerImpl#handleRecipeBookSeenRecipePacket.
    fn handle_recipe_book_seen_recipe(
        &mut self,
        entity: EntityRef,
        packet: sb::RecipeBookSeenRecipe,
    ) -> Result<()> {
        self.level
            .level
            .entity_mut::<ServerPlayer>(entity)
            .map_err(game_error)?
            .recipe_book
            .mark_display_seen(&self.recipe_synchronization, packet.recipe);
        Ok(())
    }
    /// Port of ServerGamePacketListenerImpl#handleSetCreativeModeSlot.
    fn handle_creative_slot(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::SetCreativeModeSlot,
    ) -> Result<()> {
        let stack = packet.item_stack.0;
        stack.validate_persistent()?;
        let valid_data = stack.is_empty()
            || ferraloom_game::entity::player::inventory::count(&stack)
                <= ferraloom_game::entity::player::inventory::max_stack_size(&stack);
        let tick = self.connections[&connection].network_ticks;
        let mut drop = None;
        let (synchronize, packets) = {
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if player.game_mode != GameType::Creative || !valid_data {
                return Ok(());
            }
            if (1..=45).contains(&packet.slot_num) {
                let infinite_materials = player.instabuild();
                let mut callbacks = PendingCraftingCallbacks;
                let mut hooks = RecipeMenuHooks {
                    manager: &self.recipe_manager,
                    context: self.crafting_context.as_ref(),
                    synchronization: &self.recipe_synchronization,
                    book: &mut player.recipe_book,
                    limited_crafting: Some(
                        self.game_rules
                            .boolean(ferraloom_game::gamerules::GameRule::LimitedCrafting),
                    ),
                    callbacks: &mut callbacks,
                };
                let mut context = MenuContext::player_with_abilities(
                    &mut player.inventory,
                    &mut player.parent.equipment,
                    true,
                    infinite_materials,
                );
                let menu = &mut player.menus.inventory_menu;
                menu.set_item_live(
                    &mut context,
                    packet.slot_num as usize,
                    stack.clone(),
                    &mut hooks,
                )?;
                menu.set_remote_slot(packet.slot_num as usize, &stack);
                (true, menu.drain_packets())
            } else {
                if packet.slot_num < 0 && player.menus.allow_creative_drop(tick) {
                    drop = Some(stack);
                }
                (false, Vec::new())
            }
        };
        ferraloom_game::inventory::send_container_packets(entity, packets, &mut self.level.level)?;
        if let Some(stack) = drop {
            self.drop_inventory_stack(entity, stack, true)?;
        }
        if synchronize {
            let menu = self
                .level
                .level
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .menus
                .inventory_menu
                .reference();
            ferraloom_game::inventory::synchronize_menu(
                entity,
                menu,
                ferraloom_game::inventory::MenuSynchronization::Changes,
                &mut self.level.level,
            )?;
        }
        self.apply_level_effects()
    }
    /// Port of ServerGamePacketListenerImpl#handleSetCarriedItem.
    fn handle_set_carried_item(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::SetCarriedItem,
    ) -> Result<()> {
        if (0..9).contains(&packet.slot) {
            self.level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?
                .inventory
                .set_selected_slot(packet.slot as usize);
            self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        }
        Ok(())
    }
    /// Port of handlePlayerAction's DROP_ITEM, DROP_ALL_ITEMS and SWAP_ITEM_WITH_OFFHAND cases.
    fn handle_inventory_action(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        action: PlayerAction,
    ) -> Result<()> {
        use ferraloom_game::entity::player::inventory::{count, split};
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let (drop, packets) = {
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if !player.loaded || player.game_mode == GameType::Spectator {
                return Ok(());
            }
            let inventory = &mut player.inventory;
            let drop = if action == PlayerAction::SwapItemWithOffhand {
                ferraloom_game::inventory::swap_player_hands(entity, &mut self.level.level)?;
                return ferraloom_game::entity::living::stop_using_item(
                    entity,
                    &mut self.level.level,
                )
                .map_err(game_error);
            } else {
                let amount = if action == PlayerAction::DropAllItems {
                    count(inventory.selected_item())
                } else {
                    1
                };
                let stack = split(inventory.selected_item_mut(), amount);
                let menu = player.menus.current_mut();
                if let Some(slot) = menu.slots.iter().position(|s| {
                    s.container == ferraloom_game::inventory::ContainerId::Player
                        && s.container_slot == inventory.selected_slot()
                }) {
                    menu.set_remote_slot(slot, inventory.selected_item());
                }
                Some(stack)
            };
            let menu = player.menus.current_mut();
            (drop, menu.drain_packets())
        };
        ferraloom_game::inventory::send_container_packets(entity, packets, &mut self.level.level)?;
        if let Some(stack) = drop {
            self.drop_inventory_stack(entity, stack, true)?;
        }
        ferraloom_game::inventory::broadcast_container_menu(entity, &mut self.level.level)
            .map_err(game_error)?;
        self.apply_level_effects()
    }
    fn drop_inventory_stack(
        &mut self,
        entity: EntityRef,
        stack: protocol::component::ItemStack,
        throw_randomly: bool,
    ) -> Result<()> {
        if !stack.is_empty() {
            ferraloom_game::entity::player::drop_item(
                entity,
                stack,
                throw_randomly,
                &mut self.level.level,
            )
            .map_err(game_error)?;
        }
        Ok(())
    }
    /// Port of ServerGamePacketListenerImpl#handlePickItemFromBlock.
    fn handle_pick_item_from_block(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::PickItemFromBlock,
    ) -> Result<()> {
        let player = self.player(entity)?;
        let reach = self.pick_interaction_range(entity, true)? + 1.;
        let eye = Vec3::new(
            player.position.x,
            player.position.y + f64::from(player.eye_height),
            player.position.z,
        );
        if ferraloom_util::Aabb::from_block(packet.pos).distance_to_sqr(eye) >= reach * reach
            || self
                .level
                .level
                .chunk(ChunkPos::containing(packet.pos))
                .is_none()
        {
            return Ok(());
        }
        let state = self
            .level
            .level
            .get_block_state(packet.pos)
            .map_err(game_error)?;
        let include_data = player.game_mode == GameType::Creative && packet.include_data;
        let stack = self
            .level
            .level
            .get_clone_item_stack(state, packet.pos, include_data)
            .map_err(game_error)?;
        if stack.is_empty() {
            return Ok(());
        }
        ensure!(
            !include_data || !state.has_block_entity(),
            "Unsupported: block entity components in picked item"
        );
        self.try_pick_item(connection, entity, stack)
    }
    /// Port of ServerGamePacketListenerImpl#handlePickItemFromEntity.
    fn handle_pick_item_from_entity(
        &mut self,
        connection: ConnectionId,
        player: EntityRef,
        packet: sb::PickItemFromEntity,
    ) -> Result<()> {
        let Some(entity) = self.level.level.entity_by_network_id(packet.id.0) else {
            return Ok(());
        };
        let source = self.player(player)?;
        let target = self
            .level
            .level
            .entity::<ferraloom_game::entity::EntityData>(entity)
            .map_err(game_error)?;
        let reach = self.pick_interaction_range(player, false)? + 3.;
        let eye = Vec3::new(
            source.position.x,
            source.position.y + f64::from(source.eye_height),
            source.position.z,
        );
        if target.removed || target.bounding_box.distance_to_sqr(eye) >= reach * reach {
            return Ok(());
        }
        let stack = self
            .level
            .level
            .entity_behavior(entity)
            .map_err(game_error)?
            .get_pick_result(entity, &self.level.level)
            .map_err(game_error)?;
        if let Some(stack) = stack
            && !stack.is_empty()
        {
            self.try_pick_item(connection, player, stack)?;
        }
        Ok(())
    }
    /// Uses the extracted Player attribute base until the attributes workstream
    /// lands. Modified interaction ranges remain explicit Unsupported.
    fn pick_interaction_range(&self, entity: EntityRef, block: bool) -> Result<f64> {
        use ferraloom_game::entity::player::inventory::component;
        use protocol::component::{ComponentTypeId, DataComponent};
        let player = self.player(entity)?;
        let attribute = if block {
            ferraloom_data::registries::attribute::Id::BLOCK_INTERACTION_RANGE
        } else {
            ferraloom_data::registries::attribute::Id::ENTITY_INTERACTION_RANGE
        };
        for stack in player
            .inventory
            .items
            .iter()
            .chain(player.parent.equipment.iter().map(|(_, stack)| stack))
        {
            if let Some(DataComponent::AttributeModifiers(modifiers)) =
                component(stack, ComponentTypeId::ATTRIBUTE_MODIFIERS)
            {
                ensure!(
                    !modifiers
                        .iter()
                        .any(|m| m.attribute.0.as_str() == attribute.name()),
                    "Unsupported: modified interaction range"
                );
            }
        }
        Ok(EntityType::PLAYER
            .data()
            .attributes
            .iter()
            .find(|(id, _)| *id == attribute)
            .expect("Player interaction range attribute")
            .1)
    }
    /// Port of ServerGamePacketListenerImpl#tryPickItem.
    fn try_pick_item(
        &mut self,
        _connection: ConnectionId,
        entity: EntityRef,
        stack: protocol::component::ItemStack,
    ) -> Result<()> {
        ferraloom_game::inventory::try_pick_item_live(entity, stack, &mut self.level.level)?;
        self.apply_level_effects()
    }
    fn player(&self, entity: EntityRef) -> Result<&ServerPlayer> {
        self.level
            .level
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)
    }
    /// Port of ServerGamePacketListenerImpl#teleport. Gameplay owns the single
    /// pending teleport; the runtime immediately updates tickets and appends the
    /// already-built packet to the connection's ordinary ordered output.
    pub(super) fn teleport_player(
        &mut self,
        entity: EntityRef,
        destination: PositionMoveRotation,
        relatives: i32,
    ) -> Result<()> {
        entity_teleport::teleport_player(entity, destination, relatives, &mut self.level.level)
            .map_err(game_error)?;
        self.update_player_tickets(entity)?;
        self.apply_level_effects()
    }
    async fn movement(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        target: Vec3,
        rotation: (f32, f32),
        on_ground: bool,
        horizontal_collision: bool,
    ) -> Result<()> {
        let (yaw, pitch) = rotation;
        if [target.x, target.y, target.z].iter().any(|v| v.is_nan())
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            bail!("multiplayer.disconnect.invalid_player_movement");
        }
        let player = self.player(entity)?;
        if let Some(position) = player.teleport.awaiting_position {
            if player.teleport.resend_due(player.connection_ticks) {
                let destination =
                    position_move_rotation(position, Vec3::ZERO, player.yaw, player.pitch);
                self.teleport_player(entity, destination, 0)?;
            }
            self.level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?
                .abs_snap_rotation_to(
                    ferraloom_util::mth::wrap_degrees_f32(yaw),
                    ferraloom_util::mth::wrap_degrees_f32(pitch),
                );
            return Ok(());
        }
        let player = self
            .level
            .level
            .entity_mut::<ServerPlayer>(entity)
            .map_err(game_error)?;
        player.teleport.awaiting_teleport_time = player.connection_ticks as i32;
        let origin = self.player(entity)?.position;
        if target.distance_to_sqr(origin) > 10000. {
            let player = self.player(entity)?;
            let (yaw, pitch) = (player.yaw, player.pitch);
            self.teleport_player(
                entity,
                position_move_rotation(origin, Vec3::ZERO, yaw, pitch),
                0,
            )?;
            return Ok(());
        }
        let center = ChunkPos::containing(BlockPos::new(
            target.x.floor() as i32,
            target.y.floor() as i32,
            target.z.floor() as i32,
        ));
        self.request_area(center, 1)?;
        // getChunk(..., true) waits at this visibility point, never treats missing ownership as air.
        while (-1..=1).any(|x| {
            (-1..=1).any(|z| {
                self.level
                    .level
                    .chunk(ChunkPos::new(center.x + x, center.z + z))
                    .is_none()
            })
        }) {
            self.integrate_jobs()?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let checks = MovementChecks::dedicated(&self.tick_rate_manager);
        let result = move_player_with_checks(
            &mut self.level.level,
            entity,
            PlayerMovement {
                position: target,
                yaw,
                pitch,
                on_ground,
                horizontal_collision,
            },
            &checks,
        )
        .map_err(game_error)?;
        match result {
            MovementResult::Invalid => bail!("multiplayer.disconnect.invalid_player_movement"),
            MovementResult::TooQuickly | MovementResult::Wrongly => {
                let player = self.player(entity)?;
                let (position, yaw, pitch) = (player.position, player.yaw, player.pitch);
                self.teleport_player(
                    entity,
                    position_move_rotation(position, Vec3::ZERO, yaw, pitch),
                    0,
                )?;
            }
            MovementResult::Accepted => {
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                player.known_movement = player.position - origin;
                let moved = player.known_movement.length_sqr() > 9.999999747378752E-6;
                self.update_player_tickets(entity)?;
                let c = self.connections.get_mut(&connection).unwrap();
                c.received_movement_this_tick = true;
                if moved {
                    c.last_action = Instant::now();
                }
            }
        }
        Ok(())
    }
    fn track_players(&mut self) -> Result<()> {
        let players = self.players.clone();
        for connection in players {
            let Some(c) = self.connections.get(&connection) else {
                continue;
            };
            if c.close {
                continue;
            }
            let Phase::Play(entity) = c.phase else {
                continue;
            };
            self.update_player_tickets(entity)?;
            let player = self.player(entity)?;
            let center = player.chunk_position();
            let distance = player
                .view_distance
                .clamp(2, self.properties.int("view-distance").clamp(2, 32));
            let desired: HashSet<_> = tracking_view(center, distance).into_iter().collect();
            self.request_area(center, distance + 2)?;
            let c = self.connections.get_mut(&connection).unwrap();
            if c.center != Some(center) {
                c.push(cb::Packet::SetChunkCacheCenter(cb::SetChunkCacheCenter {
                    x: VarInt(center.x),
                    z: VarInt(center.z),
                }));
                c.center = Some(center);
            }
            let player = self
                .level
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            let mut leaving = player
                .chunk_tracking_view
                .difference(&desired)
                .copied()
                .collect::<Vec<_>>();
            leaving.sort_by_key(|p| p.pack());
            for pos in leaving {
                if player.chunk_sender.drop_chunk(pos) {
                    c.push(cb::Packet::ForgetLevelChunk(cb::ForgetLevelChunk {
                        pos: play::ChunkPos { x: pos.x, z: pos.z },
                    }));
                }
            }
            for &pos in desired.difference(&player.chunk_tracking_view) {
                player.chunk_sender.pending.insert(pos);
            }
            player.chunk_tracking_view = desired;
        }
        Ok(())
    }
    async fn tick_connections(&mut self) -> Result<()> {
        self.process_command_suggestions()?;
        let now = Instant::now();
        let mut joining = Vec::new();
        for (&id, c) in &mut self.connections {
            if c.close {
                continue;
            }
            if c.ack_block_changes_up_to > -1 {
                c.push(cb::Packet::BlockChangedAck(cb::BlockChangedAck {
                    sequence: VarInt(c.ack_block_changes_up_to),
                }));
                c.ack_block_changes_up_to = -1;
            }
            c.network_ticks += 1;
            if c.network_ticks.is_multiple_of(20) {
                let received = (c.incoming_sequence - c.rate_packets) as f32;
                c.rate_packets = c.incoming_sequence;
                c.average_packets = received + 0.75 * (c.average_packets - received);
                let limit = self.properties.int("rate-limit");
                if limit > 0 && c.average_packets > limit as f32 {
                    c.disconnect("disconnect.exceeded_packet_rate");
                    continue;
                }
            }
            c.chat_spam = (c.chat_spam - 1).max(0);
            c.command_spam = (c.command_spam - 1).max(0);
            if now.duration_since(c.last_keepalive) >= Duration::from_secs(15) {
                if c.keepalive.is_some() {
                    c.disconnect("disconnect.timeout");
                    continue;
                }
                let key = time::OffsetDateTime::now_utc()
                    .unix_timestamp_nanos()
                    .div_euclid(1_000_000) as i64;
                c.keepalive = Some(key);
                c.last_keepalive = now;
                match c.phase {
                    Phase::Configuration(_) => {
                        c.push(cfg::ClientboundPacket::KeepAlive(common_cb::KeepAlive {
                            id: key,
                        }))
                    }
                    Phase::Play(_) => {
                        c.push(cb::Packet::KeepAlive(common_cb::KeepAlive { id: key }))
                    }
                }
            }
            match c.phase {
                Phase::Configuration(ConfigTask::Preparing) => {
                    let player = c.player.as_ref().unwrap();
                    let center = player.chunk_position();
                    if (-3..=3).all(|x| {
                        (-3..=3).all(|z| {
                            self.level.generation().with_world(|world| {
                                world
                                    .light_ready
                                    .contains(&ChunkPos::new(center.x + x, center.z + z))
                            })
                        })
                    }) {
                        joining.push(id);
                    }
                }
                Phase::Configuration(ConfigTask::Finishing) => {
                    let center = c.player.as_ref().unwrap().chunk_position();
                    self.level
                        .generation()
                        .with_tickets_mut(|tickets| tickets.add_player_spawn(center, self.tick));
                }
                Phase::Play(entity) => {
                    if !self.was_paused {
                        let player = self
                            .level
                            .level
                            .entity_mut::<ServerPlayer>(entity)
                            .map_err(game_error)?;
                        player.reset_position();
                        player.old_position = player.position;
                        ferraloom_game::entity::player::player_do_tick_with_tick_rate(
                            entity,
                            &mut self.level.level,
                            &self.tick_rate_manager,
                        )
                        .map_err(game_error)?;
                        let player = self
                            .level
                            .level
                            .entity::<ServerPlayer>(entity)
                            .map_err(game_error)?;
                        let (position, yaw, pitch) = (player.first_good, player.yaw, player.pitch);
                        ferraloom_game::entity::teleport::abs_snap_to(
                            entity,
                            position,
                            yaw,
                            pitch,
                            &mut self.level.level,
                        )
                        .map_err(game_error)?;
                        self.level
                            .level
                            .entity_mut::<ServerPlayer>(entity)
                            .map_err(game_error)?
                            .finish_connection_tick();
                    }
                    let idle = self.idle_timeout;
                    if idle > 0
                        && now.duration_since(c.last_action) > Duration::from_secs(idle as u64 * 60)
                    {
                        c.disconnect("multiplayer.disconnect.idling");
                    }
                }
                _ => {}
            }
        }
        for id in joining {
            let c = self.connections.get_mut(&id).unwrap();
            self.level.generation().with_tickets_mut(|tickets| {
                tickets.add_player_spawn(c.player.as_ref().unwrap().chunk_position(), self.tick)
            });
            c.phase = Phase::Configuration(ConfigTask::Finishing);
            c.push(cfg::ClientboundPacket::FinishConfiguration(
                cfg::FinishConfiguration {},
            ));
        }
        self.broadcast_container_changes()?;
        self.apply_level_effects()?;
        Ok(())
    }
    fn keepalive_reply(&mut self, connection: ConnectionId, key: i64) -> Result<()> {
        let c = self.connections.get_mut(&connection).unwrap();
        ensure!(c.keepalive == Some(key), "disconnect.timeout");
        c.keepalive = None;
        Ok(())
    }
    fn send_chunks(&mut self) -> Result<()> {
        let ready = self.level.generation().with_world(|world| {
            world
                .ready
                .intersection(&world.light_ready)
                .copied()
                .collect::<HashSet<_>>()
        });
        let mut batches = Vec::new();
        for (&connection, c) in &mut self.connections {
            if c.close {
                continue;
            }
            if let Phase::Play(entity) = c.phase {
                let positions = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .chunk_sender
                    .next_batch(c.center.unwrap(), |pos| {
                        // prepareTickingChunk already awaited the FULL neighborhood
                        // and post-processing; sending does not await its neighbors'
                        // own ticking futures. Retain the center light dependency.
                        ready.contains(&pos)
                    });
                if !positions.is_empty() {
                    batches.push((connection, positions));
                }
            }
        }
        for (connection, positions) in batches {
            let mut packets = vec![cb::Packet::ChunkBatchStart(cb::ChunkBatchStart {})];
            for &pos in &positions {
                let chunk = self
                    .level
                    .level
                    .chunk(pos)
                    .context("ready chunk not owned")?;
                use ferraloom_game::level::heightmap::Heightmap as LiveHeightmap;
                packets.push(cb::Packet::LevelChunkWithLight(cb::LevelChunkWithLight {
                    x: pos.x,
                    z: pos.z,
                    chunk_data: LevelChunkPacketData {
                        heightmaps: vec![
                            Heightmap {
                                kind: HeightmapType::WorldSurface,
                                data: heightmap(
                                    chunk
                                        .heightmap(LiveHeightmap::WorldSurface)
                                        .context("unprimed world surface")?,
                                ),
                            },
                            Heightmap {
                                kind: HeightmapType::MotionBlocking,
                                data: heightmap(
                                    chunk
                                        .heightmap(LiveHeightmap::MotionBlocking)
                                        .context("unprimed motion blocking")?,
                                ),
                            },
                        ],
                        sections: chunk.sections().map(LevelChunkSectionData::from).collect(),
                        block_entities: self
                            .level
                            .level
                            .block_entity_chunk_packet_info(pos)
                            .map_err(game_error)?,
                    },
                    light_data: self
                        .level
                        .generation()
                        .with_world(|world| light_packet(world.lights.get(&pos.pack()))),
                }));
            }
            packets.push(cb::Packet::ChunkBatchFinished(cb::ChunkBatchFinished {
                batch_size: VarInt(positions.len() as i32),
            }));
            let c = self.connections.get_mut(&connection).unwrap();
            for packet in packets {
                c.push(packet);
            }
        }
        Ok(())
    }
    fn flush_outbound(&mut self) {
        for c in self.connections.values_mut() {
            if c.pending.is_empty() && !c.close {
                continue;
            }
            let batch = OutboundBatch {
                sequence: c.outgoing_sequence,
                packets: std::mem::take(&mut c.pending),
                close: c.close,
            };
            match c.outbound.try_send(batch) {
                Ok(()) => c.outgoing_sequence += 1,
                Err(_) => {
                    c.close = true;
                }
            }
        }
    }
    fn broadcast_chat(
        &mut self,
        message: ValidatedMessage,
        chat_type: protocol::Identifier,
    ) -> Result<()> {
        let name = self
            .connections
            .values()
            .find(|c| c.profile.id == message.sender)
            .map(|c| c.profile.name.clone())
            .unwrap_or_default();
        let display_name = self
            .connections
            .values()
            .find(|c| c.profile.id == message.sender)
            .and_then(|c| {
                if let Phase::Play(entity) = c.phase {
                    self.player(entity).ok()
                } else {
                    None
                }
            })
            .map_or_else(|| Component::literal(&name), ServerPlayer::display_name);
        println!("<{name}> {}", message.content);
        for c in self.connections.values_mut() {
            if !matches!(c.phase, Phase::Play(_))
                || c.close
                || c.information.chat_visibility != common::serverbound::ChatVisibility::Full
            {
                continue;
            }
            if let Some(signature) = &message.signature
                && let Err(e) = c.last_seen.add_pending(signature)
            {
                c.disconnect(e.to_string());
                continue;
            }
            let global_index = c.next_chat_index;
            c.next_chat_index += 1;
            let body = c.signature_cache.pack(&message);
            c.push(cb::Packet::PlayerChat(Box::new(cb::PlayerChat {
                global_index: VarInt(global_index),
                sender: message.sender,
                index: VarInt(message.index),
                signature: message.signature.clone(),
                body,
                unsigned_content: None,
                filter_mask: FilterMask::PassThrough,
                chat_type: ChatTypeBound {
                    chat_type: ChatTypeHolder(Holder::Reference(chat_type.clone())),
                    name: display_name.clone(),
                    target_name: None,
                },
            })));
        }
        Ok(())
    }
    fn broadcast(&mut self, packet: cb::Packet) {
        for c in self.connections.values_mut() {
            if matches!(c.phase, Phase::Play(_)) && !c.close {
                c.push(packet.clone());
            }
        }
    }
    fn time_packet(&self) -> cb::Packet {
        cb::Packet::SetTime(ferraloom_game::commands::clocks::game_time_sync_packet(
            self.level.game_time,
        ))
    }
    fn refresh_status(&mut self) {
        use rand::{Rng, seq::SliceRandom};
        let mut random = rand::thread_rng();
        let sample_size = self.players.len().min(12);
        let offset = random.gen_range(0..=self.players.len() - sample_size);
        let sample = if self.properties.boolean("hide-online-players") {
            Vec::new()
        } else {
            let mut sample = self
                .players
                .iter()
                .skip(offset)
                .take(sample_size)
                .filter_map(|id| self.connections.get(id))
                .map(|c| status::NameAndId {
                    id: if c.information.allows_listing {
                        c.profile.id
                    } else {
                        Uuid::NIL
                    },
                    name: if c.information.allows_listing {
                        c.profile.name.clone()
                    } else {
                        "Anonymous Player".into()
                    },
                })
                .collect::<Vec<_>>();
            sample.shuffle(&mut random);
            sample
        };
        self.status.send_replace(make_status(
            &self.properties,
            self.players.len() as i32,
            sample,
            self.favicon.clone(),
            self.enforces_secure_chat,
        ));
    }
    async fn remove_player(&mut self, connection: ConnectionId) -> Result<()> {
        if let Some(c) = self.connections.remove(&connection)
            && let Phase::Play(entity) = c.phase
        {
            let player = self.player(entity)?;
            let progress_path = self
                .level
                .generation()
                .with_world(|world| world.access.path().to_path_buf());
            let progress =
                ferraloom_game::advancements::PlayerProgressSave::capture(player, &progress_path)?;
            let tag = player.save();
            let uuid = player.uuid.to_string();
            let name = player.display_name();
            let access = self
                .level
                .generation()
                .with_world(|world| world.access.clone());
            tokio::task::spawn_blocking(move || -> Result<()> {
                access.player_data().save(&uuid, &tag)?;
                progress.write()?;
                Ok(())
            })
            .await??;
            self.level.generation().update_player(entity, None);
            self.level.level.remove_entity(entity).map_err(game_error)?;
            self.players.retain(|&id| id != connection);
            self.broadcast(cb::Packet::PlayerInfoRemove(cb::PlayerInfoRemove {
                profile_ids: vec![c.profile.id],
            }));
            self.broadcast(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                content: Component::translatable(
                    "multiplayer.player.left",
                    vec![protocol::chat::Argument::Component(Box::new(name))],
                ),
                overlay: false,
            })));
        }
        Ok(())
    }
    async fn execute_command_inner(
        &mut self,
        source: Option<ConnectionId>,
        text: &str,
    ) -> Result<()> {
        let mut stack = if let Some(id) = source {
            let Phase::Play(entity) = self
                .connections
                .get(&id)
                .context("unknown command source")?
                .phase
            else {
                bail!("command source not playing");
            };
            let stack = self.player_command_source(entity).map_err(game_error)?;
            let is_operator = self.shared().ops.contains(self.player(entity)?.uuid);
            let c = self.connections.get_mut(&id).unwrap();
            c.command_spam += 20;
            c.last_action = Instant::now();
            ensure!(
                c.command_spam
                    <= self
                        .properties
                        .int("command-spam-threshold-seconds")
                        .saturating_mul(20)
                    || is_operator,
                "disconnect.spam"
            );
            stack
        } else {
            let spawn = self.level.generation().with_world(|world| world.spawn);
            CommandSourceStack::server(
                4,
                Vec3::new(f64::from(spawn.x), f64::from(spawn.y), f64::from(spawn.z)),
            )
        };
        if let Some(connection) = source {
            let signed_argument = self
                .connections
                .get_mut(&connection)
                .unwrap()
                .command_message
                .take();
            if let Some((argument_name, message)) = signed_argument {
                stack.signing_context.arguments = Arc::from([(
                    argument_name,
                    ferraloom_game::commands::source::SignedArgument {
                        sender: message.sender,
                        index: message.index,
                        signature: message.signature,
                        content: message.content,
                        timestamp: message.timestamp,
                        salt: message.salt,
                        last_seen: Arc::from(message.last_seen),
                    },
                )]);
            }
        }
        let prepared = match self.commands.prepare(text, stack.clone()) {
            Ok(command) => command,
            Err(error) => {
                self.command_syntax_failure(source, &stack, &error, text);
                return Ok(());
            }
        };
        use ferraloom_game::{
            commands::{
                execution::{self, ExecutionContext, QueueResult},
                source::CommandResultCallback,
            },
            gamerules::GameRule,
        };
        let mut queue = ExecutionContext::new(
            self.game_rules
                .integer(GameRule::MaxCommandSequenceLength)
                .max(1),
            self.game_rules.integer(GameRule::MaxCommandForks),
        );
        let frame = queue.top_frame(CommandResultCallback::default());
        execution::queue_command(&mut queue, frame, prepared, false);
        let mut pending = None;
        loop {
            match queue.run_next(&mut pending) {
                Ok(false) => break,
                Err(QueueResult::CommandLimit) => {
                    println!(
                        "Command execution stopped due to limit (executed {} commands)",
                        queue.command_limit()
                    );
                    break;
                }
                Err(QueueResult::QueueOverflow) => {
                    eprintln!("Command execution stopped due to command queue overflow (max {})", ExecutionContext::<Option<ferraloom_game::commands::PreparedCommand>>::MAX_QUEUE_DEPTH);
                    break;
                }
                Err(QueueResult::Complete) => unreachable!(),
                Ok(true) => {}
            }
            let Some(prepared) = pending.take() else {
                continue;
            };
            let stack = prepared.source.clone();
            let action = Commands::action(prepared);
            match action {
                CommandAction::Run(command) => {
                    match self.run_level_command(source, &command).await {
                        Ok(true) => continue,
                        Ok(false) => {}
                        Err(error) => {
                            if let Some(syntax) =
                                error.downcast_ref::<ferraloom_brigadier::CommandSyntaxException>()
                            {
                                self.command_failure(
                                    source,
                                    &stack,
                                    ferraloom_game::commands::exceptions::error_component(syntax),
                                );
                                stack.callback.on_failure();
                                continue;
                            }
                            return Err(error);
                        }
                    }
                    if let Err(error) = self.run_builtin(source, *command) {
                        self.command_failure(
                            source,
                            &stack,
                            ferraloom_game::commands::exceptions::error_component(&error),
                        );
                        stack.callback.on_failure();
                    }
                }
                CommandAction::Stop => {
                    self.command_success(
                        source,
                        &stack,
                        Component::translatable("commands.stop.stopping", vec![]),
                        true,
                    );
                    self.stop = true;
                    stack.callback.on_success(1);
                }
                CommandAction::Save { flush } => {
                    self.command_success(
                        source,
                        &stack,
                        Component::translatable("commands.save.saving", vec![]),
                        false,
                    );
                    if self.save_all(flush, true).await.is_ok() {
                        self.command_success(
                            source,
                            &stack,
                            Component::translatable("commands.save.success", vec![]),
                            true,
                        );
                        stack.callback.on_success(1);
                    } else {
                        self.command_failure(
                            source,
                            &stack,
                            Component::translatable("commands.save.failed", vec![]),
                        );
                        stack.callback.on_failure();
                    }
                }
            }
        }
        Ok(())
    }

    async fn save_all(&mut self, flush: bool, force: bool) -> Result<()> {
        let lent = self.lend_server_shared();
        let result = self.save_all_inner(flush, force).await;
        self.return_server_shared(lent);
        result
    }
    async fn save_all_inner(&mut self, flush: bool, force: bool) -> Result<()> {
        self.finish_generation_layer().await?;
        self.integrate_jobs()?;
        let context = crate::world::LiveLevelSaveContext::capture(&self.level.level)?;
        let save_context = self
            .level
            .generation()
            .with_world_mut(|world| world.begin_save_live_level_context(context))?;
        save_context.await??;
        self.save_live_entities(flush).await?;

        self.save_game_rules().await?;
        self.save_command_storage().await?;
        self.save_command_tickets().await?;
        let positions = self
            .level
            .generation()
            .holder_positions()
            .into_iter()
            .filter(|_| self.saving_enabled || force)
            .filter(|&pos| self.level.level.chunk(pos).is_some())
            .collect::<Vec<_>>();
        let mut snapshots = Vec::new();
        for pos in positions {
            snapshots.push(
                self.level
                    .capture_chunk_for_save(pos, self.level.game_time)?,
            );
        }
        let encode = self
            .level
            .generation()
            .with_world(|world| world.begin_encode_saves(snapshots));
        let encoded = encode.await??;
        for (pos, tag) in encoded {
            let save = self
                .level
                .generation()
                .with_world(|world| world.io.store(pos, Some(tag)));
            save.await?;
        }
        let progress_path = self
            .level
            .generation()
            .with_world(|world| world.access.path().to_path_buf());
        let progress = self
            .players
            .iter()
            .filter_map(|id| self.connections.get(id))
            .filter_map(|connection| match connection.phase {
                Phase::Play(entity) => Some(entity),
                _ => None,
            })
            .map(|entity| {
                Ok(ferraloom_game::advancements::PlayerProgressSave::capture(
                    self.player(entity)?,
                    &progress_path,
                )?)
            })
            .collect::<Result<Vec<_>>>()?;
        let players = self
            .players
            .iter()
            .filter_map(|id| self.connections.get(id))
            .filter_map(|c| {
                if let Phase::Play(entity) = c.phase {
                    Some(entity)
                } else {
                    None
                }
            })
            .map(|entity| {
                let player = self.player(entity)?;
                Ok((player.uuid.to_string(), player.save()))
            })
            .collect::<Result<Vec<_>>>()?;
        let bootstrap = self
            .level
            .generation()
            .with_world_mut(|world| world.take_bootstrap_save_snapshots(self.level.game_time));
        let encode = self
            .level
            .generation()
            .with_world(|world| world.begin_encode_saves(bootstrap));
        for (pos, tag) in encode.await?? {
            let save = self
                .level
                .generation()
                .with_world(|world| world.io.store(pos, Some(tag)));
            save.await?;
        }
        let save = self.level.generation().with_world_mut(|world| {
            world.begin_save_level_and_players(self.level.game_time, players)
        });
        save.await??;
        tokio::task::spawn_blocking(move || progress.into_iter().try_for_each(|save| save.write()))
            .await??;
        let synchronize = self
            .level
            .generation()
            .with_world(|world| world.io.synchronize(flush));
        synchronize.await?;
        Ok(())
    }
    async fn finish_generation_layer(&mut self) -> Result<()> {
        while self
            .level
            .generation()
            .with_world(World::generation_running)
        {
            self.level
                .generation()
                .with_world_mut(World::poll_running_generation)?;
            if self
                .level
                .generation()
                .with_world(World::generation_running)
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        Ok(())
    }
    async fn save_live_entities(&mut self, flush: bool) -> Result<()> {
        if !flush {
            self.level.level.auto_save_entities()?;
            let requests = self.level.level.take_entity_storage_requests();
            self.level
                .generation()
                .with_world_mut(|world| world.submit_entity_requests(requests, &self.properties))?;
            return Ok(());
        }
        self.level.level.begin_entity_save_all()?;
        loop {
            let requests = self.level.level.take_entity_storage_requests();
            self.level
                .generation()
                .with_world_mut(|world| world.submit_entity_requests(requests, &self.properties))?;
            let sync = self.level.generation().synchronize_entities(false)?;
            sync.await?;
            self.level
                .generation()
                .poll_entity_progress(&self.level.level)?;
            if self.level.level.poll_entity_save_all()? {
                let requests = self.level.level.take_entity_storage_requests();
                self.level.generation().with_world_mut(|world| {
                    world.submit_entity_requests(requests, &self.properties)
                })?;
                let sync = self.level.generation().synchronize_entities(true)?;
                sync.await?;
                while self
                    .level
                    .generation()
                    .poll_entity_progress(&self.level.level)?
                {}
                return Ok(());
            }
        }
    }
    async fn stop_server(&mut self) -> Result<()> {
        for c in self.connections.values_mut() {
            c.disconnect("multiplayer.disconnect.server_shutdown");
        }
        self.save_all(true, true).await?;
        self.flush_outbound();
        // Writers consume their ordered shutdown batch before channels are released.
        self.connections.clear();
        self.shutdown.send_replace(true);
        if let Some(listener) = self.listener_task.take() {
            listener.await??;
        }
        let close_entities = self
            .level
            .generation()
            .with_world_mut(World::begin_close_entity_storage);
        if let Some(close) = close_entities {
            close.await?;
        }
        self.finish_generation_layer().await?;
        let drain = self
            .level
            .generation()
            .with_world_mut(World::prepare_close_chunk_storage)?;
        for save in drain.proto {
            save.await?;
        }
        for save in drain.chunks {
            save.await??;
        }
        let close_chunks = self
            .level
            .generation()
            .with_world_mut(World::begin_close_chunk_storage)?;
        close_chunks.await?;
        println!(
            "Server stopped; world and players saved to {}",
            self.root.display()
        );
        Ok(())
    }
}
/// Attach the live simulation's datapack and saved collision/weather context
/// before configuration can finish and a player's movement can be applied.
fn initialize_live_level(
    level: &mut ServerLevel,
    world: &World,
    resources: &ResourceManager,
    properties: &ServerProperties,
    unported_behavior: UnportedBehavior,
) -> Result<()> {
    use ferraloom_game::level::{
        border::{self, WorldBorder},
        weather::WeatherData,
    };
    use ferraloom_storage::SavedDataStorage;
    level
        .level
        .set_block_tags(Arc::new(TagSet::<Block>::load(resources)?));
    level.level.set_fluid_tags(Arc::new(
        TagSet::<ferraloom_data::registries::fluid::Id>::load(resources)?,
    ));
    level
        .level
        .set_entity_type_tags(Arc::new(TagSet::<ferraloom_data::EntityType>::load(
            resources,
        )?));
    world.configure_live_rules(&mut level.level);
    level.level.set_game_time(level.game_time);
    level.level.set_unported_behavior(unported_behavior);
    level
        .level
        .set_max_chained_neighbor_updates(properties.int("max-chained-neighbor-updates"));
    let saved = SavedDataStorage::new(world.access.path().join("data"));
    let mut border = saved
        .load::<WorldBorder>(border::SAVED_DATA_ID)?
        .map_or_else(WorldBorder::default, |data| data.value().clone());
    let _ = border.apply_initial_settings(level.game_time);
    border.set_absolute_max_size(properties.int("max-world-size"));
    level.level.set_entity_world_border(border);
    let weather = saved
        .load::<WeatherData>("minecraft:weather")?
        .map_or_else(WeatherData::default, |data| *data.value());
    level.level.load_weather(weather);
    command_level::load_command_tickets(level, world)?;
    Ok(())
}
fn game_error(error: LevelError) -> anyhow::Error {
    anyhow::anyhow!("{error:?}")
}
fn game_type(name: &str) -> GameType {
    match name {
        "creative" => GameType::Creative,
        "adventure" => GameType::Adventure,
        "spectator" => GameType::Spectator,
        _ => GameType::Survival,
    }
}
fn protocol_game_type(mode: GameType) -> play::GameType {
    match mode {
        GameType::Survival => play::GameType::Survival,
        GameType::Creative => play::GameType::Creative,
        GameType::Adventure => play::GameType::Adventure,
        GameType::Spectator => play::GameType::Spectator,
    }
}
fn abilities(player: &ServerPlayer) -> cb::Packet {
    cb::Packet::PlayerAbilities(cb::PlayerAbilities {
        invulnerable: player.abilities.invulnerable,
        is_flying: player.abilities.flying,
        can_fly: player.abilities.mayfly,
        instabuild: player.instabuild(),
        flying_speed: player.ability_flying_speed(),
        walking_speed: player.ability_walking_speed(),
    })
}
fn player_info(profile: &GameProfile, player: &ServerPlayer) -> PlayerInfoEntry {
    PlayerInfoEntry {
        profile_id: profile.id,
        actions: vec![
            PlayerInfoAction::AddPlayer {
                name: common::Utf8(profile.name.clone()),
                properties: ProfileProperties(
                    profile
                        .properties
                        .iter()
                        .map(|p| play::ProfileProperty {
                            name: common::Utf8(p.name.clone()),
                            value: p.value.clone(),
                            signature: p.signature.clone().map(common::Utf8),
                        })
                        .collect(),
                ),
            },
            PlayerInfoAction::InitializeChat { session: None },
            PlayerInfoAction::UpdateGameMode {
                game_mode: protocol_game_type(player.game_mode),
            },
            PlayerInfoAction::UpdateListed { listed: true },
            PlayerInfoAction::UpdateLatency { latency: VarInt(0) },
            PlayerInfoAction::UpdateDisplayName { display_name: None },
            PlayerInfoAction::UpdateListOrder {
                list_order: VarInt(0),
            },
            PlayerInfoAction::UpdateHat { show_hat: true },
        ],
    }
}

fn light_packet(light: Option<&ChunkLight>) -> LightUpdatePacketData {
    light_packet_with_masks(light, (1_u64 << 26) - 1, (1_u64 << 26) - 1)
}
fn light_packet_with_masks(
    light: Option<&ChunkLight>,
    sky_filter: u64,
    block_filter: u64,
) -> LightUpdatePacketData {
    let mut sky_mask = 0;
    let mut block_mask = 0;
    let mut empty_sky = 0;
    let mut empty_block = 0;
    let mut sky_updates = Vec::new();
    let mut block_updates = Vec::new();
    for (index, y) in (-5..=20).enumerate() {
        for (sky, mask, empty, updates, filter) in [
            (
                true,
                &mut sky_mask,
                &mut empty_sky,
                &mut sky_updates,
                sky_filter,
            ),
            (
                false,
                &mut block_mask,
                &mut empty_block,
                &mut block_updates,
                block_filter,
            ),
        ] {
            if filter & (1_u64 << index) == 0 {
                continue;
            }
            let data = light.and_then(|l| if sky { l.sky.get(&y) } else { l.block.get(&y) });
            if let Some(bytes) = data.filter(|bytes| bytes.iter().any(|&v| v != 0)) {
                *mask |= 1u64 << index;
                updates.push(common::ByteArray(bytes.clone()));
            } else {
                *empty |= 1u64 << index;
            }
        }
    }
    LightUpdatePacketData {
        sky_y_mask: play::ByteBitSet(BitSet::from_words(vec![sky_mask])),
        block_y_mask: play::ByteBitSet(BitSet::from_words(vec![block_mask])),
        empty_sky_y_mask: play::ByteBitSet(BitSet::from_words(vec![empty_sky])),
        empty_block_y_mask: play::ByteBitSet(BitSet::from_words(vec![empty_block])),
        sky_updates,
        block_updates,
    }
}
fn client_information() -> common::serverbound::ClientInformation {
    common::serverbound::ClientInformation {
        language: common::FriendlyString("en_us".into()),
        view_distance: 2,
        chat_visibility: common::serverbound::ChatVisibility::Full,
        chat_colors: true,
        model_customisation: 127,
        main_hand: common::serverbound::HumanoidArm::Right,
        text_filtering_enabled: false,
        allows_listing: true,
        particle_status: common::serverbound::ParticleStatus::All,
    }
}
fn random_uuid() -> Uuid {
    let mut bytes: [u8; 16] = rand::random();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Uuid::from_byte_array(bytes)
}
fn make_status(
    properties: &ServerProperties,
    online: i32,
    sample: Vec<status::NameAndId>,
    favicon: Option<Vec<u8>>,
    enforces_secure_chat: bool,
) -> status::ServerStatus {
    status::ServerStatus {
        description: Component::literal(properties.get("motd")),
        players: Some(status::Players {
            max: properties.int("max-players"),
            online,
            sample,
        }),
        version: Some(status::Version {
            name: "26.3".into(),
            protocol: protocol::PROTOCOL_VERSION,
        }),
        favicon: favicon.map(|icon_bytes| status::Favicon { icon_bytes }),
        enforces_secure_chat,
    }
}
fn load_conduct(root: &Path, properties: &ServerProperties) -> Result<BTreeMap<String, String>> {
    let mut conduct = BTreeMap::new();
    if properties.boolean("enable-code-of-conduct") {
        let directory = std::fs::canonicalize(root.join("codeofconduct"))
            .context("Code of Conduct folder does not exist")?;
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|s| s == "txt") {
                ensure!(
                    std::fs::canonicalize(&path)?.parent() == Some(directory.as_path()),
                    "Code of Conduct file links outside allowed directory"
                );
                let language = path.file_stem().unwrap().to_string_lossy().to_lowercase();
                let text = std::fs::read_to_string(path)?
                    .lines()
                    .collect::<Vec<_>>()
                    .join("\n");
                conduct.insert(language, text);
            }
        }
    }
    Ok(conduct)
}

#[cfg(test)]
mod live_level_tests {
    use super::*;

    #[test]
    fn abilities_packet_reads_mutable_instabuild_instead_of_mode() {
        for mode in [
            GameType::Survival,
            GameType::Creative,
            GameType::Adventure,
            GameType::Spectator,
        ] {
            let mut player =
                ServerPlayer::new(Uuid::new(1, 2), "abilities".into(), 1, Vec3::ZERO, mode);
            for value in [false, true] {
                player.abilities.instabuild = value;
                player.abilities.invulnerable = value;
                player.abilities.mayfly = !value;
                player.abilities.flying = value;
                player.abilities.set_flying_speed(0.125);
                player.abilities.set_walking_speed(-0.25);
                let cb::Packet::PlayerAbilities(packet) = abilities(&player) else {
                    unreachable!()
                };
                assert_eq!(packet.instabuild, value);
                assert_eq!(packet.invulnerable, value);
                assert_eq!(packet.can_fly, !value);
                assert_eq!(packet.is_flying, value);
                assert_eq!(packet.flying_speed, 0.125);
                assert_eq!(packet.walking_speed, -0.25);
            }
        }
    }
}
