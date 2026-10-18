//! Single-region MinecraftServer phase driver. Game state has one mutable owner.
#[path = "command_runtime.rs"]
mod command_runtime;
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
    registries::{RegistrySet, core_pack, id},
    world::{ChunkLight, World, heightmap, surface_heights},
};
use anyhow::{Context, Result, bail, ensure};
use ferraloom_data::{
    EntityType,
    datapack::{Pack, ResourceManager},
};
use ferraloom_game::{
    Behaviors,
    entity::{
        EntityRef, EntityState,
        player::{GameType, MovementResult, PlayerBehavior, ServerPlayer, move_player},
    },
    level::{
        LevelError, Region, chunk_holder::ChunkStatus, chunk_map::PlayerChunkSender,
        distance_manager::tracking_view, server_level::ServerLevel,
    },
};
use ferraloom_protocol::{
    self as protocol, Uuid,
    chat::Component,
    codec::{BitSet, GlobalPos, Holder, VarInt, VarLong},
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
    chunks: PlayerChunkSender,
    view: HashSet<ChunkPos>,
    center: Option<ChunkPos>,
    teleport_id: i32,
    awaiting_teleport: Option<(i32, Vec3, u64)>,
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
}
impl Connection {
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
    access: AccessLists,
    level: ServerLevel,
    world: World,
    commands: Commands,
    game_rules: ferraloom_game::gamerules::GameRules,
    clocks: ferraloom_game::commands::clocks::ServerClockManager,
    saving_enabled: bool,
    idle_timeout: i32,
    default_game_mode: GameType,
    command_entity_tags: BTreeMap<String, Vec<EntityType>>,
    connections: BTreeMap<ConnectionId, Connection>,
    players: Vec<ConnectionId>,
    next_entity: u64,
    ingress: mpsc::Receiver<Ingress>,
    status: watch::Sender<status::ServerStatus>,
    shutdown: watch::Sender<bool>,
    tick: u64,
    stop: bool,
    empty_ticks: i32,
    was_paused: bool,
    autosave: i32,
    code_of_conduct: BTreeMap<String, String>,
    favicon: Option<Vec<u8>>,
    services: ServicesKeySet,
    enforces_secure_chat: bool,
    listener_task: Option<JoinHandle<Result<()>>>,
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
        let world = World::open(&options.root, &properties, &resources, &registries)?;
        let game_rules = command_runtime::load_game_rules(&world)?;
        let clocks = command_runtime::load_clocks(&world, &resources)?;
        let command_context = command_runtime::command_build_context(&registries, &resources);
        let command_entity_tags =
            ferraloom_data::datapack::load_tags(&resources, "entity_type", |id, _| {
                EntityType::from_name(&id.to_string())
            })
            .values
            .into_iter()
            .map(|(id, types)| (id.to_string(), types))
            .collect();
        let game_time = world.level_data.data.get_long_or("Time", 0);
        let mut level = ServerLevel::new(Region::new(&BEHAVIORS), "minecraft:overworld");
        level.game_time = game_time;
        let access = AccessLists::load(&options.root)?;
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
        let server = Self {
            properties: properties.clone(),
            root: options.root,
            registries,
            access,
            level,
            world,
            game_rules,
            clocks,
            command_entity_tags,
            saving_enabled: true,
            idle_timeout: properties.int("player-idle-timeout"),
            default_game_mode: game_type(properties.get("gamemode")),
            commands: Commands::new(command_context),
            connections: BTreeMap::new(),
            players: Vec::new(),
            next_entity: 0,
            ingress,
            status,
            shutdown,
            tick: 0,
            stop: false,
            empty_ticks: 0,
            was_paused: false,
            autosave: 6000,
            code_of_conduct: conduct,
            favicon,
            services,
            enforces_secure_chat,
            listener_task: Some(listener_task),
        };
        let task = tokio::spawn(server.run());
        Ok(RunningServer {
            address,
            ingress: send,
            task,
        })
    }
    /// Port of MinecraftServer#runServer's 50ms deadline and overload catch-up.
    async fn run(mut self) -> Result<()> {
        println!(
            "Ferraloom 26.3 running; single-region mode. Structures, carvers, features and spawn generation are passthrough."
        );
        let mut next = tokio::time::Instant::now();
        let mut last_overload = next;
        while !self.stop {
            let now = tokio::time::Instant::now();
            let behind = now.saturating_duration_since(next);
            if behind > Duration::from_secs(2)
                && now.duration_since(last_overload) > Duration::from_secs(15)
            {
                let skipped = behind.as_millis() / 50;
                eprintln!("Can't keep up! Running {skipped} ticks behind");
                next += Duration::from_millis(skipped as u64 * 50);
                last_overload = now;
            }
            self.process_packets().await?;
            self.tick_server().await?;
            next += Duration::from_millis(50);
            tokio::time::sleep_until(next).await;
        }
        self.stop_server().await
    }
    /// Port of MinecraftServer#processPacketsAndTick. No IO task applies play state.
    async fn process_packets(&mut self) -> Result<()> {
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
                        chunks: PlayerChunkSender::default(),
                        view: HashSet::new(),
                        center: None,
                        teleport_id: 0,
                        awaiting_teleport: None,
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
        self.empty_ticks = if self.players.is_empty() {
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
            self.tick += 1;
            // Functions execute before server clocks in this serial slot.
            self.clocks.tick(
                self.game_rules
                    .boolean(ferraloom_game::gamerules::GameRule::AdvanceTime),
            );
            if self.tick.is_multiple_of(20) {
                let packet = self.time_packet();
                for c in self.connections.values_mut() {
                    if matches!(c.phase, Phase::Play(_)) {
                        c.push(packet.clone());
                    }
                }
            }
            self.level.game_time += 1;
            self.level
                .level
                .tick_scheduled_blocks(self.level.game_time, 65536)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            // Fluid ticks and raids have no queued content in this generation slice.
            self.level.tickets.purge_stale(self.tick);
            self.integrate_jobs()?;
            self.track_players()?;
            // ChunkMap tracking precedes entity ticks, per ServerLevel#tick.
            self.level
                .level
                .tick_entities()
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            self.level.level.finish_entity_management();
        } else {
            self.integrate_jobs()?;
        }
        self.tick_connections().await?;
        self.send_chunks()?;
        self.flush_outbound();
        self.refresh_status();
        if !paused {
            self.autosave -= 1;
            if self.autosave <= 0 {
                self.save_all(false, false).await?;
                self.autosave = 6000;
            }
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
        let access = self.world.access.clone();
        let uuid = profile.id.to_string();
        let saved = tokio::task::spawn_blocking(move || access.player_data().load(&uuid)).await??;
        let mode = self.default_game_mode;
        let mut player = ServerPlayer::new(
            profile.id,
            profile.name,
            self.next_entity as i32,
            Vec3::new(
                f64::from(self.world.spawn.x) + 0.5,
                f64::from(self.world.spawn.y),
                f64::from(self.world.spawn.z) + 0.5,
            ),
            mode,
        );
        self.next_entity += 1;
        if let Some(saved) = saved {
            player.load(saved);
        }
        if self.properties.boolean("force-gamemode") {
            player.game_mode = mode;
        }
        player.view_distance = i32::from(self.connections[&connection].information.view_distance);
        let center = player.chunk_position();
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
            if let Some(request) = self.level.chunks.request(pos) {
                self.world.request(request, pos);
            }
        }
        Ok(())
    }
    fn integrate_jobs(&mut self) -> Result<()> {
        for (request, terrain) in self.world.poll_terrain()? {
            let pos = terrain.pos;
            if pos == ChunkPos::containing(self.world.spawn)
                && !self.world.level_data.data.contains_key("spawn")
            {
                self.world.spawn.y = terrain.surface
                    [((self.world.spawn.z & 15) * 16 + (self.world.spawn.x & 15)) as usize];
            }
            let chunk = self.world.integrate(request, terrain)?;
            self.level
                .level
                .add_chunk(chunk)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            self.level
                .chunks
                .holders
                .get_mut(&pos.pack())
                .unwrap()
                .status = ChunkStatus::InitializeLight;
        }
        while let Ok(light) = self.world.light_rx.try_recv() {
            let holder = self
                .level
                .chunks
                .holders
                .get_mut(&light.pos.pack())
                .context("unknown light job")?;
            ensure!(holder.request_id == light.request, "stale light completion");
            holder.status = ChunkStatus::Full;
            self.world.ready.insert(light.pos);
            for (pos, data) in light.updates {
                self.world.lights.insert(pos, data);
            }
        }
        Ok(())
    }
    fn place_player(&mut self, connection: ConnectionId) -> Result<()> {
        let mut c = self
            .connections
            .remove(&connection)
            .context("missing joining connection")?;
        let mut player = c.player.take().context("spawn not prepared")?;
        if player.saved.is_empty() {
            player.position.y = f64::from(self.world.spawn.y);
            player.first_good = player.position;
            player.last_good = player.position;
        }
        let entity = EntityRef(self.next_entity);
        self.next_entity += 1;
        let pos = player.position;
        let yaw = player.yaw;
        let pitch = player.pitch;
        let center = player.chunk_position();
        let mode = protocol_game_type(player.game_mode);
        let entity_id = player.network_id;
        let permission = self.access.permission(player.uuid);
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
                seed: ferraloom_worldgen::biome_manager::obfuscate_seed(self.world.seed),
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
        c.push(cb::Packet::ChangeDifficulty(cb::ChangeDifficulty {
            difficulty: match self.properties.get("difficulty") {
                "peaceful" => Difficulty::Peaceful,
                "normal" => Difficulty::Normal,
                "hard" => Difficulty::Hard,
                _ => Difficulty::Easy,
            },
            locked: false,
        }));
        c.push(abilities(&player));
        c.push(cb::Packet::SetHeldSlot(cb::SetHeldSlot { slot: VarInt(0) }));
        c.push(cb::Packet::UpdateRecipes(cb::UpdateRecipes {
            item_sets: Vec::new(),
            stonecutter_recipes: Vec::new(),
        }));
        c.push(cb::Packet::EntityEvent(cb::EntityEvent {
            entity_id,
            event_id: 24 + permission.clamp(0, 4) as i8,
        }));
        c.push(self.commands.packet(permission));
        c.push(cb::Packet::RecipeBookSettings(
            cb::RecipeBookSettingsPacket {
                book_settings: RecipeBookSettings(std::array::from_fn(|_| {
                    RecipeBookTypeSettings {
                        open: false,
                        filtering: false,
                    }
                })),
            },
        ));
        c.push(cb::Packet::RecipeBookAdd(cb::RecipeBookAdd {
            entries: Vec::new(),
            replace: true,
        }));
        self.broadcast(cb::Packet::SystemChat(Box::new(cb::SystemChat {
            content: Component::translatable(
                "multiplayer.player.joined",
                vec![protocol::chat::Argument::Component(Box::new(
                    player.display_name(),
                ))],
            ),
            overlay: false,
        })));
        teleport(&mut c, pos, yaw, pitch, self.tick);
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
        c.push(cb::Packet::InitializeBorder(cb::InitializeBorder {
            new_center_x: 0.,
            new_center_z: 0.,
            old_size: 59_999_968.,
            new_size: 59_999_968.,
            lerp_time: VarLong(0),
            new_absolute_max_size: VarInt(self.properties.int("max-world-size")),
            warning_blocks: VarInt(5),
            warning_time: VarInt(15),
        }));
        c.push(self.time_packet());
        c.push(cb::Packet::SetDefaultSpawnPosition(
            cb::SetDefaultSpawnPosition {
                respawn_data: RespawnData {
                    global_pos: GlobalPos {
                        dimension: id("overworld"),
                        pos: self.world.spawn,
                    },
                    yaw: 0.,
                    pitch: 0.,
                },
            },
        ));
        c.push(cb::Packet::GameEvent(cb::GameEvent {
            event: 13,
            param: 0.,
        }));
        c.push(cb::Packet::TickingState(cb::TickingState {
            tick_rate: 20.,
            is_frozen: false,
        }));
        c.push(cb::Packet::TickingStep(cb::TickingStep {
            tick_steps: VarInt(0),
        }));
        c.push(cb::Packet::SetChunkCacheCenter(cb::SetChunkCacheCenter {
            x: VarInt(center.x),
            z: VarInt(center.z),
        }));
        c.center = Some(center);
        c.push(cb::Packet::PostEffects(common_cb::PostEffects {
            post_effects: Vec::new(),
        }));
        c.push(cb::Packet::ContainerSetContent(cb::ContainerSetContent {
            container_id: VarInt(0),
            state_id: VarInt(1),
            items: vec![protocol::component::ItemStack::default(); 46],
            carried_item: protocol::component::ItemStack::default(),
        }));
        use protocol::entity_data::{DataValue, EntityDataItems, EntityDataValue};
        c.push(cb::Packet::SetEntityData(cb::SetEntityData {
            id: VarInt(entity_id),
            packed_items: EntityDataItems(vec![
                DataValue {
                    id: 9,
                    value: EntityDataValue::Float(player.parent.health),
                },
                DataValue {
                    id: 16,
                    value: EntityDataValue::Byte(c.information.model_customisation as i8),
                },
            ]),
        }));
        c.push(cb::Packet::UpdateAdvancements(cb::UpdateAdvancements {
            should_reset: true,
            added: Vec::new(),
            removed: Vec::new(),
            progress: Vec::new(),
            show_advancements: true,
        }));
        c.push(cb::Packet::SetHealth(cb::SetHealth {
            health: player.parent.health,
            food: VarInt(20),
            saturation: 5.,
        }));
        c.push(cb::Packet::SetExperience(cb::SetExperience {
            experience_progress: 0.,
            experience_level: VarInt(0),
            total_experience: VarInt(0),
        }));
        self.level
            .level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        println!(
            "{} joined at {}, {}, {}",
            c.profile.name, pos.x, pos.y, pos.z
        );
        self.players.push(connection);
        self.connections.insert(connection, c);
        Ok(())
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
            sb::Packet::AcceptTeleportation(p) => {
                if let Some((pending, _, _)) = self.connections[&connection].awaiting_teleport
                    && p.id.0 == pending
                {
                    self.connections
                        .get_mut(&connection)
                        .unwrap()
                        .awaiting_teleport = None;
                    self.movement(
                        connection,
                        entity,
                        Vec3::new(p.x, p.y, p.z),
                        (p.y_rot, p.x_rot),
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
                .connections
                .get_mut(&connection)
                .unwrap()
                .chunks
                .acknowledge(p.desired_chunks_per_tick),
            sb::Packet::KeepAlive(p) => self.keepalive_reply(connection, p.id)?,
            sb::Packet::PlayerLoaded(_) => {
                self.level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .loaded = true
            }
            sb::Packet::ClientInformation(p) => {
                self.level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?
                    .view_distance = i32::from(p.view_distance);
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
                        || self.access.permission(c.profile.id) > 0,
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
                    Err(e) => self.feedback(Some(connection), &e.to_string()),
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
                let c = self.connections.get_mut(&connection).unwrap();
                let last_seen = c
                    .last_seen
                    .apply(&p.last_seen_messages)
                    .context("multiplayer.disconnect.chat_validation_failed")?;
                let permission = self.access.permission(c.profile.id);
                let source = CommandSourceStack::server(
                    permission,
                    self.level
                        .level
                        .entity::<ServerPlayer>(entity)
                        .map_err(game_error)?
                        .position,
                );
                let prepared = match self.commands.prepare(&p.command, source.clone()) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        self.command_syntax_failure(Some(connection), &source, &error, &p.command);
                        return Ok(());
                    }
                };
                if let Some((argument_name, message)) =
                    prepared.arguments.iter().find_map(|(name, value)| {
                        if let ferraloom_game::commands::arguments::ArgumentValue::Message(
                            message,
                        ) = value
                        {
                            Some((name, message))
                        } else {
                            None
                        }
                    })
                {
                    let content = message.text.clone();
                    let signature = p
                        .argument_signatures
                        .0
                        .iter()
                        .find(|argument| argument.name.0 == *argument_name)
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
                    c.command_message = Some((argument_name.clone(), validated));
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
                if matches!(player.game_mode, GameType::Creative | GameType::Spectator) {
                    player.flying = p.is_flying;
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
            sb::Packet::PlayerInput(_)
            | sb::Packet::CustomPayload(_)
            | sb::Packet::Pong(_)
            | sb::Packet::PlayerCommand(_)
            | sb::Packet::RecipeBookChangeSettings(_)
            | sb::Packet::RecipeBookSeenRecipe(_)
            | sb::Packet::SeenAdvancements(_) => {}
            // These gameplay ports are outside the terrain/join milestone.
            _ => {}
        }
        Ok(())
    }
    fn player(&self, entity: EntityRef) -> Result<&ServerPlayer> {
        self.level
            .level
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)
    }
    async fn movement(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        target: Vec3,
        rotation: (f32, f32),
        on_ground: bool,
    ) -> Result<()> {
        let (yaw, pitch) = rotation;
        if self.connections[&connection].awaiting_teleport.is_some() {
            return Ok(());
        }
        if [target.x, target.y, target.z].iter().any(|v| v.is_nan())
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            bail!("multiplayer.disconnect.invalid_player_movement");
        }
        let origin = self.player(entity)?.position;
        if target.distance_to_sqr(origin) > 10000. {
            let player = self.player(entity)?;
            let (yaw, pitch) = (player.yaw, player.pitch);
            teleport(
                self.connections.get_mut(&connection).unwrap(),
                origin,
                yaw,
                pitch,
                self.tick,
            );
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
        let result = move_player(&mut self.level.level, entity, target, yaw, pitch, on_ground)
            .map_err(game_error)?;
        match result {
            MovementResult::Invalid => bail!("multiplayer.disconnect.invalid_player_movement"),
            MovementResult::TooQuickly | MovementResult::Wrongly => {
                let player = self.player(entity)?;
                let (position, yaw, pitch) = (player.position, player.yaw, player.pitch);
                teleport(
                    self.connections.get_mut(&connection).unwrap(),
                    position,
                    yaw,
                    pitch,
                    self.tick,
                );
            }
            MovementResult::Accepted => {
                let player = self
                    .level
                    .level
                    .entity_mut::<ServerPlayer>(entity)
                    .map_err(game_error)?;
                player.known_movement = player.position - origin;
                let c = self.connections.get_mut(&connection).unwrap();
                c.received_movement_this_tick = true;
                if player.known_movement.length_sqr() > 9.999999747378752E-6 {
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
            let mut leaving = c.view.difference(&desired).copied().collect::<Vec<_>>();
            leaving.sort_by_key(|p| p.pack());
            for pos in leaving {
                if c.chunks.drop_chunk(pos) {
                    c.push(cb::Packet::ForgetLevelChunk(cb::ForgetLevelChunk {
                        pos: play::ChunkPos { x: pos.x, z: pos.z },
                    }));
                }
            }
            for &pos in desired.difference(&c.view) {
                c.chunks.pending.insert(pos);
            }
            c.view = desired;
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
                            self.world
                                .ready
                                .contains(&ChunkPos::new(center.x + x, center.z + z))
                        })
                    }) {
                        joining.push(id);
                    }
                }
                Phase::Play(entity) => {
                    let player = self
                        .level
                        .level
                        .entity_mut::<ServerPlayer>(entity)
                        .map_err(game_error)?;
                    player.do_tick();
                    if let Some((_, position, tick)) = c.awaiting_teleport
                        && self.tick.saturating_sub(tick) > 20
                    {
                        teleport(c, position, player.yaw, player.pitch, self.tick);
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
            c.phase = Phase::Configuration(ConfigTask::Finishing);
            c.push(cfg::ClientboundPacket::FinishConfiguration(
                cfg::FinishConfiguration {},
            ));
        }
        Ok(())
    }
    fn keepalive_reply(&mut self, connection: ConnectionId, key: i64) -> Result<()> {
        let c = self.connections.get_mut(&connection).unwrap();
        ensure!(c.keepalive == Some(key), "disconnect.timeout");
        c.keepalive = None;
        Ok(())
    }
    fn send_chunks(&mut self) -> Result<()> {
        let ready = &self.world.ready;
        let mut batches = Vec::new();
        for (&connection, c) in &mut self.connections {
            if c.close {
                continue;
            }
            if let Phase::Play(_) = c.phase {
                let positions = c.chunks.next_batch(c.center.unwrap(), |pos| {
                    ready.contains(&pos)
                        && (-1..=1).all(|x| {
                            (-1..=1).all(|z| ready.contains(&ChunkPos::new(pos.x + x, pos.z + z)))
                        })
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
                let heights = surface_heights(&chunk.sections().cloned().collect::<Vec<_>>());
                packets.push(cb::Packet::LevelChunkWithLight(cb::LevelChunkWithLight {
                    x: pos.x,
                    z: pos.z,
                    chunk_data: LevelChunkPacketData {
                        heightmaps: vec![
                            Heightmap {
                                kind: HeightmapType::WorldSurface,
                                data: heightmap(&heights),
                            },
                            Heightmap {
                                kind: HeightmapType::MotionBlocking,
                                data: heightmap(&heights),
                            },
                        ],
                        sections: chunk.sections().map(LevelChunkSectionData::from).collect(),
                        block_entities: Vec::new(),
                    },
                    light_data: light_packet(self.world.lights.get(&pos.pack())),
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
        self.clock_packet(None)
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
            let tag = player.save();
            let uuid = player.uuid.to_string();
            let name = player.display_name();
            let access = self.world.access.clone();
            tokio::task::spawn_blocking(move || access.player_data().save(&uuid, &tag)).await??;
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
        let (permission, position) = if let Some(id) = source {
            let c = self
                .connections
                .get_mut(&id)
                .context("unknown command source")?;
            c.command_spam += 20;
            c.last_action = Instant::now();
            ensure!(
                c.command_spam
                    <= self
                        .properties
                        .int("command-spam-threshold-seconds")
                        .saturating_mul(20)
                    || self.access.permission(c.profile.id) > 0,
                "disconnect.spam"
            );
            let Phase::Play(entity) = c.phase else {
                bail!("command source not playing");
            };
            (
                self.access.permission(c.profile.id),
                self.player(entity)?.position,
            )
        } else {
            (4, Vec3::ZERO)
        };
        let mut stack = CommandSourceStack::server(permission, position);
        if let Some(connection) = source {
            let c = &self.connections[&connection];
            if let Phase::Play(entity) = c.phase {
                let player = self.player(entity)?;
                stack = stack
                    .with_entity(entity)
                    .with_source(ferraloom_game::commands::source::CommandSource::Player(
                        entity,
                    ))
                    .with_rotation(ferraloom_game::commands::source::Rotation {
                        pitch: player.pitch,
                        yaw: player.yaw,
                    });
            }
            if let Some((argument_name, message)) = &c.command_message {
                stack.signing_context.arguments = Arc::from([(
                    argument_name.clone(),
                    ferraloom_game::commands::source::SignedArgument {
                        sender: message.sender,
                        index: message.index,
                        signature: message
                            .signature
                            .as_ref()
                            .map(|signature| Arc::from(signature.0.as_slice())),
                    },
                )]);
            }
        }
        let action = match self.commands.parse(text, stack.clone()) {
            Ok(action) => action,
            Err(error) => {
                self.command_syntax_failure(source, &stack, &error, text);
                return Ok(());
            }
        };
        match action {
            CommandAction::Run(command) => {
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
        Ok(())
    }
    async fn save_all(&mut self, flush: bool, force: bool) -> Result<()> {
        self.save_game_rules().await?;
        let snapshots = self
            .level
            .chunks
            .holders
            .values()
            .filter(|_| self.saving_enabled || force)
            .filter_map(|holder| self.level.level.chunk(holder.position))
            .map(|chunk| self.world.capture_chunk(chunk, self.level.game_time))
            .collect();
        let encoded = self.world.encode_saves(snapshots).await?;
        for (pos, tag) in encoded {
            self.world.io.store(pos, Some(tag)).await?;
        }
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
        self.world
            .save_level_and_players(self.level.game_time, players)
            .await?;
        self.world.io.synchronize(flush).await?;
        Ok(())
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
        self.world.io.shutdown().await?;
        println!(
            "Server stopped; world and players saved to {}",
            self.root.display()
        );
        Ok(())
    }
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
    let privileged = matches!(player.game_mode, GameType::Creative | GameType::Spectator);
    cb::Packet::PlayerAbilities(cb::PlayerAbilities {
        invulnerable: privileged,
        is_flying: player.flying,
        can_fly: privileged,
        instabuild: player.game_mode == GameType::Creative,
        flying_speed: 0.05,
        walking_speed: 0.1,
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
fn teleport(c: &mut Connection, position: Vec3, yaw: f32, pitch: f32, tick: u64) {
    c.teleport_id = if c.teleport_id == i32::MAX {
        0
    } else {
        c.teleport_id + 1
    };
    c.awaiting_teleport = Some((c.teleport_id, position, tick));
    c.push(cb::Packet::PlayerPosition(cb::PlayerPosition {
        id: VarInt(c.teleport_id),
        change: PositionMoveRotation {
            position: play::Vec3 {
                x: position.x,
                y: position.y,
                z: position.z,
            },
            delta_movement: play::Vec3 {
                x: 0.,
                y: 0.,
                z: 0.,
            },
            y_rot: yaw,
            x_rot: pitch,
        },
        relatives: 0,
    }));
}
fn light_packet(light: Option<&ChunkLight>) -> LightUpdatePacketData {
    let mut sky_mask = 0;
    let mut block_mask = 0;
    let mut empty_sky = 0;
    let mut empty_block = 0;
    let mut sky_updates = Vec::new();
    let mut block_updates = Vec::new();
    for (index, y) in (-5..=20).enumerate() {
        for (sky, mask, empty, updates) in [
            (true, &mut sky_mask, &mut empty_sky, &mut sky_updates),
            (false, &mut block_mask, &mut empty_block, &mut block_updates),
        ] {
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
