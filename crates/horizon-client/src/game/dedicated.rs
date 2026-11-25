//! The dedicated server (net.minecraft.server.Main and DedicatedServer):
//! `horizon-client --dedicated [--nogui] [--port N] [--world NAME]
//! [--universe DIR] [--initSettings]`. It reads server.properties and
//! eula.txt from the working directory, opens `<universe>/<level-name>` and
//! runs the same game as single player, without a local player: the world,
//! its simulation and the play-state hub that LAN hosting uses, configured
//! from server.properties, with the operator, whitelist and ban lists and a
//! console on standard input.
use super::{Game, Options, lan::LanSettings, world_loading::WorldRequest};
use horizon_client::{
    Result,
    lan::{LoginGate, NetConfig, NetLog, OnlineMode, hub::HubConfig},
    server::{
        eula, profiles,
        properties::{GAME_MODES, ServerProperties, SystemEntropy},
        user_cache::UserCache,
        user_lists::{BanList, IpBanList, NameAndId, OpList, WhiteList, format_date},
    },
    text::Text,
};
use horizon_protocol::text::translatable;
use horizon_world::nbt::{JavaString, Tag};
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};

mod status;

/// Double.toString for the magnitudes positions have (no exponent form
/// between 1e-3 and 1e7).
pub(super) fn java_double(v: f64) -> String {
    if v == 0.0 || (1e-3..1e7).contains(&v.abs()) {
        format!("{v:?}")
    } else {
        let s = format!("{v:E}");
        let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
        let m = if m.contains('.') {
            m.to_owned()
        } else {
            format!("{m}.0")
        };
        format!("{m}E{e}")
    }
}

/// What the startup read before the game's resources were loaded.
struct Setup {
    props: ServerProperties,
    universe: PathBuf,
    level_name: String,
    port: Option<u16>,
    started: Instant,
}
static SETUP: OnceLock<Mutex<Option<Setup>>> = OnceLock::new();
/// usercache.json (Services.nameToIdCache): used by the server thread only.
static USER_CACHE: Mutex<Option<UserCache>> = Mutex::new(None);

/// PlayerList.placeNewPlayer: a player who joined is cached (and the file saved).
pub(super) fn player_joined(id: u128, name: &str) {
    if let Some(cache) = USER_CACHE.lock().unwrap().as_mut() {
        cache.add(NameAndId {
            id,
            name: name.to_owned(),
        });
    }
}

/// Log4j's console pattern: `[HH:mm:ss] [thread/LEVEL]: message`.
pub(super) fn log(level: &str, message: &str) {
    log_from("Server thread", level, message);
}

/// A line from another of vanilla's threads (the login's User Authenticator).
pub(super) fn log_from(thread: &str, level: &str, message: &str) {
    let file_level = match level {
        "WARN" => ::log::Level::Warn,
        "ERROR" => ::log::Level::Error,
        _ => ::log::Level::Info,
    };
    horizon_client::logging::write_as(thread, file_level, message);
    CAPTURE.with_borrow_mut(|lines| {
        if let Some(lines) = lines {
            lines.push(message.to_owned());
        }
    });
    // Log4j's %d{HH:mm:ss}: the local time, as in logs/latest.log.
    // The launcher can close its stdout pipe while the server keeps running.
    let _ = writeln!(
        std::io::stdout().lock(),
        "[{}] [{thread}/{level}]: {message}",
        chrono::Local::now().format("%H:%M:%S")
    );
}

