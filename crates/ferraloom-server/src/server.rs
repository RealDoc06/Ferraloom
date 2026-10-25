//! Single-region MinecraftServer phase driver. Game state has one mutable owner.
#[path = "block_actions.rs"]
mod block_actions;
#[path = "command_level.rs"]
mod command_level;
#[path = "command_runtime.rs"]
mod command_runtime;
#[path = "generation_completion.rs"]
pub(crate) mod generation_completion;
#[path = "interaction.rs"]
mod interaction;
#[path = "level_tick.rs"]
mod level_tick;
#[path = "levels.rs"]
mod levels;
#[path = "player_spawn.rs"]
mod player_spawn;
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
    world::{ChunkLight, World, WorldRoot, heightmap},
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
use player_spawn::{PrepareSpawnTask, PreparedPlayerSpawn};
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
    spawn: Option<PrepareSpawnTask>,
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
    registries: Arc<RegistrySet>,
    recipe_manager: Arc<RecipeManager>,
    recipe_synchronization: Arc<RecipeSynchronization>,
    crafting_context: Arc<RuntimeCraftingContext>,
    access: AccessLists,
    world_root: WorldRoot,
    level: crate::generation::runtime::LiveLevel,
    additional_levels: Vec<crate::generation::runtime::LiveLevel>,
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
        let registries = Arc::new(RegistrySet::load(&resources)?);
        let recipe_manager = Arc::new(RecipeManager::load(&resources, &TagSet::load(&resources)?)?);
        let recipe_synchronization =
            Arc::new(RecipeSynchronization::new(&recipe_manager, |_| true));
        let loot = Arc::new(LootData::load(
            &resources,
            Some(&LootRegistryLookup::new(&registries, &resources)),
        )?);
        let crafting_context = Arc::new(RuntimeCraftingContext::load(
            &resources,
            registries.as_ref(),
        )?);

        let mut world_root = WorldRoot::open(
            &options.root,
            &properties,
            &resources,
            &registries,
            options.unported_behavior,
        )?;
        let world = world_root.open_dimension("minecraft:overworld".parse()?, &properties)?;
        let game_rules = command_runtime::load_game_rules(&world_root)?;
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
        let clocks = command_runtime::load_clocks(&world_root, &resources)?;
        // Older bossbar storage is migrated by the datafix workstream, never runtime fallback.
        let custom_boss_events = {
            use ferraloom_game::bossbar::{BossEventsSaveData, CustomBossEvents};
            let storage =
                ferraloom_storage::SavedDataStorage::new(world_root.access.path().join("data"));
            CustomBossEvents::load(
                storage
                    .load::<BossEventsSaveData>(CustomBossEvents::STORAGE_ID)?
                    .map(|saved| saved.value().clone())
                    .unwrap_or_default(),
                random_uuid,
            )
        };
        let command_storage_folder = world.access.path().join("data");
        let command_storage = tokio::task::spawn_blocking(move || {
            ferraloom_game::commands::storage::CommandStorage::preload(&command_storage_folder)
        })
        .await?;
        let command_context = command_runtime::command_build_context(
            &registries,
            &resources,
            command_runtime::rule_features(&world_root),
        );
        let command_entity_tags =
            ferraloom_data::datapack::load_tags(&resources, "entity_type", |id, _| {
                EntityType::from_name(&id.to_string())
            })
            .values
            .into_iter()
            .map(|(id, types)| (id.to_string(), types))
            .collect();

        let game_time = world_root.level_data.data.get_long_or("Time", 0);
        let (access, ops) = AccessLists::load(&options.root)?;
        let weather =
            ferraloom_storage::SavedDataStorage::new(world_root.access.path().join("data"))
                .load::<ferraloom_game::level::weather::WeatherData>("minecraft:weather")?
                .map_or_else(Default::default, |data| *data.value());
        let registry_context: Arc<dyn protocol::codec::RegistryContext> = registries.clone();
        let level_context = Arc::new(
            Arc::unwrap_or_clone(world.level_context(&resources)?)
                .with_registry_context(registry_context.clone()),
        );
        let mut level = ServerLevel::new(
            Region::new(&BEHAVIORS),
            "minecraft:overworld",
            level_context,
        );
        level
            .level
            .install_shared(Box::new(ferraloom_game::ServerShared {
                scoreboard,
                custom_boss_events,
                ops,
                weather,
                entity_ids: Default::default(),
            }));
        level.game_time = game_time;
        level.level.set_command_storage(command_storage.snapshot());
        initialize_live_level(
            &mut level,
            &world,
            &resources,
            &properties,
            options.unported_behavior,
            &weather,
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
        let container_resources = Arc::new(ContainerResources {
            recipes: Arc::clone(&recipe_manager),
            loot,
            crafting: Arc::clone(&crafting_context),
            advancements,
            recipe_tags,
            recipe_synchronization: Arc::clone(&recipe_synchronization),
        });
        level
            .level
            .set_container_resources(container_resources.clone(), world.seed);
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
            registries: Arc::clone(&registries),
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
        let mut additional_levels = Vec::new();
        // ResourceKey identity-hash traversal is non-parity order. Keep the
        // baked WorldDimensions order after the explicit overworld insertion.
        let dimensions = world_root
            .dimensions
            .dimensions
            .iter()
            .filter(|stem| stem.name.namespace() != "minecraft" || stem.name.path() != "overworld")
            .map(|stem| stem.name.to_string().parse::<protocol::Identifier>())
            .collect::<protocol::Result<Vec<_>>>()?;
        for dimension in dimensions {
            let world = world_root.open_dimension(dimension.clone(), &properties)?;
            let level_context = Arc::new(
                Arc::unwrap_or_clone(world.level_context(&resources)?)
                    .with_registry_context(registry_context.clone()),
            );
            let mut bootstrap = ServerLevel::new(
                Region::new(&BEHAVIORS),
                dimension.to_string(),
                level_context,
            );
            bootstrap.game_time = game_time;
            initialize_live_level(
                &mut bootstrap,
                &world,
                &resources,
                &properties,
                options.unported_behavior,
                &weather,
            )?;
            bootstrap
                .level
                .set_command_storage(command_storage.snapshot());
            bootstrap
                .level
                .set_container_resources(container_resources.clone(), world.seed);
            game_rules.update_level_rules(bootstrap.level.rules_mut());
            world.install_live_environment(
                &mut bootstrap.level,
                command_runtime::environment_clock_snapshot(&clocks),
            )?;
            bootstrap
                .distance_manager
                .set_view_distance(properties.int("view-distance"));
            bootstrap.level.configure_entity_tracking(
                properties.int("view-distance"),
                properties.int("entity-broadcast-range-percentage"),
            );
            bootstrap.distance_manager.set_simulation_distance(
                properties.int("simulation-distance").clamp(2, 32),
                &mut bootstrap.tickets,
            );
            additional_levels.push(crate::generation::runtime::LiveLevel::install(
                world,
                bootstrap,
                properties.clone(),
            )?);
        }
        let shared = Some(level.level.take_shared().map_err(game_error)?);
        let mut server = Self {
            properties: properties.clone(),
            root: options.root,
            registries,
            recipe_manager,
            recipe_synchronization,
            crafting_context,
            access,
            world_root,
            level,
            additional_levels,
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
        self.shared
            .as_deref()
            .or_else(|| self.get_all_levels().find_map(|level| level.shared().ok()))
            .expect("server shared owner is installed for the phase")
    }
    fn shared_mut(&mut self) -> &mut ferraloom_game::ServerShared {
        if let Some(shared) = self.shared.as_deref_mut() {
            return shared;
        }
        std::iter::once(&mut self.level)
            .chain(self.additional_levels.iter_mut())
            .find_map(|level| level.level.shared_mut().ok())
            .expect("server shared owner is installed for the phase")
    }
    /// Only outer phase boundaries move the single owner. Nested operations
    /// require it to remain installed on the selected level.
    fn lend_server_shared_to(&mut self, dimension: &protocol::Identifier) -> Result<bool> {
        self.live_level_in(dimension)?
            .generation()
            .enter_owner_thread();
        if self.shared.is_none() {
            self.live_level_in(dimension)?
                .level
                .shared()
                .map_err(game_error)?;
            return Ok(false);
        }
        let shared = self.shared.take().expect("checked shared owner");
        self.live_level_in_mut(dimension)?
            .level
            .install_shared(shared);
        Ok(true)
    }
    fn lend_server_shared(&mut self) -> bool {
        if self.shared.is_none() {
            self.shared(); // A nested command retains the current phase owner.
            return false;
        }
        self.lend_server_shared_to(&id("overworld"))
            .expect("overworld phase shared owner")
    }
    fn drain_scoreboard_packets(&mut self) {
        for packet in self.shared_mut().scoreboard.drain_packets() {
            self.broadcast(packet);
        }
    }
    fn return_server_shared(&mut self, lent: bool) {
        self.drain_scoreboard_packets();
        self.drain_bossbar_packets();
        if lent {
            let mut owner = None;
            for live in self.live_levels_mut() {
                if live.level.shared().is_ok() {
                    assert!(owner.is_none(), "one canonical server shared owner");
                    owner = Some(live.level.take_shared().expect("installed shared owner"));
                }
            }
            self.shared = Some(owner.expect("phase must return its shared owner"));
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
        self.process_packets_inner().await
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
                        spawn: None,
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
                    let phase = c.phase;
                    let preparing_dimension =
                        c.spawn.as_ref().map(|spawn| spawn.dimension().clone());
                    let dimension = match phase {
                        Phase::Play(entity) => Some(
                            self.level_for_entity(entity)
                                .map_err(game_error)?
                                .dimension_key()
                                .map_err(game_error)?
                                .to_string()
                                .parse::<protocol::Identifier>()?,
                        ),
                        Phase::Configuration(_) => preparing_dimension,
                    };
                    let lent = match &dimension {
                        Some(dimension) => self.lend_server_shared_to(dimension)?,
                        None => false,
                    };
                    let result = match packet {
                        Incoming::Configuration(p) => self.configuration(id, p).await,
                        Incoming::Play(p) => self.play_packet(id, *p).await,
                    };
                    self.return_server_shared(lent);
                    if let Err(e) = result {
                        eprintln!("Packet rejected for {id}: {e:#}");
                        if let Some(c) = self.connections.get_mut(&id) {
                            c.disconnect(e.to_string());
                        }
                    }
                }
                Ingress::Closed { id } => {
                    let dimension = match self.connections.get(&id).map(|c| c.phase) {
                        Some(Phase::Play(entity)) => self
                            .level_for_entity(entity)
                            .map_err(game_error)?
                            .dimension_key()
                            .map_err(game_error)?
                            .to_string()
                            .parse::<protocol::Identifier>()?,
                        _ => crate::registries::id("overworld"),
                    };
                    let lent = self.lend_server_shared_to(&dimension)?;
                    let result = self.remove_player(id).await;
                    self.return_server_shared(lent);
                    result?;
                }
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
        }
        // Complete each level before entering the next, in createLevels order.
        for dimension in self.level_keys() {
            self.tick_level(&dimension, paused)?;
        }
        self.tick_connections().await?;
        self.send_chunks()?;
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
                self.place_player(connection).await?;
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
        let access = self.world_root.access.clone();
        let uuid = self.connections[&connection].profile.id.to_string();
        let saved = tokio::task::spawn_blocking(move || access.player_data().load(&uuid)).await??;
        let saved = ServerPlayer::saved_position(saved.as_ref());
        let respawn = self.default_spawn_data();
        let dimension = saved.resolve_dimension(&respawn.dimension.parse()?, |key| {
            self.get_level(key).is_some()
        });
        let dimension_type = self.world_root.stem(&dimension)?.dimension;
        let game_type = self.default_game_mode;
        let radius = self
            .game_rules
            .integer(ferraloom_game::gamerules::GameRule::RespawnRadius);
        let lent = self.lend_server_shared_to(&dimension)?;
        let task = PrepareSpawnTask::new(
            self.live_level_in_mut(&dimension)?,
            dimension_type,
            dimension.clone(),
            saved,
            &respawn,
            game_type,
            radius,
        );
        self.return_server_shared(lent);
        let c = self
            .connections
            .get_mut(&connection)
            .context("missing preparing connection")?;
        c.spawn = Some(task?);
        c.phase = Phase::Configuration(ConfigTask::Preparing);
        Ok(())
    }

    /// Port of Ready#spawn's waitForEntities -> constructor -> second player NBT
    /// load. All IO captures are owned; no Level borrow survives an await.
    async fn construct_ready_player(
        &mut self,
        connection: ConnectionId,
        spawn: &PreparedPlayerSpawn,
    ) -> Result<ServerPlayer> {
        self.wait_for_spawn_entities(spawn).await?;
        let profile = self.connections[&connection].profile.clone();
        let mode = self.default_game_mode;
        let network_id = self
            .live_level_in(&spawn.dimension)?
            .level
            .next_entity_id()
            .map_err(game_error)?;
        let mut player =
            ServerPlayer::new(profile.id, profile.name, network_id, spawn.position, mode);
        let access = self.world_root.access.clone();
        let uuid = profile.id.to_string();
        let saved = tokio::task::spawn_blocking(move || access.player_data().load(&uuid)).await??;
        if let Some(saved) = saved {
            player.try_load_with_recipes(saved, &self.recipe_manager)?;
        }
        let advancement_manager = self
            .live_level_in(&spawn.dimension)?
            .level
            .container_resources()
            .map_err(game_error)?
            .advancements
            .clone();
        let progress_path = self.world_root.access.path().to_path_buf();
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
        player.snap_to(spawn.position, spawn.yaw, spawn.pitch);
        // ServerPlayer#updateOptions publishes canonical Avatar metadata before tracking starts.
        let information = &self.connections[&connection].information;
        player.entity_data.set(
            ferraloom_game::entity::synched_data::PLAYER_MODEL_CUSTOMISATION,
            information.model_customisation as i8,
        );
        player.parent.set_main_arm(match information.main_hand {
            common::serverbound::HumanoidArm::Left => protocol::entity_data::HumanoidArm::Left,
            common::serverbound::HumanoidArm::Right => protocol::entity_data::HumanoidArm::Right,
        });
        player.view_distance = i32::from(self.connections[&connection].information.view_distance);
        player.text_filtering_enabled = self.connections[&connection]
            .information
            .text_filtering_enabled;
        Ok(player)
    }

    /// Port of ServerLevel#waitForEntities. Owned tasks, entity responses and
    /// pending load callbacks progress without simulating entities or time.
    async fn wait_for_spawn_entities(&mut self, spawn: &PreparedPlayerSpawn) -> Result<()> {
        let center = ChunkPos::containing(BlockPos::new(
            ferraloom_util::mth::floor(spawn.position.x),
            ferraloom_util::mth::floor(spawn.position.y),
            ferraloom_util::mth::floor(spawn.position.z),
        ));
        loop {
            let ready = {
                let live = self.live_level_in_mut(&spawn.dimension)?;
                live.poll_entity_loads_for_wait()?;
                (-3..=3).all(|z| {
                    (-3..=3).all(|x| {
                        live.level
                            .are_entities_loaded(ChunkPos::new(center.x + x, center.z + z))
                    })
                })
            };
            if ready {
                return Ok(());
            }
            self.live_level_in_mut(&spawn.dimension)?
                .level
                .poll_chunk_acquisition_mut()
                .map_err(game_error)?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Apply accepted section transitions immediately, including Y-only moves.
    /// Preserve ChunkMap#move's audience pass before the distance-manager pass.
    pub(super) fn update_player_tickets(&mut self, entity: EntityRef) -> Result<()> {
        let dimension = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .dimension_key()
            .map_err(game_error)?
            .to_string()
            .parse::<protocol::Identifier>()?;
        self.live_level_in_mut(&dimension)?
            .level
            .move_tracked_player(entity)
            .map_err(anyhow::Error::new)?;
        let changes = self
            .live_level_in_mut(&dimension)?
            .level
            .take_player_section_changes();
        for (player, section) in changes {
            self.live_level_in_mut(&dimension)?
                .level
                .update_player_tracking_section(player, section)
                .map_err(game_error)?;
        }
        let section = ferraloom_util::SectionPos::from_position(self.player(entity)?.position);
        self.live_level_in_mut(&dimension)?
            .level
            .update_player_tracking_section(entity, section)
            .map_err(game_error)?;
        self.live_level_in_mut(&dimension)?
            .level
            .update_player_chunk_tracking(entity)
            .map_err(game_error)?;
        // ServerChunkCache#move refreshes receiving connections after ChunkMap#move.
        if self
            .level
            .level
            .is_receiving_waypoints(entity)
            .map_err(game_error)?
        {
            self.level
                .level
                .update_waypoint_player(entity)
                .map_err(game_error)?;
        }
        Ok(())
    }

    fn update_chunk_tickets(&mut self) -> Result<()> {
        self.update_chunk_tickets_in(&id("overworld"))
    }
    fn integrate_jobs(&mut self) -> Result<()> {
        self.integrate_jobs_in(&id("overworld"))
    }
    fn apply_level_effects(&mut self) -> Result<()> {
        for dimension in self.level_keys() {
            self.apply_level_effects_in(&dimension)?;
        }
        Ok(())
    }
    fn broadcast_container_changes(&mut self) -> Result<()> {
        for dimension in self.level_keys() {
            self.broadcast_container_changes_in(&dimension)?;
        }
        Ok(())
    }
    async fn place_player(&mut self, connection: ConnectionId) -> Result<()> {
        let spawn = self.connections[&connection]
            .spawn
            .as_ref()
            .context("spawn not prepared")?
            .ready()?
            .clone();
        let dimension = spawn.dimension.clone();
        let mut player = self.construct_ready_player(connection, &spawn).await?;
        let mut c = self
            .connections
            .remove(&connection)
            .context("missing joining connection")?;
        c.spawn = None;
        let entity = self.live_level_in(&dimension)?.level.next_entity_ref();
        let pos = player.position;
        let yaw = player.yaw;
        let pitch = player.pitch;
        let mode = protocol_game_type(player.game_mode);
        let entity_id = player.network_id;
        let permission = self.shared().ops.permission_level(player.uuid).id();
        self.access.remember(&c.profile)?;
        c.phase = Phase::Play(entity);
        c.push(cb::Packet::Login(cb::Login {
            player_id: entity_id,
            hardcore: self.properties.boolean("hardcore"),
            levels: self.level_keys(),
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
                dimension_type: DimensionTypeId(self.registries.numeric(
                    "dimension_type",
                    self.world_root.dimension_type_key(&dimension)?.as_str(),
                )),
                dimension: dimension.clone(),
                seed: ferraloom_worldgen::biome_manager::obfuscate_seed(self.world_root.seed),
                game_type: mode,
                previous_game_type: OptionalGameType(None),
                is_debug: self.world_root.is_debug_world(),
                is_flat: self.world_root.is_flat_world(),
                last_death_location: None,
                portal_cooldown: VarInt(0),
                sea_level: VarInt(self.live_level_in(&dimension)?.level.sea_level()),
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
                Phase::Play(handle) => self.player(handle).ok().map(|p| player_info(&c.profile, p)),
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
            self.live_level_in(&dimension)?
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
        self.live_level_in_mut(&dimension)?
            .level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .map_err(game_error)?;
        // PlayerList#placeNewPlayer: boss events follow addNewPlayer, before initInventoryMenu.
        self.shared_mut().custom_boss_events.on_player_connect(
            ferraloom_game::bossbar::BossPlayer {
                entity,
                uuid: c.profile.id,
                network_id: entity_id,
            },
        );
        for (target, packet) in self.shared_mut().custom_boss_events.drain_packets() {
            if target == entity {
                c.push(packet);
            } else {
                self.send_bossbar_packet(target, packet);
            }
        }
        ferraloom_game::inventory::send_container_packets(
            entity,
            [cb::Packet::PostEffects(common_cb::PostEffects {
                post_effects: Vec::new(),
            })],
            &mut self.live_level_in_mut(&dimension)?.level,
        )?;
        ferraloom_game::inventory::initialize_inventory_menu(
            entity,
            &mut self.live_level_in_mut(&dimension)?.level,
        )?;
        use protocol::entity_data::{DataValue, EntityDataItems, EntityDataValue};
        let post_initialization = {
            let player = self
                .live_level_in(&dimension)?
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
                            value: EntityDataValue::Byte(*player.entity_data.get(
                                ferraloom_game::entity::synched_data::PLAYER_MODEL_CUSTOMISATION,
                            )),
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
            &mut self.live_level_in_mut(&dimension)?.level,
        )?;
        println!(
            "{} joined at {}, {}, {}",
            c.profile.name, pos.x, pos.y, pos.z
        );
        self.players.push(connection);
        self.connections.insert(connection, c);
        self.apply_level_effects_in(&dimension)
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
                    PlayerAction::StartDestroyBlock
                        | PlayerAction::ChangeDestroyDirection
                        | PlayerAction::AbortDestroyBlock
                        | PlayerAction::StopDestroyBlock
                ) =>
            {
                self.handle_player_block_action(connection, entity, packet)?;
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
                    entity_teleport::accept_teleport(
                        entity,
                        p.id.0,
                        self.level_for_entity_mut(entity).map_err(game_error)?,
                    )
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
                self.level_for_entity_mut(entity)
                    .map_err(game_error)?
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .on_ground = p.flags.on_ground
            }
            sb::Packet::ChunkBatchReceived(p) => self
                .level_for_entity_mut(entity)
                .map_err(game_error)?
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?
                .chunk_sender
                .acknowledge(p.desired_chunks_per_tick),
            sb::Packet::KeepAlive(p) => self.keepalive_reply(connection, p.id)?,
            sb::Packet::PlayerLoaded(_) => {
                self.level_for_entity_mut(entity)
                    .map_err(game_error)?
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
                    .level_for_entity_mut(entity)
                    .map_err(game_error)?
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                // ServerPlayer#updateOptions updates the same canonical accessors after join.
                player.entity_data.set(
                    ferraloom_game::entity::synched_data::PLAYER_MODEL_CUSTOMISATION,
                    p.model_customisation as i8,
                );
                player.parent.set_main_arm(match p.main_hand {
                    common::serverbound::HumanoidArm::Left => {
                        protocol::entity_data::HumanoidArm::Left
                    }
                    common::serverbound::HumanoidArm::Right => {
                        protocol::entity_data::HumanoidArm::Right
                    }
                });
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
                    .level_for_entity_mut(entity)
                    .map_err(game_error)?
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
                if !self.connections[&connection].received_movement_this_tick {
                    self.level_for_entity_mut(entity)
                        .map_err(game_error)?
                        .entity_mut::<ServerPlayer>(entity)
                        .map_err(game_error)?
                        .known_movement = Vec3::ZERO;
                }
                let c = self.connections.get_mut(&connection).unwrap();
                c.received_position_this_tick = false;
                c.received_movement_this_tick = false;
            }
            sb::Packet::PlayerInput(p) => {
                let loaded = {
                    let player = self
                        .level_for_entity_mut(entity)
                        .map_err(game_error)?
                        .entity_mut::<ServerPlayer>(entity)
                        .map_err(game_error)?;
                    player.last_client_input = p.input;
                    player.has_client_loaded()
                };
                if loaded {
                    self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
                    self.level_for_entity_mut(entity)
                        .map_err(game_error)?
                        .entity_mut::<ServerPlayer>(entity)
                        .map_err(game_error)?
                        .set_shared_flag(1, p.input.shift);
                }
            }
            sb::Packet::SeenAdvancements(packet) => {
                ferraloom_game::advancements::seen_advancements(
                    self.level_for_entity_mut(entity).map_err(game_error)?,
                    entity,
                    &packet,
                )
                .map_err(game_error)?;
            }
            sb::Packet::ClientCommand(packet)
                if packet.action == play::ClientCommandAction::RequestStats =>
            {
                ferraloom_game::stats::request_stats(
                    self.level_for_entity_mut(entity).map_err(game_error)?,
                    entity,
                )
                .map_err(game_error)?;
            }
            sb::Packet::CustomPayload(_) | sb::Packet::Pong(_) | sb::Packet::PlayerCommand(_) => {}
            // These gameplay ports are outside the terrain/join milestone.
            _ => {}
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
                .level_for_entity(entity)
                .map_err(game_error)?
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
                self.level_for_entity_mut(entity).map_err(game_error)?,
            )?;
        } else {
            if !inventory::live_menu_still_valid(
                entity,
                menu,
                self.level_for_entity(entity).map_err(game_error)?,
            )? || !valid_slot
            {
                return Ok(());
            }
            let mismatch = {
                let receiver = self
                    .level_for_entity_mut(entity)
                    .map_err(game_error)?
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .menus
                    .menu_mut(menu)?;
                let mismatch = receiver.state_id() != packet.state_id.0;
                receiver.suppress_remote_updates();
                mismatch
            };
            let resources = self
                .level_for_entity(entity)
                .map_err(game_error)?
                .container_resources()
                .map_err(game_error)?;
            let kind = self
                .level_for_entity(entity)
                .map_err(game_error)?
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
                self.level_for_entity_mut(entity).map_err(game_error)?,
            )?;
            // Java re-reads player.containerMenu after clicked: a callback may
            // have replaced it. The captured old receiver was rooted only for
            // clicked itself, and must never receive the new remote state.
            let current = {
                let menus = &mut self
                    .level_for_entity_mut(entity)
                    .map_err(game_error)?
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
                self.level_for_entity_mut(entity).map_err(game_error)?,
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
            self.level_for_entity_mut(entity).map_err(game_error)?,
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
                .level_for_entity(entity)
                .map_err(game_error)?
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
            self.level_for_entity_mut(entity).map_err(game_error)?,
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
        ferraloom_game::inventory::send_container_packets(
            entity,
            packets,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )?;
        for drop in drops {
            self.drop_inventory_stack(entity, drop.stack, drop.thrown_from_hand)?;
        }
        if accepted {
            ferraloom_game::inventory::broadcast_container_menu(
                entity,
                self.level_for_entity_mut(entity).map_err(game_error)?,
            )
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
        let recipe_manager = Arc::clone(&self.recipe_manager);
        let recipe_synchronization = Arc::clone(&self.recipe_synchronization);
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let (menu, allow_dropping) = {
            let player = self
                .level_for_entity(entity)
                .map_err(game_error)?
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
        if !inventory::live_menu_still_valid(
            entity,
            menu,
            self.level_for_entity(entity).map_err(game_error)?,
        )? {
            return Ok(());
        }
        let Some(display) = recipe_synchronization.get_recipe_from_display(packet.recipe) else {
            return Ok(());
        };
        let recipe = recipe_manager
            .by_key(&display.parent)
            .context("recipe display without parent")?;
        let kind = {
            let player = self
                .level_for_entity(entity)
                .map_err(game_error)?
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
            resources: self
                .level_for_entity(entity)
                .map_err(game_error)?
                .container_resources()
                .map_err(game_error)?,
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
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )?;
        if action == ferraloom_game::crafting::PostPlaceAction::PlaceGhostRecipe {
            let container_id = self
                .level_for_entity(entity)
                .map_err(game_error)?
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
                self.level_for_entity_mut(entity).map_err(game_error)?,
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
        self.level_for_entity_mut(entity)
            .map_err(game_error)?
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
        let dimension = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .dimension_key()
            .map_err(game_error)?
            .to_string();
        let live = std::iter::once(&mut self.level)
            .chain(self.additional_levels.iter_mut())
            .find(|live| live.dimension == dimension)
            .expect("selected player dimension");
        live.level
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
        let dimension = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .dimension_key()
            .map_err(game_error)?
            .to_string();
        let (synchronize, packets) = {
            let live = std::iter::once(&mut self.level)
                .chain(self.additional_levels.iter_mut())
                .find(|live| live.dimension == dimension)
                .expect("selected player dimension");
            let player = live
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
        ferraloom_game::inventory::send_container_packets(
            entity,
            packets,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )?;
        if let Some(stack) = drop {
            self.drop_inventory_stack(entity, stack, true)?;
        }
        if synchronize {
            let menu = self
                .level_for_entity(entity)
                .map_err(game_error)?
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .menus
                .inventory_menu
                .reference();
            ferraloom_game::inventory::synchronize_menu(
                entity,
                menu,
                ferraloom_game::inventory::MenuSynchronization::Changes,
                self.level_for_entity_mut(entity).map_err(game_error)?,
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
            self.level_for_entity_mut(entity)
                .map_err(game_error)?
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
                .level_for_entity_mut(entity)
                .map_err(game_error)?
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            if !player.loaded || player.game_mode == GameType::Spectator {
                return Ok(());
            }
            let inventory = &mut player.inventory;
            let drop = if action == PlayerAction::SwapItemWithOffhand {
                ferraloom_game::inventory::swap_player_hands(
                    entity,
                    self.level_for_entity_mut(entity).map_err(game_error)?,
                )?;
                return ferraloom_game::entity::living::stop_using_item(
                    entity,
                    self.level_for_entity_mut(entity).map_err(game_error)?,
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
        ferraloom_game::inventory::send_container_packets(
            entity,
            packets,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )?;
        if let Some(stack) = drop {
            self.drop_inventory_stack(entity, stack, true)?;
        }
        ferraloom_game::inventory::broadcast_container_menu(
            entity,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )
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
                self.level_for_entity_mut(entity).map_err(game_error)?,
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
                .level_for_entity(entity)
                .map_err(game_error)?
                .chunk(ChunkPos::containing(packet.pos))
                .is_none()
        {
            return Ok(());
        }
        let state = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .get_block_state(packet.pos)
            .map_err(game_error)?;
        let include_data = player.game_mode == GameType::Creative && packet.include_data;
        let stack = self
            .level_for_entity(entity)
            .map_err(game_error)?
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
        let Some(entity) = self
            .level_for_entity(player)
            .map_err(game_error)?
            .entity_by_network_id(packet.id.0)
        else {
            return Ok(());
        };
        let source = self.player(player)?;
        let target = self
            .level_for_entity(player)
            .map_err(game_error)?
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
            .level_for_entity(player)
            .map_err(game_error)?
            .entity_behavior(entity)
            .map_err(game_error)?
            .get_pick_result(entity, self.level_for_entity(player).map_err(game_error)?)
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
        ferraloom_game::inventory::try_pick_item_live(
            entity,
            stack,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )?;
        self.apply_level_effects()
    }
    fn player(&self, entity: EntityRef) -> Result<&ServerPlayer> {
        self.level_for_entity(entity)
            .and_then(|level| level.entity::<ServerPlayer>(entity))
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
        entity_teleport::teleport_player(
            entity,
            destination,
            relatives,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )
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
        let dimension = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .dimension_key()
            .map_err(game_error)?
            .to_string()
            .parse::<protocol::Identifier>()?;
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
            self.level_for_entity_mut(entity)
                .map_err(game_error)?
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?
                .abs_snap_rotation_to(
                    ferraloom_util::mth::wrap_degrees_f32(yaw),
                    ferraloom_util::mth::wrap_degrees_f32(pitch),
                );
            return Ok(());
        }
        let player = self
            .level_for_entity_mut(entity)
            .map_err(game_error)?
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
        // Native noPhysics movement never waits for generation tickets.
        if !self.player(entity)?.no_physics {
            let center = ChunkPos::containing(BlockPos::new(
                target.x.floor() as i32,
                target.y.floor() as i32,
                target.z.floor() as i32,
            ));
            self.request_area_in(&dimension, center, 1)?;
            // A required chunk read retains its selected dimension across the IO wait.
            loop {
                let missing = {
                    let level = self.level_for_entity(entity).map_err(game_error)?;
                    (-1..=1).any(|x| {
                        (-1..=1).any(|z| {
                            level
                                .chunk(ChunkPos::new(center.x + x, center.z + z))
                                .is_none()
                        })
                    })
                };
                if !missing {
                    break;
                }
                self.integrate_jobs_in(&dimension)?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        let checks = MovementChecks::dedicated(&self.tick_rate_manager);
        let live = std::iter::once(&mut self.level)
            .chain(self.additional_levels.iter_mut())
            .find(|live| live.dimension == dimension.as_str())
            .expect("selected player dimension");
        let result = move_player_with_checks(
            &mut live.level,
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
                    .level_for_entity_mut(entity)
                    .map_err(game_error)?
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

    async fn tick_connections(&mut self) -> Result<()> {
        let lent = self.lend_server_shared();
        let suggestions = self.process_command_suggestions();
        self.return_server_shared(lent);
        suggestions?;
        let now = Instant::now();
        let mut joining = Vec::new();
        let connections: Vec<_> = self.connections.keys().copied().collect();
        for id in connections {
            let c = self.connections.get_mut(&id).expect("selected connection");
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
            let phase = c.phase;
            match phase {
                Phase::Configuration(ConfigTask::Preparing) => {
                    let dimension = self.connections[&id]
                        .spawn
                        .as_ref()
                        .context("spawn not prepared")?
                        .dimension()
                        .clone();
                    let dimension_type = self.world_root.stem(&dimension)?.dimension;
                    let lent = self.lend_server_shared_to(&dimension)?;
                    let result = {
                        let task = self
                            .connections
                            .get_mut(&id)
                            .unwrap()
                            .spawn
                            .as_mut()
                            .expect("prepared spawn task");
                        let live = std::iter::once(&mut self.level)
                            .chain(self.additional_levels.iter_mut())
                            .find(|live| live.dimension == dimension.as_str())
                            .expect("selected dimension");
                        task.tick(live, dimension_type)
                    };
                    self.return_server_shared(lent);
                    if result? {
                        joining.push(id);
                    }
                }
                Phase::Configuration(ConfigTask::Finishing) => {
                    let task = self.connections[&id]
                        .spawn
                        .as_ref()
                        .context("spawn not prepared")?;
                    task.keep_alive(self.live_level_in(task.dimension())?, self.tick)?;
                }
                Phase::Play(entity) => {
                    if !self.was_paused {
                        let dimension = self
                            .level_for_entity(entity)
                            .map_err(game_error)?
                            .dimension_key()
                            .map_err(game_error)?
                            .to_string()
                            .parse::<protocol::Identifier>()?;
                        let lent = self.lend_server_shared_to(&dimension)?;
                        let result = {
                            let live = std::iter::once(&mut self.level)
                                .chain(self.additional_levels.iter_mut())
                                .find(|live| live.dimension == dimension.as_str())
                                .expect("selected dimension");
                            levels::tick_player_connection(
                                entity,
                                &mut live.level,
                                &self.tick_rate_manager,
                            )
                        };
                        self.return_server_shared(lent);
                        result?;
                    }
                    let c = self.connections.get_mut(&id).expect("selected connection");
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
            let task = self.connections[&id]
                .spawn
                .as_ref()
                .context("spawn not prepared")?;
            task.keep_alive(self.live_level_in(task.dimension())?, self.tick)?;
            let c = self.connections.get_mut(&id).unwrap();
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
        let mut batches = Vec::new();
        for connection in self.players.clone() {
            let Some(c) = self.connections.get(&connection) else {
                continue;
            };
            if c.close {
                continue;
            }
            let Phase::Play(entity) = c.phase else {
                continue;
            };
            let dimension = self
                .level_for_entity(entity)
                .map_err(game_error)?
                .dimension_key()
                .map_err(game_error)?
                .to_string()
                .parse::<protocol::Identifier>()?;
            let ready = self
                .live_level_in(&dimension)?
                .generation()
                .with_world(|world| {
                    world
                        .ready
                        .intersection(&world.light_ready)
                        .copied()
                        .collect::<HashSet<_>>()
                });
            let player = self
                .live_level_in_mut(&dimension)?
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            let center = player.chunk_position();
            let positions = player.chunk_sender.next_batch(center, |pos| {
                // prepareTickingChunk and center-light import own readiness;
                // neither another dimension nor neighbor readiness substitutes.
                ready.contains(&pos)
            });
            if !positions.is_empty() {
                batches.push((connection, dimension, positions));
            }
        }
        for (connection, dimension, positions) in batches {
            let mut packets = vec![cb::Packet::ChunkBatchStart(cb::ChunkBatchStart {})];
            for &pos in &positions {
                let chunk = self
                    .live_level_in(&dimension)?
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
                            .live_level_in(&dimension)?
                            .level
                            .block_entity_chunk_packet_info(pos)
                            .map_err(game_error)?,
                    },
                    light_data: self
                        .live_level_in(&dimension)?
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
            let access = self.world_root.access.clone();
            let (progress, tag, uuid, name, boss_player) = {
                let level = self.level_for_entity(entity).map_err(game_error)?;
                let player = level.entity::<ServerPlayer>(entity).map_err(game_error)?;
                (
                    ferraloom_game::advancements::PlayerProgressSave::capture(
                        player,
                        access.path(),
                    )?,
                    player.save(level).map_err(game_error)?,
                    player.uuid.to_string(),
                    level
                        .entity_display_name_with_scoreboard(
                            entity,
                            self.shared().scoreboard.scoreboard(),
                        )
                        .map_err(game_error)?,
                    ferraloom_game::bossbar::BossPlayer {
                        entity,
                        uuid: player.uuid,
                        network_id: player.network_id,
                    },
                )
            };
            tokio::task::spawn_blocking(move || -> Result<()> {
                access.player_data().save(&uuid, &tag)?;
                progress.write()?;
                Ok(())
            })
            .await??;
            let dimension = self
                .level_for_entity(entity)
                .map_err(game_error)?
                .dimension_key()
                .map_err(game_error)?
                .to_string()
                .parse::<protocol::Identifier>()?;
            self.level_for_entity_mut(entity)
                .map_err(game_error)?
                .remove_entity(entity)
                .map_err(game_error)?;
            self.apply_level_effects_in(&dimension)?;
            self.players.retain(|&id| id != connection);
            self.shared_mut()
                .custom_boss_events
                .on_player_disconnect(boss_player);
            self.drain_bossbar_packets();
            self.broadcast(cb::Packet::PlayerInfoRemove(cb::PlayerInfoRemove {
                profile_ids: vec![c.profile.id],
            }));
            // ServerGamePacketListenerImpl#removePlayerFromWorld styles the
            // complete leave component, so the name inherits yellow as well.
            let mut message = Component::translatable(
                "multiplayer.player.left",
                vec![protocol::chat::Argument::Component(Box::new(name))],
            );
            message.style.color = Some(protocol::chat::TextColor::parse("yellow").unwrap());
            self.broadcast(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                content: message,
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
            let spawn = self.world_root.spawn_position();
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
        self.world_root
            .begin_save_weather(self.shared().weather)
            .await??;
        self.save_live_entities(flush).await?;

        self.save_game_rules().await?;
        self.save_custom_boss_events().await?;
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
        let progress_path = self.world_root.access.path().to_path_buf();
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
                let level = self.level_for_entity(entity).map_err(game_error)?;
                let player = level.entity::<ServerPlayer>(entity).map_err(game_error)?;
                Ok((
                    player.uuid.to_string(),
                    player.save(level).map_err(game_error)?,
                ))
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
        let save = self
            .world_root
            .begin_save_level_and_players(self.level.game_time, players);
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
            self.level.poll_entity_loads_for_wait()?;
            if self.level.level.poll_entity_save_all()? {
                let requests = self.level.level.take_entity_storage_requests();
                self.level.generation().with_world_mut(|world| {
                    world.submit_entity_requests(requests, &self.properties)
                })?;
                let sync = self.level.generation().synchronize_entities(true)?;
                sync.await?;
                while self.level.poll_entity_loads_for_wait()? {}
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
    weather: &ferraloom_game::level::weather::WeatherData,
) -> Result<()> {
    use ferraloom_game::level::border::{self, WorldBorder};
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
    let saved = SavedDataStorage::new(world.access.dimension(&level.dimension)?.data);
    let mut border = saved
        .load::<WorldBorder>(border::SAVED_DATA_ID)?
        .map_or_else(WorldBorder::default, |data| data.value().clone());
    let _ = border.apply_initial_settings(level.game_time);
    border.set_absolute_max_size(properties.int("max-world-size"));
    level.level.set_entity_world_border(border);
    if level.level.rules().can_have_weather {
        level.level.prepare_weather(weather);
    }
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