thread_local! {
    /// The lines logged while a control request runs a console command.
    static CAPTURE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` and returns the lines it logged.
fn capture_log(f: impl FnOnce()) -> Vec<String> {
    CAPTURE.set(Some(Vec::new()));
    f();
    CAPTURE.take().unwrap_or_default()
}

/// Main.main up to the server start: arguments, server.properties (stored
/// back at once), eula.txt. False when the process should stop here.
pub(super) fn preflight(options: &mut Options) -> Result<bool> {
    let args = options.dedicated.clone().unwrap_or_default();
    let mut universe = PathBuf::from(".");
    let mut world = None;
    let mut port = None;
    let mut init_settings = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let name = arg.trim_start_matches('-');
        match name {
            "nogui"
            | "demo"
            | "bonusChest"
            | "forceUpgrade"
            | "eraseCache"
            | "recreateRegionFiles"
            | "safeMode"
            | "jfrProfile" => {}
            "initSettings" => init_settings = true,
            "help" => {
                eprintln!(
                    "Option                Description\n------                -----------\n--help\n--initSettings        Initializes 'server.properties' and 'eula.txt', then quits\n--nogui\n--port <Integer>      (default: -1)\n--universe <String>   (default: .)\n--world <String>"
                );
                return Ok(false);
            }
            "universe" => universe = PathBuf::from(it.next().ok_or("--universe needs a value")?),
            "world" => world = Some(it.next().ok_or("--world needs a value")?.clone()),
            "port" => {
                let v: i32 = it.next().ok_or("--port needs a value")?.parse()?;
                port = u16::try_from(v).ok();
            }
            "serverId" | "pidFile" => {
                let value = it.next().ok_or("missing value")?;
                if name == "pidFile" {
                    std::fs::write(value, std::process::id().to_string())?;
                }
            }
            _ => return Err(format!("{arg} is not a recognized option").into()),
        }
    }
    let started = Instant::now();
    let settings = PathBuf::from("server.properties");
    let props = ServerProperties::from_file(&settings, &mut SystemEntropy::default())?;
    for w in &props.warnings {
        log("WARN", w);
    }
    // Horizon's own commands are the owner's choice, off as on vanilla.
    if props
        .raw
        .get(horizon_client::command::vanilla::PREGEN_PROPERTY)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    {
        horizon_client::command::vanilla::set_horizon_commands(true);
    }
    // Commands.CommandSelection.DEDICATED: `/op`, `/ban`, `/stop`... for
    // the console and the players alike (game/commands/server_cmds.rs).
    horizon_client::command::vanilla::set_dedicated(true);
    props.store(&settings)?;
    let eula_file = PathBuf::from("eula.txt");
    let agreed = match eula::agreed(&eula_file) {
        Ok(a) => a,
        Err(w) => {
            log("WARN", &w);
            false
        }
    };
    if init_settings {
        let abs = |p: &PathBuf| std::env::current_dir().map_or(p.clone(), |d| d.join(p));
        log(
            "INFO",
            &format!(
                "Initialized '{}' and '{}'",
                abs(&settings).display(),
                abs(&eula_file).display()
            ),
        );
        return Ok(false);
    }
    if !agreed {
        log(
            "INFO",
            "You need to agree to the EULA in order to run the server. Go to eula.txt for more info.",
        );
        return Ok(false);
    }
    let level_name = world.unwrap_or_else(|| props.level_name.clone());
    SETUP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .replace(Setup {
            props,
            universe,
            level_name,
            port,
            started,
        });
    Ok(true)
}

/// The operator, whitelist and ban lists (DedicatedPlayerList), shared with
/// the connection threads that check logins.
pub(super) struct Lists {
    pub ops: OpList,
    pub white: WhiteList,
    pub bans: BanList,
    pub ip_bans: IpBanList,
    pub use_whitelist: bool,
    pub enforce_whitelist: bool,
}
impl Lists {
    fn load() -> Self {
        let mut l = Self {
            ops: OpList::new("ops.json"),
            white: WhiteList::new("whitelist.json"),
            bans: BanList::new("banned-players.json"),
            ip_bans: IpBanList::new("banned-ips.json"),
            use_whitelist: false,
            enforce_whitelist: false,
        };
        // DedicatedPlayerList's constructor order and its LOGGER.warn lines.
        let warn = |what: &str, r: std::result::Result<(), String>| {
            if let Err(e) = r {
                log("WARN", &format!("Failed to {what}: {e}"));
            }
        };
        let saved = |r: std::io::Result<()>| r.map_err(|e| e.to_string());
        warn("load user banlist", l.bans.load());
        warn("save user banlist", saved(l.bans.save()));
        warn("load ip banlist", l.ip_bans.load());
        warn("save ip banlist", saved(l.ip_bans.save()));
        warn("load operators list", l.ops.load());
        warn("load white-list", l.white.load());
        warn("save operators list", saved(l.ops.save()));
        if !l.white.file.exists() {
            warn("save white-list", saved(l.white.save()));
        }
        l
    }
    /// PlayerList.isWhiteListed.
    fn whitelisted(&self, user: &NameAndId) -> bool {
        !self.use_whitelist || self.ops.contains(user) || self.white.contains(user)
    }
    /// PlayerList.canPlayerLogin before the player count.
    fn can_login(&mut self, address: std::net::SocketAddr, user: &NameAndId) -> Option<Tag> {
        if self.bans.contains(user)
            && let Some(ban) = self.bans.get(user).cloned()
        {
            let mut reason = translatable_tag(
                "multiplayer.disconnect.banned.reason",
                vec![ban_reason(&ban.reason)],
            );
            if let Some(expires) = ban.expires {
                append(
                    &mut reason,
                    translatable_tag(
                        "multiplayer.disconnect.banned.expiration",
                        vec![Tag::String(JavaString::from(ban_date(expires).as_str()))],
                    ),
                );
            }
            return Some(reason);
        }
        if !self.whitelisted(user) {
            return Some(translatable("multiplayer.disconnect.not_whitelisted", &[]));
        }
        let ip = address.ip().to_string();
        if self.ip_bans.contains(&ip)
            && let Some(ban) = self.ip_bans.get(&ip).cloned()
        {
            let mut reason = translatable_tag(
                "multiplayer.disconnect.banned_ip.reason",
                vec![ban_reason(&ban.reason)],
            );
            if let Some(expires) = ban.expires {
                append(
                    &mut reason,
                    translatable_tag(
                        "multiplayer.disconnect.banned_ip.expiration",
                        vec![Tag::String(JavaString::from(ban_date(expires).as_str()))],
                    ),
                );
            }
            return Some(reason);
        }
        None
    }
}

/// The world presets of 26.3's `worldgen/world_preset`.
const WORLD_PRESETS: [&str; 6] = [
    "minecraft:normal",
    "minecraft:flat",
    "minecraft:large_biomes",
    "minecraft:amplified",
    "minecraft:single_biome_surface",
    "minecraft:debug_all_block_states",
];

/// Identifier.tryParse: `namespace:path`, or a path in `minecraft`.
fn parse_identifier(text: &str) -> Option<String> {
    let (namespace, path) = text.split_once(':').unwrap_or(("minecraft", text));
    let valid = |s: &str, extra: &[char]| {
        s.chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-') || extra.contains(&c))
    };
    (valid(namespace, &[]) && valid(path, &['/'])).then(|| format!("{namespace}:{path}"))
}

impl Game<'_> {
    /// DedicatedServerProperties.createDimensions for a world that doesn't
    /// exist yet: `level-type`'s preset, a flat world's `generator-settings`
    /// and `generate-structures`, written before the world opens so that
    /// streaming reads them. Horizon generates the normal and flat presets.
    fn write_world_gen(&self, dir: &std::path::Path, props: &ServerProperties) -> Result<()> {
        let level_type = &props.level_type;
        let preset = parse_identifier(level_type)
            .filter(|id| WORLD_PRESETS.contains(&id.as_str()))
            .unwrap_or_else(|| {
                log_from(
                    "Worker-Main-1",
                    "WARN",
                    &format!(
                        "Failed to parse level-type {level_type}, defaulting to minecraft:normal"
                    ),
                );
                "minecraft:normal".to_owned()
            });
        let flat = preset == "minecraft:flat";
        if !flat && preset != "minecraft:normal" {
            log_from(
                "Worker-Main-1",
                "WARN",
                &format!(
                    "Horizon doesn't generate the {preset} world preset yet: using minecraft:normal"
                ),
            );
        }
        let settings = if flat {
            let biomes = self.res.biomes();
            let (settings, errors) = horizon_client::flatgen::settings_from_json(
                &props.generator_settings,
                |b| self.pack.assets.registry.default_state(b).is_some(),
                |b| biomes.get(biomes.id(b)).name == b,
            );
            for e in errors {
                log_from("Worker-Main-1", "ERROR", &e);
            }
            settings
        } else {
            None
        };
        let options = horizon_client::save::WorldGen {
            flat,
            structures: props.generate_structures,
            bonus_chest: false,
        };
        horizon_client::save::write_world_gen_settings_with(dir, props.seed, options, settings)
    }
}

/// PlayerList.BAN_DATE_FORMAT ("yyyy-MM-dd 'at' HH:mm:ss z"), in UTC.
fn ban_date(millis: i64) -> String {
    let s = format_date(millis);
    format!("{} at {} UTC", &s[..10], &s[11..19])
}
/// BanListEntry.getReasonMessage.
fn ban_reason(reason: &Option<String>) -> Tag {
    match reason {
        Some(r) => Tag::String(JavaString::from(r.as_str())),
        None => translatable("multiplayer.disconnect.banned.reason.default", &[]),
    }
}
fn translatable_tag(key: &str, args: Vec<Tag>) -> Tag {
    Tag::compound([
        ("translate", Tag::String(JavaString::from(key))),
        ("with", Tag::List(args)),
    ])
}
/// MutableComponent.append.
fn append(to: &mut Tag, sibling: Tag) {
    if let Tag::Compound(map) = to {
        let extra = map
            .entry(JavaString::from("extra"))
            .or_insert_with(|| Tag::List(Vec::new()));
        if let Tag::List(items) = extra {
            items.push(sibling);
        }
    }
}

/// What the dedicated-only commands read and change (DedicatedServer and
/// its DedicatedPlayerList), for the console's commands and the players'
/// alike (game/commands/server_cmds.rs). Set while `run_dedicated` runs.
pub(super) struct Admin {
    pub lists: Arc<Mutex<Lists>>,
    /// `/stop` ran (MinecraftServer.halt): the loop ends after this tick.
    pub stop: bool,
    /// The autosave (`ServerLevel.noSave`, `/save-off` and `/save-on`).
    pub autosave: bool,
    /// `op-permission-level`: what `/op` grants (operatorUserPermissions).
    pub op_level: i32,
    /// `player-idle-timeout` in minutes, which `/setidletimeout` changes.
    pub idle_timeout: i32,
    /// Players' `/save-all`, done once their turn is over.
    pub saves: Vec<super::commands::server_cmds::DeferredSave>,
}
static ADMIN: Mutex<Option<Admin>> = Mutex::new(None);

/// `f` on the dedicated server's [`Admin`]; None when no dedicated server
/// runs. Never call it again inside `f`.
pub(super) fn admin<T>(f: impl FnOnce(&mut Admin) -> T) -> Option<T> {
    ADMIN.lock().unwrap().as_mut().map(f)
}

/// What the server loop keeps between ticks.
struct ServerState {
    console: mpsc::Receiver<String>,
    empty_ticks: i32,
    /// When the last passes of the loop ended, for the tick rate horizon-server shows.
    ticks: std::collections::VecDeque<Instant>,
    paused: bool,
    /// `broadcast-console-to-ops`.
    broadcast_console: bool,
}

/// The replies of the console's own `kick` vanilla sends with `broadcast`
/// (`sendSuccess(..., true)`): the operators hear of them. The dispatcher's
/// commands broadcast through `Game::feedback`.
const BROADCAST: &[&str] = &["commands.kick.success"];

impl Game<'_> {
    /// DedicatedServer.initServer and MinecraftServer.runServer.
    pub(super) fn run_dedicated(&mut self) -> Result<()> {
        // This thread ticks the server (MinecraftServer.spin's priority).
        horizon_threads::favour_current();
        // Optional in-process CPU sampling of the dedicated server.
        #[cfg(all(feature = "profiler", target_os = "linux"))]
        horizon_client::profiler::start();
        self.join_world_rest()?;
        let Setup {
            props,
            universe,
            level_name,
            port,
            started,
        } = SETUP
            .get()
            .and_then(|m| m.lock().unwrap().take())
            .ok_or("dedicated server not prepared")?;
        // Main's WorldLoader: a new world's dimensions come from
        // server.properties before the server starts.
        let world_dir = std::env::current_dir()?.join(&universe).join(&level_name);
        let new_world = !world_dir.join("level.dat").exists();
        if new_world {
            self.write_world_gen(&world_dir, &props)?;
        }
        // The console thread starts first, as in initServer.
        let (tx, console) = mpsc::channel();
        std::thread::Builder::new()
            .name("Server console handler".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut line = String::new();
                while stdin.read_line(&mut line).is_ok_and(|n| n > 0) {
                    if tx
                        .send(line.trim_end_matches(['\r', '\n']).to_owned())
                        .is_err()
                    {
                        break;
                    }
                    line.clear();
                }
            })?;
        // Main: Services.create loads the universe's usercache.json.
        let (mut cache, warnings) = UserCache::load(universe.join("usercache.json"));
        for warning in warnings {
            log_from("main", "WARN", &warning);
        }
        // DedicatedServer.initServer: names nobody has resolve offline only without authentication.
        cache.resolve_offline_users = !props.online_mode;
        *USER_CACHE.lock().unwrap() = Some(cache);
        // Services.create: the services' public keys, fetched in the
        // background (they check players' chat keys).
        let secure_chat = Arc::new(horizon_client::lan::SecureChat::new(
            props.enforce_secure_profile && props.online_mode,
            || {
                let discovery = profiles::discovery_url();
                profiles::fetch_profile_key_keys(&discovery).unwrap_or_else(|e| {
                    ::log::error!("{e}");
                    Vec::new()
                })
            },
        ));
        log(
            "INFO",
            &format!(
                "Starting minecraft server version {}",
                horizon_core::VANILLA_VERSION
            ),
        );
        log("INFO", "Loading properties");
        let game_mode = props.game_mode;
        log(
            "INFO",
            &format!(
                "Default game type: {}",
                GAME_MODES[game_mode as usize].to_uppercase()
            ),
        );
        let port = port.unwrap_or(props.server_port as u16);
        // MinecraftServer.initializeKeyPair (only online mode uses it).
        log("INFO", "Generating keypair");
        let online = if props.online_mode {
            let key = horizon_protocol::rsa::RsaPrivateKey::generate(1024)
                .map_err(|e| format!("Failed to generate key pair: {e}"))?;
            Some(Arc::new(OnlineMode {
                public_der: key.public_der(),
                key,
                prevent_proxy: props.prevent_proxy_connections,
            }))
        } else {
            None
        };
        log(
            "INFO",
            &format!(
                "Starting Minecraft server on {}:{port}",
                if props.server_ip.is_empty() {
                    "*"
                } else {
                    &props.server_ip
                }
            ),
        );
        if online.is_none() {
            log("WARN", "**** SERVER IS RUNNING IN OFFLINE/INSECURE MODE!");
            log(
                "WARN",
                "The server will make no attempt to authenticate usernames. Beware.",
            );
            log(
                "WARN",
                "While this makes the game possible to play without internet access, it also opens up the ability for hackers to connect with any username they choose.",
            );
            log(
                "WARN",
                "To change this, set \"online-mode\" to \"true\" in the server.properties file.",
            );
        }
        let mut lists = Lists::load();
        lists.use_whitelist = props.white_list;
        lists.enforce_whitelist = props.enforce_whitelist;
        let lists = Arc::new(Mutex::new(lists));
        // Ferraloom: the lists and the user cache for plugins (Server.getBanList, OfflinePlayer.setOp...).
        super::plugins::register_lists(&lists, &USER_CACHE, props.op_permission_level);
        // Ferraloom: functions compile at `function-permission-level` (Main's WorldLoader.InitConfig).
        super::functions::set_compilation_level(props.function_permission_level);
        // Ferraloom: plugins load (onLoad) before the level, as CraftBukkit's do.
        self.start_plugins(&level_name, &props, port);
        log("INFO", &format!("Preparing level \"{level_name}\""));
        let folder = world_dir.to_string_lossy().into_owned();
        self.load_world(WorldRequest::new(
            &folder,
            Some(&level_name),
            Some(props.seed),
            game_mode == 1,
            None,
        )?)?;
        if new_world {
            self.level.hardcore = props.hardcore;
        }
        // The single-player host's state is not used: players have their own files.
        self.level.position = None;
        // DedicatedServer.initServer: worldData.setGameType; forceDifficulty.
        self.set_default_game_mode(game_mode);
        self.ui.difficulty = props.difficulty as u8;
        // DedicatedServer.initServer: a legacy `announce-player-achievements`
        // sets the show_advancement_messages rule once, as the file is read.
        if let Some(announce) = props.announce_player_achievements {
            self.ui.rules.set(
                "show_advancement_messages",
                horizon_client::gamerules::Value::Bool(announce),
            );
        }
        self.settings.render_distance = props.view_distance.clamp(2, 32);
        self.settings.simulation_distance = props.simulation_distance.clamp(2, 32);
        self.streaming.radius = self.settings.render_distance;
        self.settle_spawn()?;
        let gate_lists = lists.clone();
        let gate: LoginGate = Arc::new(move |address, uuid, name| {
            let user = NameAndId {
                id: uuid,
                name: name.to_owned(),
            };
            gate_lists.lock().unwrap().can_login(address, &user)
        });
        let bypass_lists = lists.clone();
        let favicon = std::fs::read("server-icon.png")
            .ok()
            .filter(|b| png_size(b) == Some((64, 64)));
        let net = NetConfig {
            bind: (!props.server_ip.is_empty())
                .then(|| props.server_ip.parse().ok())
                .flatten(),
            port,
            motd: props.motd.clone(),
            max_players: props.max_players.max(0) as usize,
            compression_threshold: props.network_compression_threshold,
            enable_status: props.enable_status,
            hide_online_players: props.hide_online_players,
            favicon,
            secure_chat: Some(secure_chat),
            accepts_transfers: props.accepts_transfers,
            announce: false,
            listen: true,
            bypasses_limit: Some(Arc::new(move |uuid| {
                bypass_lists
                    .lock()
                    .unwrap()
                    .ops
                    .entries()
                    .iter()
                    .any(|e| e.user.id == uuid && e.bypasses_player_limit)
            })),
            gate: Some(gate),
            online,
            log: Some(NetLog {
                write: Arc::new(log_from),
                lang: Arc::new(horizon_client::net::load_language(&self.options.resources)),
                log_ips: props.log_ips,
            }),
            resource_pack: props.resource_pack.clone(),
            // DedicatedServer.serverLinks: `bug-report-link` as the only link
            // (ServerLinks.KnownLinkType.BUG_REPORT is id 0).
            server_links: if props.bug_report_link.is_empty() {
                Vec::new()
            } else if super::server_links::valid_untrusted_uri(&props.bug_report_link) {
                vec![horizon_protocol::packets::configuration::ServerLink {
                    kind: horizon_protocol::packets::configuration::ServerLinkKind::Known(0),
                    link: props.bug_report_link.clone(),
                }]
            } else {
                log(
                    "WARN",
                    &format!("Failed to parse bug link {}", props.bug_report_link),
                );
                Vec::new()
            },
        };
        let config = HubConfig {
            default_game_mode: game_mode,
            guest_commands: true,
            force_game_mode: props.force_game_mode,
            hardcore: self.level.hardcore,
            view_distance: props.view_distance.clamp(2, 32),
            simulation_distance: props.simulation_distance.clamp(2, 32),
            dimension: self.dim.id().to_owned(),
            seed: self.level.seed,
            max_players: props.max_players.max(0) as usize,
            dedicated: true,
            integrated: false,
        };
        let settings = LanSettings {
            port,
            guest_commands: true,
            force_game_mode: props.force_game_mode,
        };
        if let Err(e) = self.lan_host(settings, net, config, None) {
            log("WARN", "**** FAILED TO BIND TO PORT!");
            log("WARN", &format!("The exception was: {e}"));
            log("WARN", "Perhaps a server is already running on that port?");
            return Ok(());
        }
        self.sync_ops(&lists);
        if let Some(host) = self.lan.as_mut() {
            host.hub.spawn_protection = props.spawn_protection;
        }
        // Ferraloom: plugins' onEnable, before "Done" as CraftBukkit's POSTWORLD.
        self.enable_plugins();
        log(
            "INFO",
            &format!(
                "Done ({:.3}s)! For help, type \"help\"",
                started.elapsed().as_secs_f64()
            ),
        );
        // DedicatedServer.initServer ends with saveEverything(false, true, true).
        self.save_game()?;
        self.log_saved_levels();
        if let Some(host) = self.lan.as_mut() {
            host.hub.entity_range_percent = props.entity_broadcast_range_percentage;
        }
        *ADMIN.lock().unwrap() = Some(Admin {
            lists,
            stop: false,
            autosave: true,
            op_level: props.op_permission_level,
            idle_timeout: props.player_idle_timeout,
            saves: Vec::new(),
        });
        let mut state = ServerState {
            console,
            empty_ticks: 0,
            ticks: Default::default(),
            paused: false,
            broadcast_console: props.broadcast_console_to_ops,
        };
        let pause_after = props.pause_when_empty_seconds * 20;
        horizon_threads::tick_affinity();
        let mut next = Instant::now();
        while !admin(|a| a.stop).unwrap_or(true) {
            let tick_start = Instant::now();
            while let Ok(line) = state.console.try_recv() {
                self.console_line(&mut state, &line);
            }
            self.poll_server_control(&mut state);
            let players = self.lan.as_ref().map_or(0, |h| h.hub.guests.len());
            if pause_after > 0 {
                // A `/pregen` task keeps the server busy as a player would:
                // paused, nothing would post-process its chunks.
                if players == 0 && self.pregen_level().is_none() {
                    state.empty_ticks += 1;
                } else {
                    state.empty_ticks = 0;
                }
            }
            let paused = pause_after > 0 && state.empty_ticks >= pause_after;
            state.paused = paused;
            if paused && state.empty_ticks == pause_after {
                log(
                    "INFO",
                    &format!(
                        "Server empty for {} seconds, pausing",
                        props.pause_when_empty_seconds
                    ),
                );
                self.save_game()?;
            }
            if !paused && !admin(|a| a.autosave).unwrap_or(true) {
                // `save-off`: the periodic save inside the tick is skipped.
                self.ticks = self.ticks.max(1);
            }
            // Ferraloom: the plugins' scheduler heartbeat (tickChildren's, so not while paused).
            if !paused && self.tick_plugins() {
                admin(|a| a.stop = true);
            }
            // MinecraftServer.tickServer: while paused only the connections run.
            self.tick_server_timed(paused)?;
            self.run_deferred_saves();
            let idle_timeout = admin(|a| a.idle_timeout).unwrap_or(0);
            if idle_timeout > 0 {
                self.kick_idle(idle_timeout);
            }
            // MinecraftServer.tickServer records each tick's time, for /tick query (and the status).
            self.clock.record(tick_start.elapsed());
            state.ticked(Instant::now());
            self.wait_for_next_server_tick(&mut next, paused);
        }
        let stopped = self.stop_dedicated();
        *ADMIN.lock().unwrap() = None;
        stopped
    }

    /// Load the spawn area and find the world spawn as single player does,
    /// without keeping a player there.
    pub(super) fn settle_spawn(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(600);
        while self.player.is_none() {
            self.update_world();
            self.spawn_player()?;
            if Instant::now() > deadline {
                return Err("the spawn area did not load".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.player = None;
        Ok(())
    }

    /// The chunks to keep loaded: the players' (lan_update_streaming), or
    /// the spawn chunk while nobody is online.
    pub(super) fn update_dedicated_streaming(&mut self) {
        let center = self
            .lan
            .as_ref()
            .and_then(|h| {
                h.hub
                    .guests
                    .iter()
                    .find(|g| h.hub.here(g))
                    .map(|g| g.chunk())
            })
            .or_else(|| {
                self.spawn
                    .map(|s| ((s.x.floor() as i32) >> 4, (s.z.floor() as i32) >> 4))
            });
        if let Some(c) = center {
            self.streaming.center = c;
        }
    }

    /// ServerGamePacketListenerImpl.tick: players idle longer than the
    /// timeout are kicked.
    fn kick_idle(&mut self, minutes: i32) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let limit = u64::try_from(minutes).unwrap_or(0) * 1200;
        let idle: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.ticks.saturating_sub(g.last_action_tick) > limit)
            .map(|g| g.conn)
            .collect();
        for conn in idle {
            host.hub
                .kick(conn, translatable("multiplayer.disconnect.idling", &[]));
        }
    }

    /// Ops' permission levels for the hub (PlayerList.getProfilePermissions).
    pub(super) fn sync_ops(&mut self, lists: &Arc<Mutex<Lists>>) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        host.hub.ops = lists
            .lock()
            .unwrap()
            .ops
            .entries()
            .iter()
            .map(|e| (e.user.id, e.level as u8))
            .collect();
        host.hub.resend_commands();
    }

    /// A player on the server, by name.
    fn online_player(&self, name: &str) -> Option<NameAndId> {
        let hub = &self.lan.as_ref()?.hub;
        let g = hub
            .guests
            .iter()
            .find(|g| g.name.eq_ignore_ascii_case(name))?;
        Some(NameAndId {
            id: g.uuid,
            name: g.name.clone(),
        })
    }

    /// GameProfileArgument for a name: the user cache, else the profile
    /// service, else (offline) the offline player. None: no such player.
    pub(super) fn resolve(&self, name: &str) -> Option<NameAndId> {
        USER_CACHE.lock().unwrap().as_mut().and_then(|cache| {
            cache.get_by_name(name, |lower| {
                let found = profiles::find_by_name(&profiles::discovery_url(), lower);
                if found.is_none() {
                    log("WARN", &format!("Couldn't find profile with name: {lower}"));
                }
                found
            })
        })
    }

    /// MinecraftServer.sendSystemMessage: what the console is told; while
    /// the console runs a command, the replies vanilla broadcasts go to the
    /// operators too.
    fn console_reply(&mut self, text: Text) {
        log("INFO", &format!("System chat: {}", text.plain()));
        if self.console_broadcast.is_some()
            && text
                .translation_key()
                .is_some_and(|k| BROADCAST.contains(&k))
        {
            self.broadcast_to_admins(text);
        }
    }

    /// One console line, then the operators' share of its feedback.
    fn console_line(&mut self, state: &mut ServerState, line: &str) {
        self.console_broadcast = Some(state.broadcast_console);
        self.console_command(line);
        self.console_broadcast = None;
        if let Some(mut host) = self.lan.take() {
            self.run_level_teleports(&mut host);
            self.send_admin_feedback(&mut host, None);
            self.lan = Some(host);
        }
        self.admin_feedback.clear();
    }

    /// DedicatedServer.handleConsoleInputs: one console line, run as the
    /// server (permission level 4). The console's own `list`, `kick` and
    /// `say` are here; everything else, the dedicated-only commands
    /// included (game/commands/server_cmds.rs), goes to the game's dispatcher.
    fn console_command(&mut self, line: &str) {
        let line = line.strip_prefix('/').unwrap_or(line);
        // Ferraloom: a Bukkit plugin's command (game/plugins.rs).
        if self.plugin_console_command(line) {
            return;
        }
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else {
            return;
        };
        let rest: Vec<&str> = words.collect();
        let lit = |s: &str| Text::literal(s.to_owned());
        match command {
            "list" => {
                let guests: Vec<(String, u128)> = self
                    .lan
                    .as_ref()
                    .map(|h| {
                        h.hub
                            .guests
                            .iter()
                            .map(|g| (g.name.clone(), g.uuid))
                            .collect()
                    })
                    .unwrap_or_default();
                let max = self
                    .lan
                    .as_ref()
                    .map_or(0, |h| h.hub.lan.shared.max_players());
                let names: Vec<String> = if rest.first() == Some(&"uuids") {
                    guests
                        .iter()
                        .map(|(n, u)| {
                            format!("{n} ({})", horizon_protocol::crypto::uuid_string(*u))
                        })
                        .collect()
                } else {
                    guests.iter().map(|(n, _)| n.clone()).collect()
                };
                self.console_reply(Text::translate(
                    "commands.list.players",
                    &[
                        lit(&guests.len().to_string()),
                        lit(&max.to_string()),
                        lit(&names.join(", ")),
                    ],
                ));
            }
            "kick" if !rest.is_empty() => {
                let Some(user) = self.online_player(rest[0]) else {
                    self.console_reply(Text::translate("argument.entity.notfound.player", &[]));
                    return;
                };
                let reason = if rest.len() > 1 {
                    Tag::String(JavaString::from(rest[1..].join(" ").as_str()))
                } else {
                    translatable("multiplayer.disconnect.kicked", &[])
                };
                self.kick_uuid(user.id, reason);
                self.console_reply(Text::translate(
                    "commands.kick.success",
                    &[
                        lit(&user.name),
                        if rest.len() > 1 {
                            lit(&rest[1..].join(" "))
                        } else {
                            Text::translate("multiplayer.disconnect.kicked", &[])
                        },
                    ],
                ));
            }
            "say" if !rest.is_empty() => {
                // SayCommand from the console: an unsigned system chat
                // message of type say_command, logged as not secure.
                let message = rest.join(" ");
                log("INFO", &format!("[Not Secure] [Server] {message}"));
                if let Some(h) = &self.lan {
                    h.hub.say("Server", &message);
                }
            }
            _ => {
                // The game's dispatcher, captured for the console.
                self.chat_capture = Some(Vec::new());
                self.as_console(|g| g.run_command(line));
                for text in self.chat_capture.take().unwrap_or_default() {
                    self.console_reply(text);
                }
            }
        }
    }

    /// DedicatedServerSettings.update: change one key and store the file.
    pub(super) fn store_property(&mut self, key: &str, value: String) {
        let path = PathBuf::from("server.properties");
        if let Ok(p) = ServerProperties::from_file(&path, &mut SystemEntropy::default())
            && let Ok(p) = p.update(key, value)
        {
            let _ = p.store(&path);
        }
    }

    /// MinecraftServer.kickUnlistedPlayers: with `enforce-whitelist` and the
    /// whitelist on, whoever is online and not on the whitelist file is
    /// disconnected (operators are not spared here).
    pub(super) fn kick_unlisted_players(&mut self, lists: &Arc<Mutex<Lists>>) {
        let unlisted: Vec<u128> = {
            let l = lists.lock().unwrap();
            let Some(host) = self.lan.as_ref() else {
                return;
            };
            if !(l.enforce_whitelist && l.use_whitelist) {
                return;
            }
            host.hub
                .guests
                .iter()
                .filter(|g| {
                    !l.white.contains(&NameAndId {
                        id: g.uuid,
                        name: g.name.clone(),
                    })
                })
                .map(|g| g.uuid)
                .collect()
        };
        for uuid in unlisted {
            self.kick_uuid(
                uuid,
                translatable("multiplayer.disconnect.not_whitelisted", &[]),
            );
        }
    }

    pub(super) fn kick_uuid(&mut self, uuid: u128, reason: Tag) {
        if let Some(h) = self.lan.as_mut() {
            let conns: Vec<u32> = h
                .hub
                .guests
                .iter()
                .filter(|g| g.uuid == uuid)
                .map(|g| g.conn)
                .collect();
            for conn in conns {
                h.hub.kick(conn, reason.clone());
            }
        }
    }

    /// MinecraftServer.stopServer: players saved and sent away, worlds saved.
    /// MinecraftServer.saveAllChunks, not silent: one line per level, in
    /// the server's level order.
    fn log_saved_levels(&self) {
        for dimension in [
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        ] {
            log(
                "INFO",
                &format!(
                    "Saving chunks for level 'ServerLevel[{}]'/{dimension}",
                    self.level.name
                ),
            );
        }
    }

    fn stop_dedicated(&mut self) -> Result<()> {
        log("INFO", "Stopping server");
        // Ferraloom: plugins' onDisable, first, as CraftBukkit's stopServer.
        self.disable_plugins();
        log("INFO", "Saving players");
        if let Some(h) = self.lan.as_mut() {
            for g in &h.hub.guests {
                horizon_client::lan::playerdata::save(g, &self.world_root);
            }
        }
        if let Some(h) = self.lan.as_ref() {
            for g in &h.hub.guests {
                self.save_guest_records(h, g);
            }
        }
        if let Some(h) = self.lan.as_mut() {
            let conns: Vec<u32> = h.hub.guests.iter().map(|g| g.conn).collect();
            for conn in conns {
                h.hub.kick(
                    conn,
                    translatable("multiplayer.disconnect.server_shutdown", &[]),
                );
            }
        }
        std::thread::sleep(Duration::from_millis(100));
        log("INFO", "Saving worlds");
        self.save_game()?;
        self.log_saved_levels();
        self.lan_close(false);
        Ok(())
    }
}

/// The width and height of a PNG (its IHDR chunk).
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::{capture_log, log, log_from};
    use std::{
        fs,
        io::{self, BufRead, BufReader, Read, Write},
        process::{Command, Stdio},
        time::{SystemTime, UNIX_EPOCH},
    };

    const LOGIN: &str = "Player[/127.0.0.1:0] logged in with entity id 2 at (0.5, 115.0, 0.5)";
    const WARNING: &str = "Authentication warning";
    const READY: &str = "horizon-console-log-ready\n";
    const CHILD_HOME: &str = "HORIZON_TEST_CONSOLE_LOG_HOME";

    fn assert_log_lines(text: &str) {
        let expected = [
            format!("] [Server thread/INFO]: {LOGIN}"),
            format!("] [User Authenticator #1/WARN]: {WARNING}"),
        ];
        assert_eq!(text.lines().count(), expected.len());
        for (line, suffix) in text.lines().zip(expected) {
            assert!(line.starts_with('['));
            assert!(chrono::NaiveTime::parse_from_str(&line[1..9], "%H:%M:%S").is_ok());
            assert_eq!(&line[9..], suffix);
        }
    }

    #[test]
    fn console_logging_survives_closed_stdout() {
        if let Some(home) = std::env::var_os(CHILD_HOME) {
            horizon_client::logging::init(std::path::Path::new(&home), "Server thread");
            // Let the parent close the real stdout pipe only after libtest's startup output.
            io::stdout().write_all(READY.as_bytes()).unwrap();
            io::stdout().flush().unwrap();
            io::stdin().read_exact(&mut [0]).unwrap();
            std::thread::Builder::new()
                .name("Server thread".into())
                .spawn(|| {
                    let captured = capture_log(|| {
                        log("INFO", LOGIN);
                        log_from("User Authenticator #1", "WARN", WARNING);
                    });
                    assert_eq!(captured, [LOGIN, WARNING]);
                    horizon_client::logging::flush();
                })
                .unwrap()
                .join()
                .unwrap();
            // Libtest's final status also writes to stdout; only the game logger is under test.
            std::process::exit(0);
        }

        for closed_stdout in [false, true] {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let home = std::env::temp_dir().join(format!(
                "horizon-console-log-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&home).unwrap();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "game::dedicated::tests::console_logging_survives_closed_stdout",
                    "--nocapture",
                ])
                .env(CHILD_HOME, &home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            loop {
                let mut line = String::new();
                assert!(
                    stdout.read_line(&mut line).unwrap() > 0,
                    "child was not ready"
                );
                if line.ends_with(READY) {
                    break;
                }
            }
            if closed_stdout {
                drop(stdout); // No reader remains: the next console write gets EPIPE.
            } else {
                child.stdout = Some(stdout.into_inner());
            }
            child.stdin.take().unwrap().write_all(&[1]).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "closed_stdout={closed_stdout}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if !closed_stdout {
                assert_log_lines(std::str::from_utf8(&output.stdout).unwrap());
            }
            assert_log_lines(&fs::read_to_string(home.join("logs/latest.log")).unwrap());
            fs::remove_dir_all(home).unwrap();
        }
    }
}
