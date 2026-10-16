use crate::block::registry::BlockRegistry;
use crate::command::commands::default_dispatcher;
use crate::command::commands::defaultgamemode::DefaultGamemode;
use crate::data::VanillaData;
use crate::data::player_server::ServerPlayerData;
use crate::entity::NBTStorage;
use crate::entity::player::statistics::StatisticsDataError;
use crate::item::registry::ItemRegistry;
use crate::net::authentication::fetch_mojang_public_keys;
use crate::net::{ClientPlatform, DisconnectReason, EncryptionError, GameProfile, PlayerConfig};
use crate::plugin::PluginManager;
use crate::plugin::loader::PluginLoader;
use crate::plugin::player::player_login::PlayerLoginEvent;
use crate::plugin::server::server_broadcast::ServerBroadcastEvent;
use crate::server::tick_rate_manager::ServerTickRateManager;
use crate::world::WorldPortal;
use crate::world::custom_bossbar::CustomBossbars;
use crate::world::scoreboard::Scoreboard;
use crate::{
    command::node::dispatcher::CommandDispatcher, entity::player::Player, world::World,
    world::map::MapManager,
};
use arc_swap::ArcSwap;
use connection_cache::{CachedBranding, CachedStatus};
use key_store::KeyStore;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::dimension::Dimension;
use pumpkin_util::permission::PermissionManager;
use pumpkin_util::text::color::NamedColor;
use pumpkin_world::dimension::into_level;
use pumpkin_world::generation::generator::GeneratorInit;
use pumpkin_world::world::WorldPortalExt;
use tracing::{debug, error, info, warn};

use pumpkin_protocol::java::client::login::CEncryptionRequest;
use pumpkin_protocol::java::client::play::{CChangeDifficulty, CTabList};
use pumpkin_protocol::{ClientPacket, java::client::config::CPluginMessage};
use pumpkin_util::Difficulty;
use pumpkin_util::text::TextComponent;
use pumpkin_world::world_info::anvil::{
    AnvilLevelInfo, LEVEL_DAT_BACKUP_FILE_NAME, LEVEL_DAT_FILE_NAME,
};
use pumpkin_world::world_info::{LevelData, WorldInfoError, WorldInfoReader, WorldInfoWriter};
use rand::seq::IndexedRandom;
use rayon::prelude::*;
use rsa::RsaPublicKey;
use std::fs;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32};
use std::{future::Future, sync::atomic::Ordering, time::Duration};
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;
use tokio_util::task::TaskTracker;

#[cfg(test)]
mod scoreboard_tests;

mod command_storage;
mod connection_cache;
pub(crate) mod debug_profiler;
pub mod enchantment;
mod key_store;
pub mod recipe;
mod saving;
pub mod scheduler;
pub mod seasonal_events;
pub mod server_test_manager;
pub mod tick_rate_manager;
pub mod ticker;

pub use recipe::RecipeManager;

use crate::data::advancement_data::AdvancementManager;
use crate::data::statistics_data::StatisticsManager;

/// Represents a Minecraft server instance.
pub struct Server {
    pub basic_config: BasicConfiguration,
    pub advanced_config: AdvancedConfiguration,
    pub telemetry_config: TelemetryConfig,

    pub data: VanillaData,

    /// Plugin manager
    pub plugin_manager: Arc<PluginManager>,

    /// Permission manager for the server.
    pub permission_manager: Arc<PermissionManager>,

    /// Handles cryptographic keys for secure communication.
    key_store: OnceCell<Arc<KeyStore>>,
    /// Bedrock OIDC provider keys, fetched on startup for 1.26.10+ token validation.
    pub bedrock_oidc_keys: Arc<OnceCell<(String, pumpkin_auth::jwt::Jwks)>>,
    /// Cached Bedrock server private key (process-lifetime). Generated on first Bedrock login and reused.
    /// Manages server status information.
    listing: std::sync::Mutex<CachedStatus>,
    /// Saves server branding information.
    branding: CachedBranding,
    /// Saves and dispatches commands to appropriate handlers.
    pub command_dispatcher: ArcSwap<CommandDispatcher>,
    /// Block behaviour.
    pub block_registry: Arc<BlockRegistry>,
    /// Item behaviour.
    pub item_registry: Arc<ItemRegistry>,
    /// Manages multiple worlds within the server.
    pub worlds: ArcSwap<Vec<Arc<World>>>,
    /// One scoreboard for all dimensions; persisted only by the server lifecycle.
    pub scoreboard: Arc<std::sync::Mutex<Scoreboard>>,
    /// All the dimensions that exist on the server.
    pub dimensions: Vec<Dimension>,
    /// Assigns unique IDs to containers.
    container_id: AtomicU32,
    pub recipe_manager: Arc<recipe::RecipeManager>,
    pub datapack_manager: Arc<crate::data::datapack::DatapackManager>,
    pub enchantment_manager: Arc<enchantment::EnchantmentManager>,
    /// Assigns unique IDs to maps.
    map_id: AtomicI32,
    /// Mojang's public keys, used for chat session signing
    /// Pulled from Mojang API on startup
    pub mojang_public_keys: ArcSwap<Vec<RsaPublicKey>>,
    /// The server's custom bossbars
    pub bossbars: std::sync::Mutex<CustomBossbars>,
    /// Manages all maps on the server
    pub map_manager: MapManager,
    /// The default gamemode when a player joins the server (reset every restart)
    pub defaultgamemode: std::sync::Mutex<DefaultGamemode>,
    /// Manages player data storage
    pub player_data_storage: ServerPlayerData,
    /// Command storage for `/data` command
    pub command_storage:
        std::sync::Mutex<std::collections::HashMap<String, pumpkin_nbt::compound::NbtCompound>>,
    command_storage_files: std::sync::Mutex<command_storage::NamespaceFiles>,
    /// Global server stopwatches
    pub stopwatches: std::sync::Mutex<crate::world::stopwatches::Stopwatches>,
    /// Global server random sequences
    pub random_sequences: std::sync::Mutex<crate::world::random_sequences::RandomSequences>,
    // Manages player advancement
    pub advancement_manager: Arc<AdvancementManager>,
    pub statistics_manager: Arc<StatisticsManager>,
    // Whether the server whitelist is on or off
    pub white_list: AtomicBool,
    /// Manages the server's tick rate, freezing, and sprinting
    pub tick_rate_manager: Arc<ServerTickRateManager>,
    /// Stores the duration of the last 100 ticks for performance analysis
    pub tick_times_nanos: std::sync::Mutex<[i64; 100]>,
    /// Aggregated tick times for efficient rolling average calculation
    pub aggregated_tick_times_nanos: AtomicI64,
    /// Total number of ticks processed by the server
    pub tick_count: AtomicI32,
    /// Owns the server-wide tick profiling session used by `/debug`.
    pub(crate) debug_profiler: debug_profiler::DebugProfiler,
    /// Random unique Server ID used by Bedrock Edition
    pub server_guid: u64,
    /// Player idle timeout in minutes (0 = disabled)
    pub player_idle_timeout: AtomicI32,
    /// Manages scheduled tasks (e.g. from plugins)
    /// Manages scheduled datapack functions (`/schedule`)
    pub scheduled_functions: Arc<crate::server::scheduler::ScheduledFunctionQueue>,
    tasks: TaskTracker,
    pub(crate) packet_runner: Arc<crate::entity::player::packet_activity::PacketRunner>,
    saving: saving::SaveCoordinator,
    pub runtime: tokio::runtime::Handle,
    pub management_hub: Arc<crate::net::management::hub::ManagementHub>,

    // world stuff which maybe should be put into a struct
    pub level_info: Arc<ArcSwap<LevelData>>,
    world_info_writer: Arc<dyn WorldInfoWriter>,
}

impl Server {
    #[expect(clippy::too_many_lines)]
    pub async fn new(
        basic_config: BasicConfiguration,
        advanced_config: AdvancedConfiguration,
        telemetry_config: TelemetryConfig,
        vanilla_data: VanillaData,
        plugin_loaders: Vec<Arc<dyn PluginLoader>>,
    ) -> Result<Arc<Self>, WorldInfoError> {
        let permission_manager = Arc::new(PermissionManager::new());
        // First register the default commands. After that, plugins can put in their own.
        let command_dispatcher = ArcSwap::from_pointee(default_dispatcher(
            &permission_manager,
            &advanced_config.commands,
        ));

        crate::command::set_broadcast_console_to_ops(
            advanced_config.commands.broadcast_console_to_ops,
        );

        let world_path = basic_config.get_world_path();
        // Load before creating worlds or writing new-world metadata. Broken existing
        // server data is a startup error, never a reason to save an empty replacement.
        let scoreboard = Arc::new(std::sync::Mutex::new(Scoreboard::load(&world_path)?));
        let (command_storage, command_storage_files) =
            command_storage::NamespaceFiles::load(&world_path)?;

        let block_registry = super::block::registry::default_registry();

        let level_info = match AnvilLevelInfo.read_world_info(&world_path) {
            Ok(level_info) => {
                let dat_path = world_path.join(LEVEL_DAT_FILE_NAME);
                if dat_path.exists() {
                    let backup_path = world_path.join(LEVEL_DAT_BACKUP_FILE_NAME);
                    if let Err(err) = fs::copy(&dat_path, &backup_path) {
                        warn!("Failed to create backup {LEVEL_DAT_BACKUP_FILE_NAME}: {err}");
                    }
                }
                level_info
            }
            Err(WorldInfoError::InfoNotFound) => {
                warn!(
                    "No {LEVEL_DAT_FILE_NAME} in {}, creating a new world with seed {}",
                    world_path.display(),
                    basic_config.seed.0 as i64
                );
                let overworld_gen = pumpkin_world::generation::generator::VanillaGenerator::new(
                    basic_config.seed,
                    Dimension::OVERWORLD,
                );
                let default_data =
                    LevelData::from_world_generator(basic_config.seed, &overworld_gen);
                AnvilLevelInfo.write_world_info(&default_data, &world_path)?;
                default_data
            }
            Err(
                error @ (WorldInfoError::UnsupportedDataVersion(_)
                | WorldInfoError::UnsupportedLevelVersion(_)),
            ) => {
                error!("Failed to load world info!");
                error!("{error}");
                error!("Unsupported world version! See the logs for more info.");
                std::process::exit(1);
            }
            Err(error) => {
                error!("Failed to load the world data in {}!", world_path.display());
                error!("{error}");
                error!(
                    "Refusing to continue: a default world would generate different terrain on top of the existing region files. Restore {LEVEL_DAT_FILE_NAME} from {LEVEL_DAT_BACKUP_FILE_NAME}, which also holds a copy of the world seed, or move the world folder aside to start a new world."
                );
                error!("Failed to load the world data! See the logs for more info.");
                std::process::exit(1);
            }
        };

        let seed = level_info.world_gen_settings.seed;
        let level_info = Arc::new(ArcSwap::new(Arc::new(level_info)));

        let listing = std::sync::Mutex::new(CachedStatus::new(
            &basic_config,
            &advanced_config.networking.java.motd,
            advanced_config.networking.java.max_players,
        ));
        let defaultgamemode = std::sync::Mutex::new(DefaultGamemode {
            gamemode: basic_config.default_gamemode,
        });
        let players_dir = world_path.join("players");
        let player_data_storage = ServerPlayerData::new(
            players_dir.join("data"),
            Duration::from_secs(advanced_config.player_data.save_player_cron_interval),
            advanced_config.player_data.save_player_data,
        );
        let advancement_manager = Arc::new(AdvancementManager::new(
            players_dir.clone(),
            advanced_config.advancement.save_advancements,
        ));
        let statistics_manager = Arc::new(StatisticsManager::new(
            players_dir,
            advanced_config.player_data.save_player_data,
        ));
        let white_list = AtomicBool::new(basic_config.white_list);

        let tick_rate_manager = Arc::new(ServerTickRateManager::new(basic_config.tps));

        let dimensions = {
            let mut dimensions = vec![Dimension::OVERWORLD];
            if basic_config.allow_nether {
                dimensions.push(Dimension::THE_NETHER);
            }
            if basic_config.allow_end {
                dimensions.push(Dimension::THE_END);
            }
            dimensions
        };
        info!(
            "Enabled dimensions: {:?}",
            dimensions
                .iter()
                .map(|d| d.minecraft_name)
                .collect::<Vec<_>>()
        );

        let management_settings = Arc::new(crate::net::management::hub::ManagementSettings::new(
            &basic_config,
            &advanced_config,
            &advanced_config.networking.management,
        ));
        let management_hub = Arc::new(crate::net::management::hub::ManagementHub::new(
            management_settings,
        ));

        let server = Self {
            basic_config,
            advanced_config,
            telemetry_config,
            data: vanilla_data,
            plugin_manager: Arc::new(PluginManager::new(plugin_loaders)),
            permission_manager,
            container_id: 0.into(),
            recipe_manager: Arc::new(recipe::RecipeManager::new()),
            datapack_manager: Arc::new(crate::data::datapack::DatapackManager::new()),
            enchantment_manager: Arc::new(enchantment::EnchantmentManager::new()),
            map_id: level_info.load().map_id.into(),
            worlds: ArcSwap::from_pointee(vec![]),
            dimensions,
            command_dispatcher,
            block_registry: block_registry.clone(),
            item_registry: super::item::items::default_registry(),
            key_store: OnceCell::new(),
            bedrock_oidc_keys: Arc::new(OnceCell::new()),
            listing,
            branding: CachedBranding::new(),
            scoreboard,
            bossbars: std::sync::Mutex::new(CustomBossbars::new()),
            map_manager: MapManager::new(),
            defaultgamemode,
            player_data_storage,
            command_storage: std::sync::Mutex::new(command_storage),
            command_storage_files: std::sync::Mutex::new(command_storage_files),
            stopwatches: std::sync::Mutex::new(crate::world::stopwatches::Stopwatches::new()),
            random_sequences: std::sync::Mutex::new(
                crate::world::random_sequences::RandomSequences::new(),
            ),
            advancement_manager,
            statistics_manager,
            white_list,
            tick_rate_manager,
            tick_times_nanos: std::sync::Mutex::new([0; 100]),
            aggregated_tick_times_nanos: AtomicI64::new(0),
            tick_count: AtomicI32::new(0),
            debug_profiler: debug_profiler::DebugProfiler::new(),
            tasks: TaskTracker::new(),
            packet_runner: Arc::default(),
            saving: saving::SaveCoordinator::default(),
            runtime: tokio::runtime::Handle::current(),
            scheduled_functions: Arc::new(crate::server::scheduler::ScheduledFunctionQueue::new()),
            server_guid: rand::random(),
            player_idle_timeout: AtomicI32::new(0),
            mojang_public_keys: ArcSwap::from_pointee(Vec::new()),
            world_info_writer: Arc::new(AnvilLevelInfo),
            level_info,
            management_hub,
        };
        let server = Arc::new(server);

        // Fetch / generate keys in background tasks to avoid blocking startup
        let server_clone = server.clone();
        server.spawn_task(async move {
            let key_store = Arc::new(KeyStore::new());
            let _ = server_clone.key_store.set(key_store);
        });

        if server.basic_config.allow_chat_reports {
            let server_clone = server.clone();
            server.spawn_task(async move {
                let auth_config = server_clone
                    .advanced_config
                    .networking
                    .java
                    .authentication
                    .clone();
                let keys = fetch_mojang_public_keys(&auth_config)
                    .await
                    .unwrap_or_else(|e| {
                        error!("Failed to fetch Mojang keys: {e}");
                        Vec::new()
                    });
                server_clone.mojang_public_keys.store(Arc::new(keys));
            });
        }

        if server.advanced_config.networking.bedrock.online_mode
            && server
                .advanced_config
                .networking
                .bedrock
                .authentication
                .enabled
        {
            let server_clone = server.clone();
            server.spawn_task(async move {
                let auth = server_clone
                    .advanced_config
                    .networking
                    .bedrock
                    .authentication
                    .clone();
                let keys = match pumpkin_auth::jwt::fetch_oidc_jwks(
                    auth.url.as_deref(),
                    auth.connect_timeout,
                    auth.read_timeout,
                )
                .await
                {
                    Ok(keys) => keys,
                    Err(error) => {
                        error!("Failed to fetch Bedrock OIDC keys: {error}");
                        (String::new(), pumpkin_auth::jwt::Jwks { keys: Vec::new() })
                    }
                };
                let _ = server_clone.bedrock_oidc_keys.set(keys);
            });
        }

        let mut worlds_vec = Vec::new();
        for dim in &server.dimensions {
            info!(
                "Loading {}",
                TextComponent::text(dim.minecraft_name.to_string())
                    .color_named(NamedColor::DarkGreen)
                    .to_pretty_console()
            );
            let config = Arc::new(server.advanced_config.world.clone());
            let level = into_level(dim.clone(), &config, world_path.clone(), seed);
            let world = Arc::new(World::load(
                level.clone(),
                server.level_info.clone(),
                dim.clone(),
                block_registry.clone(),
                Arc::downgrade(&server),
                server.scoreboard.clone(),
            ));
            let portal: Arc<dyn WorldPortalExt> = Arc::new(WorldPortal(world.clone()));
            level.world_portal.store(Arc::new(Some(portal)));
            worlds_vec.push(world);
        }

        for world in &worlds_vec {
            let mut world_init_event =
                crate::plugin::api::events::world::world_init::WorldInitEvent::new(world.clone());
            server
                .plugin_manager
                .fire(&server, &mut world_init_event)
                .await;

            let mut world_load_event =
                crate::plugin::api::events::world::world_load::WorldLoadEvent::new(world.clone());
            server
                .plugin_manager
                .fire(&server, &mut world_load_event)
                .await;
        }

        server.worlds.store(Arc::new(worlds_vec));

        info!("All worlds loaded successfully.");

        crate::data::datapack::DatapackManager::discover_new_packs(&server);
        let enabled_packs = server.level_info.load().data_packs.enabled.clone();
        server
            .datapack_manager
            .load_all(&world_path, &enabled_packs, &server.recipe_manager);

        let source = crate::command::CommandSender::Console.into_source(&server);
        let _ = server
            .datapack_manager
            .execute_function(&server, &source, "#minecraft:load");

        Ok(server)
    }

    /// Spawns a task associated with this server. All tasks spawned with this method are awaited
    /// when the server stops. This means tasks should complete in a reasonable (no looping) amount of time.
    pub fn spawn_task<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        if let Some(activity) = crate::entity::player::packet_activity::ActivityLease::current() {
            activity.submit(task).spawn_on(&self.tasks, &self.runtime)
        } else {
            self.tasks.spawn_on(task, &self.runtime)
        }
    }

    /// Reconciliation withdraws readiness and waits for callback retirement
    /// itself. It must not wait behind that same readiness or count itself as
    /// a callback. Packet descendants still poll inside the execution phase.
    pub(crate) fn spawn_chunk_reconcile<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        if let Some(activity) = crate::entity::player::packet_activity::ActivityLease::current() {
            activity
                .submit_world_stages(task)
                .spawn_on(&self.tasks, &self.runtime)
        } else {
            self.tasks.spawn_on(task, &self.runtime)
        }
    }

    /// Close the task tracker and drain accepted work before saving worlds.
    pub(crate) async fn finish_tasks(&self) {
        self.tasks.close();
        self.tasks.wait().await;
    }

    pub fn get_world_from_dimension(&self, dimension: &Dimension) -> Arc<World> {
        let worlds = self.worlds.load();
        worlds
            .iter()
            .find(|w| w.dimension.minecraft_name == dimension.minecraft_name)
            .cloned()
            .or_else(|| worlds.first().cloned())
            .unwrap_or_else(|| {
                error!("No default world exists");
                std::process::exit(1);
            })
    }

    pub fn create_world(self: &Arc<Self>, name: String, dimension: Dimension) -> Arc<World> {
        {
            let worlds = self.worlds.load();
            let world = worlds
                .iter()
                .find(|w| w.get_world_name() == name && w.dimension == dimension)
                .cloned();
            if let Some(world) = world {
                return world;
            }
        }

        let world_path = self.basic_config.get_world_path().join(name);
        let registry = self.block_registry.clone();
        let l_info = self.level_info.clone();
        let weak = Arc::downgrade(self);
        let config = Arc::new(self.advanced_config.world.clone());
        let seed = self.level_info.load().world_gen_settings.seed;

        let level =
            pumpkin_world::dimension::into_level(dimension.clone(), &config, world_path, seed);
        let world: World = World::load(
            level.clone(),
            l_info,
            dimension,
            registry,
            weak,
            self.scoreboard.clone(),
        );
        let world = Arc::new(world);
        let portal: Arc<dyn WorldPortalExt> = Arc::new(WorldPortal(world.clone()));
        level.world_portal.store(Arc::new(Some(portal)));
        self.worlds.rcu(|worlds| {
            let mut new_worlds = (**worlds).clone();
            new_worlds.push(world.clone());
            new_worlds
        });
        let mut event =
            crate::plugin::api::events::world::world_init::WorldInitEvent::new(world.clone());
        self.plugin_manager.fire_blocking(self, &mut event);
        world
    }

    pub async fn unload_world(&self, name: &str) -> Result<(), String> {
        let worlds = self.worlds.load();
        let world_to_unload = worlds
            .iter()
            .find(|w| w.get_world_name() == name || w.dimension.minecraft_name == name)
            .cloned()
            .ok_or_else(|| format!("World '{name}' not found"))?;

        if let Some(first_world) = worlds.first()
            && Arc::ptr_eq(first_world, &world_to_unload)
        {
            return Err("Cannot unload the primary/default world".to_string());
        }

        let player_count = world_to_unload.players.load().len();
        if player_count > 0 {
            return Err(format!(
                "Cannot unload world '{name}': {player_count} players are still in this world"
            ));
        }

        world_to_unload.withdraw_packet_admission().await;
        // A transfer admitted before withdrawal may have published its player.
        if !world_to_unload.players.load().is_empty() {
            world_to_unload.restore_packet_admission();
            return Err(format!("Cannot unload world '{name}': a player arrived"));
        }
        let save_result = world_to_unload.shutdown().await;
        world_to_unload.unload().await;

        self.worlds.rcu(|w_list| {
            let mut new_list = (**w_list).clone();
            new_list.retain(|w| !Arc::ptr_eq(w, &world_to_unload));
            new_list
        });

        save_result.map_err(|error| format!("Failed to save unloaded world {name}: {error}"))
    }

    pub fn save_world_info(&self) -> Result<(), WorldInfoError> {
        // Commands and ticking update the world's live clock independently of
        // level_info. Overlay it only on this save, preserving concurrent rules
        // or other metadata updates rather than storing a stale clone back.
        let mut level_data = (**self.level_info.load()).clone();
        if let Some(overworld) = self
            .worlds
            .load()
            .iter()
            .find(|world| world.dimension.minecraft_name == Dimension::OVERWORLD.minecraft_name)
        {
            level_data.day_time = overworld.get_time_of_day();
        }
        self.world_info_writer
            .write_world_info(&level_data, &self.basic_config.get_world_path())
    }

    pub fn reload_datapacks(&self, server: &Arc<Self>) {
        crate::data::datapack::DatapackManager::discover_new_packs(self);
        let enabled_packs = self.level_info.load().data_packs.enabled.clone();
        let world_path = self.basic_config.get_world_path();
        self.datapack_manager
            .load_all(&world_path, &enabled_packs, &self.recipe_manager);

        let source = crate::command::CommandSender::Console.into_source(server);
        let _ = self
            .datapack_manager
            .execute_function(server, &source, "#minecraft:load");

        let dynamic_recipes = self.recipe_manager.get_dynamic_recipes_internal();
        for player in self.get_all_players() {
            if let crate::net::ClientPlatform::Java(java_client) = player.client.as_ref() {
                let add_packet = pumpkin_protocol::java::client::play::CRecipeBookAdd::new(
                    true,
                    &dynamic_recipes,
                );
                if let Ok(data) = java_client.serialize_packet(&add_packet) {
                    java_client.try_enqueue_packet(data);
                }
            }
        }
    }

    #[must_use]
    pub fn get_known_packs<'a>(
        &self,
        server_version: &'a str,
        loaded_packs: &'a [crate::data::datapack::LoadedDatapack],
    ) -> Vec<pumpkin_protocol::KnownPack<'a>> {
        self.datapack_manager
            .get_known_packs(self, server_version, loaded_packs)
    }

    #[must_use]
    pub fn get_enabled_features(&self) -> Vec<&'static str> {
        self.datapack_manager.get_enabled_features(self)
    }

    #[must_use]
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        self.datapack_manager.is_feature_enabled(self, feature)
    }

    /// The save coordinator orders writers. Hold the live scoreboard lock only
    /// while taking its snapshot, leaving gameplay free during compression and IO.
    fn save_scoreboard(&self) -> std::io::Result<()> {
        let snapshot = self
            .scoreboard
            .lock()
            .map_err(|_| std::io::Error::other("Scoreboard lock poisoned"))?
            .to_saved_data()?;
        Scoreboard::write_saved_data(&self.basic_config.get_world_path(), snapshot)
    }

    /// Adds a new player to the server.
    ///
    /// This function takes an `Arc<Client>` representing the connected client and performs the following actions:
    ///
    /// 1. Generates a new entity ID for the player.
    /// 2. Determines the player's gamemode (defaulting to Survival if not specified in configuration).
    /// 3. **(TODO: Select default from config)** Selects the world for the player (currently uses the first world).
    /// 4. Creates a new `Player` instance using the provided information.
    /// 5. Adds the player to the chosen world.
    /// 6. **(TODO: Config if we want increase online)** Optionally updates server listing information based on the player's configuration.
    ///
    /// # Arguments
    ///
    /// * `client`: An `Arc<Client>` representing the connected client.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    ///
    /// - `Arc<Player>`: A reference to the newly created player object.
    /// - `Arc<World>`: A reference to the world the player was added to.
    ///
    /// # Note
    ///
    /// You still have to spawn the `Player` in a `World` to let them join and make them visible.
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the admission lease, three persistence loads and publication in one fail-closed sequence."
    )]
    pub async fn add_player(
        self: &Arc<Self>,
        client: Arc<ClientPlatform>,
        profile: GameProfile,
        config: Option<PlayerConfig>,
    ) -> Option<(Arc<Player>, Arc<World>)> {
        let gamemode = self
            .defaultgamemode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .gamemode;

        let first_world = self.worlds.load().first().cloned()?;

        // One lease covers loading, publication and final saves for this UUID.
        // Wait for a replaced session without blocking any other player's IO.
        let admission_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let session = tokio::select! {
            biased;
            () = client.await_close_interrupt() => return None,
            result = tokio::time::timeout_at(
                admission_deadline,
                self.player_data_storage.reserve_player_session(profile.id),
            ) => if let Ok(session) = result {
                session
            } else {
                client.try_kick(DisconnectReason::LoggedInOtherLocation, &TextComponent::text("The previous session has not disconnected yet. Please try again."));
                return None;
            },
        };
        if client.closed() {
            return None;
        }
        // Atomic advancement/statistics writes can outlive a cancelled
        // disconnect future on their blocking workers.
        tokio::select! {
            biased;
            () = client.await_close_interrupt() => return None,
            result = tokio::time::timeout_at(admission_deadline, async {
                self.advancement_manager.wait_for_pending_saves().await;
                self.statistics_manager.wait_for_pending_saves().await;
            }) => {
                if result.is_err() {
                    client.try_kick(DisconnectReason::LoggedInOtherLocation, &TextComponent::text("The previous session has not finished saving. Please try again."));
                    return None;
                }
            },
        }

        // Reject before constructing or publishing a player, so disconnect and
        // autosave paths cannot replace unreadable data with a default player.
        let data = match self.player_data_storage.load_data(&profile.id) {
            Ok(data) => data,
            Err(error) => {
                error!(
                    "Refusing login for {}: failed to load player data: {error}",
                    profile.id
                );
                client.try_kick(
                    DisconnectReason::Kicked,
                    &TextComponent::text("Your player data could not be loaded. Please contact the server administrator."),
                );
                return None;
            }
        };
        let (world, nbt) = if let Some(data) = data {
            if let Some(dimension_key) = data.get_string("Dimension") {
                if let Some(dimension) = Dimension::from_name(dimension_key) {
                    let world = self.get_world_from_dimension(dimension);
                    (world, Some(data))
                } else {
                    warn!("Invalid dimension key in player data: {dimension_key}");
                    (first_world, Some(data))
                }
            } else {
                // Player data exists but doesn't have a "Dimension" key.
                (first_world, Some(data))
            }
        } else {
            // No player data found, default to the Overworld.
            (first_world, None)
        };

        let mut player = Player::new(
            client,
            profile,
            config.clone().unwrap_or_default(),
            &world,
            gamemode,
        );

        if let Some(mut nbt_data) = nbt {
            player.read_nbt(&mut nbt_data);
            // The data file itself proves this is a returning player. Older Bedrock
            // sessions could persist HasPlayedBefore as false and mask a valid Pos.
            player.has_played_before.store(true, Ordering::Relaxed);
        }

        // Advancement load failures must reject before session publication,
        // login callbacks, world insertion or final-save eligibility.
        {
            let mut advancements = player
                .advancements
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Err(error) = advancements.load() {
                error!(
                    "Refusing login for {}: failed to load advancements: {error}",
                    player.gameprofile.id
                );
                player.client.try_kick(
                    DisconnectReason::Kicked,
                    &TextComponent::text("Your advancement data could not be loaded. Please contact the server administrator."),
                );
                return None;
            }
        }

        if let Err(error) = self.statistics_manager.load_player(&player) {
            error!(
                "Refusing login for {}: statistics admission failed (canonical file {}): {error}. Player NBT is players/data/{}.dat relative to the world; see docs/compatibility.md for the legacy format policy",
                player.gameprofile.id,
                self.statistics_manager
                    .player_path(player.gameprofile.id)
                    .display(),
                player.gameprofile.id,
            );
            let message = if matches!(error, StatisticsDataError::LegacyPrivateStatistics) {
                "Your saved statistics use an older Ferraloom format. Please ask the server administrator about migration."
            } else {
                "Your statistics data could not be loaded. Please contact the server administrator."
            };
            player
                .client
                .try_kick(DisconnectReason::Kicked, &TextComponent::text(message));
            return None;
        }

        // A refused spawn must release the local UUID lease without publishing a
        // Player or making default spawn data eligible for any of the final writers.
        let spawn = tokio::select! {
            biased;
            () = player.client.await_close_interrupt() => return None,
            result = world.prepare_initial_spawn(&player) => result,
        };
        let (position, yaw, pitch) = match spawn {
            Ok(spawn) => spawn,
            Err(error) => {
                player.kick_for_chunk_failure(&error);
                return None;
            }
        };
        player.living_entity.entity.set_pos(position);
        player.living_entity.entity.set_rotation(yaw, pitch);
        player.living_entity.entity.last_pos.store(position);

        // Wrap in Arc after all three persistence families are loaded.
        let player = Arc::new(player);
        {
            let mut advancements = player
                .advancements
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            advancements.player = Arc::downgrade(&player);
        };
        session.publish(&player);

        send_cancellable_blocking! {{
            self;
            &mut PlayerLoginEvent::new(player.clone(), TextComponent::text("You have been kicked from the server"));
            'after: {
                if player.client.closed() {
                    return None;
                }
                *player.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session);
                player.screen_handler_sync_handler.store_player(player.clone());
                world.add_player(&player).is_ok().then(|| {
                    {
                        let mut user_cache = self
                            .data
                            .user_cache
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        user_cache.upsert(player.gameprofile.id, player.gameprofile.name.clone());
                    };

                    // TODO: Config if we want increase online
                    if let Some(config) = config {
                        // TODO: Config so we can also just ignore this hehe
                        if config.server_listing {
                            self.listing
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .add_player(&player);
                        }
                    }

                    let player_dto = crate::net::management::dto::PlayerDto::new(
                        Some(player.gameprofile.id),
                        Some(player.gameprofile.name.clone()),
                    );
                    self.management_hub.broadcast_player_joined(&player_dto);

                    (player, world)
                })
            }

            'cancelled: {
                player.kick(DisconnectReason::Kicked, &event.kick_message);
                None
            }
        }}
    }

    pub fn remove_player(&self, player: &Player) {
        let player_dto = crate::net::management::dto::PlayerDto::new(
            Some(player.gameprofile.id),
            Some(player.gameprofile.name.clone()),
        );
        self.management_hub.broadcast_player_left(&player_dto);

        player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::LeaveGame as i32,
            1,
        );
        // TODO: Config if we want decrease online
        self.listing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove_player(player);
    }

    pub async fn shutdown(&self) {
        self.management_hub.broadcast_server_stopping();

        debug!("Awaiting tasks for server");
        self.finish_tasks().await;
        debug!("Done awaiting tasks for server");

        let _save_guard = self.saving.lock().await;
        self.report_autosave_failure();

        info!("Starting worlds");
        for world in self.worlds.load().iter() {
            if let Err(error) = world.shutdown().await {
                error!(
                    "Failed to save {} during shutdown: {error}",
                    world.dimension.minecraft_name
                );
                crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
            }
        }
        if let Err(err) = self.save_world_info() {
            error!("Failed to save level.dat: {err}");
            crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
        }
        if let Err(error) = self.save_scoreboard() {
            error!("Failed to save scoreboard during shutdown: {error}");
            crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
        }
        if let Err(error) = self.save_command_storage() {
            error!("Failed to save command storage during shutdown: {error}");
            crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
        }
        info!("Completed worlds");
    }

    /// Broadcasts a packet to all players in all worlds.
    ///
    /// This function sends the specified packet to every connected player in every world managed by the server.
    ///
    /// # Arguments
    ///
    /// * `packet`: A reference to the packet to be broadcast. The packet must implement the `ClientPacket` trait.
    pub fn broadcast_packet_all<P: ClientPacket>(&self, packet: &P) {
        for world in self.worlds.load().iter() {
            world.broadcast_packet_all(packet);
        }
    }

    /// Broadcasts custom server links to all players on the server.
    pub fn broadcast_server_links(&self, links: &[pumpkin_protocol::Link<'_>]) {
        let packet = pumpkin_protocol::java::client::play::CPlayServerLinks::new(links);
        self.broadcast_packet_all(&packet);
    }

    pub fn broadcast_tab_list_header_footer(&self, header: &TextComponent, footer: &TextComponent) {
        let packet = CTabList::new(header, footer);
        for world in self.worlds.load().iter() {
            for player in world.players.load().iter() {
                *player
                    .tab_list_header
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = header.clone();
                *player
                    .tab_list_footer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = footer.clone();
            }
            world.broadcast_packet_all(&packet);
        }
    }

    pub fn broadcast_message(
        self: &Arc<Self>,
        message: &TextComponent,
        sender_name: &TextComponent,
        chat_type: u8,
        target_name: Option<&TextComponent>,
    ) {
        let mut event = ServerBroadcastEvent::new(message.clone(), sender_name.clone());
        self.plugin_manager.fire_blocking(self, &mut event);
        if !event.cancelled {
            for world in self.worlds.load().iter() {
                world.broadcast_message(&event.message, &event.sender, chat_type, target_name);
            }
        }
    }

    pub fn broadcast_chat_message(
        self: &Arc<Self>,
        message: &crate::net::chat::PlayerChatMessage,
        is_filtered: impl Fn(&crate::entity::player::Player) -> bool,
        sender_player: Option<&Arc<crate::entity::player::Player>>,
        chat_type: pumpkin_protocol::codec::var_int::VarInt,
        sender_name: &TextComponent,
        target_name: Option<&TextComponent>,
    ) {
        for world in self.worlds.load().iter() {
            world.broadcast_chat_message(
                message,
                &is_filtered,
                sender_player,
                chat_type,
                sender_name,
                target_name,
            );
        }
    }

    /// Gets the current difficulty of the server.
    pub fn get_difficulty(&self) -> Difficulty {
        self.level_info.load().difficulty
    }

    /// Sets the difficulty of the server.
    ///
    /// This function updates the difficulty level of the server and broadcasts the change to all players.
    /// It also iterates through all worlds to ensure the difficulty is applied consistently.
    /// If `force_update` is `true`, the difficulty will be set regardless of the current state.
    /// If `force_update` is `false`, the difficulty will only be updated if it is not locked.
    ///
    /// # Arguments
    ///
    /// * `difficulty`: The new difficulty level to set. This should be one of the variants of the `Difficulty` enum.
    /// * `force_update`: Whether to update the difficulty even if it is currently locked.
    ///
    /// # Note
    ///
    /// This function does not handle the actual mob spawn options update, which is a TODO item for future implementation.
    pub fn set_difficulty(&self, difficulty: Difficulty, force_update: bool) {
        let new_difficulty = if self.basic_config.hardcore {
            Difficulty::Hard
        } else {
            difficulty
        };

        let previous = crate::world::level_info::update_level_info_if(
            &self.level_info,
            |info| force_update || !info.difficulty_locked,
            |info| info.difficulty = new_difficulty,
        );
        if previous.difficulty_locked && !force_update {
            return;
        }
        let locked = previous.difficulty_locked;

        for world in self.worlds.load().iter() {
            // Server-created worlds share this ArcSwap. Rewriting it here could
            // overwrite a newer difficulty; only independent worlds need a write.
            if !Arc::ptr_eq(&self.level_info, &world.level_info) {
                world.set_difficulty(new_difficulty);
            }
            world.broadcast_editioned(
                &CChangeDifficulty::new(new_difficulty as u8, locked),
                &pumpkin_protocol::bedrock::client::CSetDifficulty {
                    difficulty: (new_difficulty as u32).into(),
                },
            );
        }
    }

    /// Sets the difficulty lock status of the server and broadcasts the update to all players.
    pub fn set_difficulty_locked(&self, locked: bool) {
        let previous = crate::world::level_info::update_level_info(&self.level_info, |info| {
            info.difficulty_locked = locked;
        });
        let difficulty = previous.difficulty;

        for world in self.worlds.load().iter() {
            world.broadcast_editioned(
                &CChangeDifficulty::new(difficulty as u8, locked),
                &pumpkin_protocol::bedrock::client::CSetDifficulty {
                    difficulty: (difficulty as u32).into(),
                },
            );
        }
    }

    /// Searches for a player by their username across all worlds.
    ///
    /// This function iterates through each world managed by the server and attempts to find a player with the specified username.
    /// If a player is found in any world, it returns an `Arc<Player>` reference to that player. Otherwise, it returns `None`.
    ///
    /// # Arguments
    ///
    /// * `name`: The username of the player to search for.
    ///
    /// # Returns
    ///
    /// An `Option<Arc<Player>>` containing the player if found, or `None` if not found.
    pub fn get_player_by_name(&self, name: &str) -> Option<Arc<Player>> {
        for world in self.worlds.load().iter() {
            if let Some(player) = world.get_player_by_name(name) {
                return Some(player);
            }
        }
        None
    }

    pub fn get_players_by_ip(&self, ip: IpAddr) -> Vec<Arc<Player>> {
        let mut players = Vec::<Arc<Player>>::new();

        for world in self.worlds.load().iter() {
            for player in world.players.load().iter() {
                if player.client.address().ip() == ip {
                    players.push(player.clone());
                }
            }
        }

        players
    }

    /// Returns all players from all worlds.
    pub fn get_all_players(&self) -> Vec<Arc<Player>> {
        let mut players = Vec::<Arc<Player>>::new();

        for world in self.worlds.load().iter() {
            players.extend(world.players.load().iter().cloned());
        }

        players
    }

    pub fn for_each_player<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<Player>),
    {
        let worlds = self.worlds.load();

        for world in worlds.iter() {
            let players = world.players.load();
            for player in players.iter() {
                f(player);
            }
        }
    }

    /// Returns a random player from any of the worlds, or `None` if all worlds are empty.
    pub fn get_random_player(&self) -> Option<Arc<Player>> {
        let players = self.get_all_players();
        players.choose(&mut rand::rng()).map(Arc::<_>::clone)
    }

    /// Searches for a player by their UUID across all worlds.
    ///
    /// This function iterates through each world managed by the server and attempts to find a player with the specified UUID.
    /// If a player is found in any world, it returns an `Arc<Player>` reference to that player. Otherwise, it returns `None`.
    ///
    /// # Arguments
    ///
    /// * `id`: The UUID of the player to search for.
    ///
    /// # Returns
    ///
    /// An `Option<Arc<Player>>` containing the player if found, or `None` if not found.
    pub fn get_player_by_uuid(&self, id: uuid::Uuid) -> Option<Arc<Player>> {
        for world in self.worlds.load().iter() {
            if let Some(player) = world.get_player_by_uuid(id) {
                return Some(player);
            }
        }
        None
    }

    /// Counts the total number of players across all worlds.
    ///
    /// This function iterates through each world and sums up the number of players currently connected to that world.
    ///
    /// # Returns
    ///
    /// The total number of players connected to the server.
    pub fn get_player_count(&self) -> usize {
        let mut count = 0;
        for world in self.worlds.load().iter() {
            count += world.players.load().len();
        }
        count
    }

    /// Similar to [`Server::get_player_count`] >= n, but may be more efficient since it stops its iteration through all worlds as soon as n players were found.
    pub fn has_n_players(&self, n: usize) -> bool {
        let mut count = 0;
        for world in self.worlds.load().iter() {
            count += world.players.load().len();
            if count >= n {
                return true;
            }
        }
        false
    }

    /// Returns the maximum number of players allowed on the server.
    #[must_use]
    pub const fn max_players(&self) -> u32 {
        self.advanced_config.networking.java.max_players
    }

    /// Starts the background telemetry task if enabled in configuration.
    pub fn start_telemetry(self: &Arc<Self>) {
        crate::telemetry::start_telemetry(self.clone());
    }

    /// Generates a new container id.
    pub fn new_container_id(&self) -> u32 {
        self.container_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Generates a new map id.
    pub fn next_map_id(&self) -> i32 {
        let id = self.map_id.fetch_add(1, Ordering::SeqCst);
        self.level_info.rcu(|level_info| {
            let mut new_level_info = (**level_info).clone();
            new_level_info.map_id = self.map_id.load(Ordering::SeqCst);
            new_level_info
        });
        id
    }

    pub const fn get_branding(&self) -> CPluginMessage<'_> {
        self.branding.get_branding()
    }

    pub const fn get_status(&self) -> &std::sync::Mutex<CachedStatus> {
        &self.listing
    }

    async fn get_or_init_key_store(&self) -> &Arc<KeyStore> {
        self.key_store
            .get_or_init(|| async {
                let (tx, rx) = tokio::sync::oneshot::channel();
                rayon::spawn(move || {
                    let _ = tx.send(Arc::new(KeyStore::new()));
                });
                rx.await.unwrap_or_else(|_| Arc::new(KeyStore::new()))
            })
            .await
    }

    pub async fn encryption_request<'a>(
        &'a self,
        verification_token: &'a [u8; 4],
        should_authenticate: bool,
    ) -> CEncryptionRequest<'a> {
        self.get_or_init_key_store().await.encryption_request(
            "",
            verification_token,
            should_authenticate,
        )
    }

    pub async fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let key_store = self.get_or_init_key_store().await.clone();
        let data = data.to_vec();
        let (tx, rx) = tokio::sync::oneshot::channel();
        rayon::spawn(move || {
            let _ = tx.send(key_store.decrypt(&data));
        });
        rx.await.map_err(|_| EncryptionError::FailedDecrypt)?
    }

    pub fn digest_secret(&self, secret: &[u8]) -> String {
        self.key_store.get().map_or_else(
            || KeyStore::new().get_digest(secret),
            |key_store| key_store.get_digest(secret),
        )
    }

    /// Vanilla `ServerCommonPacketListenerImpl.suspendFlushing`. Returns the suspended players.
    fn suspend_player_flushes(&self) -> Vec<Arc<Player>> {
        let mut suspended = Vec::new();
        self.for_each_player(|player| {
            if let Some(java) = player.client.java() {
                java.suspend_flushing();
                suspended.push(player.clone());
            }
        });
        suspended
    }

    /// Vanilla `resumeFlushing` / `Connection.flushChannel` at tick end (~50ms).
    /// Resumes the suspended set, not a fresh world scan
    /// off-tick respawn or world change can leave a player in no world's list at tick end.
    fn resume_player_flushes(suspended: &[Arc<Player>]) {
        for player in suspended {
            if let Some(java) = player.client.java() {
                java.resume_flushing();
            }
        }
    }

    /// Main server tick method. This now handles both player/network ticking (which always runs)
    /// and world/game logic ticking (which is affected by freeze state).
    pub fn tick(self: &Arc<Self>) {
        let _phase = self
            .packet_runner
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if crate::STOP_INTERRUPT.is_cancelled() || self.packet_runner.is_draining() {
            return;
        }
        self.packet_runner.retry_worlds();
        // Do not flush mid-tick; `Flush` is `flushChannel`.
        let suspended = self.suspend_player_flushes();
        if self.tick_rate_manager.runs_normally() || self.tick_rate_manager.is_sprinting() {
            self.tick_worlds();
            // Always run player and network ticking, even when game is frozen
        } else {
            self.sync_game_time();
            self.tick_players_and_network();
        }
        self.schedule_autosave();
        self.flush_pending_block_updates();
        Self::resume_player_flushes(&suspended);
    }

    fn flush_pending_block_updates(&self) {
        for world in self.worlds.load().iter() {
            world.flush_block_updates();
        }
    }

    /// Ticks essential server functions that must run even when the game is frozen.
    /// This includes player ticking (network, keep-alives) and flushing world updates to clients.
    fn tick_players_and_network(self: &Arc<Self>) {
        let worlds = self.worlds.load();
        let handle = self.runtime.clone();

        for world in worlds.iter() {
            // Generation completion remains active while simulation time is frozen.
            world.update_active_chunks();
            world.flush_block_updates();
            // World::tick runs block events on simulation ticks, including steps.

            let _callbacks = world.enter_simulation_callbacks();
            let players = world.players.load();
            let player_handle = handle.clone();
            players.par_iter().for_each(|player| {
                let _guard = player_handle.enter();
                player.tick(self);
            });
        }
    }

    /// Ticks the game logic for all worlds. This is the part that is affected by `/tick freeze`.
    fn tick_worlds(self: &Arc<Self>) {
        let source = crate::command::CommandSender::Console
            .into_source(self)
            .with_silent();
        let _ = self
            .datapack_manager
            .execute_function(self, &source, "#minecraft:tick");

        self.plugin_manager.tick_loaders(self);
        self.scheduled_functions.tick(
            self,
            self.tick_count.load(std::sync::atomic::Ordering::Relaxed) as u64,
        );

        self.sync_game_time();

        let worlds = self.worlds.load();
        let handle = self.runtime.clone();
        worlds.par_iter().for_each(|world| {
            let _guard = handle.enter();
            world.tick(self);
        });

        // Global tasks
        self.player_data_storage.tick(self);
    }

    /// Vanilla `tickChildren` "timeSync": every 20 ticks, also while frozen.
    /// Sync before `tickTime()`. `tick_count + 1` is this vanilla tick.
    fn sync_game_time(&self) {
        if self.tick_count.load(Ordering::Relaxed).wrapping_add(1) % 20 == 0
            && let Some(overworld) =
                self.worlds.load().iter().find(|world| {
                    world.dimension.minecraft_name == Dimension::OVERWORLD.minecraft_name
                })
        {
            overworld.force_game_time_synchronization(self);
        }
    }

    /// Updates the tick time statistics with the duration of the last tick.
    pub fn update_tick_times(&self, tick_duration_nanos: i64) {
        let tick_count = self.tick_count.fetch_add(1, Ordering::Relaxed);
        let index = (tick_count % 100) as usize;

        let mut tick_times = self
            .tick_times_nanos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let old_time = tick_times[index];
        tick_times[index] = tick_duration_nanos;
        drop(tick_times);

        self.aggregated_tick_times_nanos
            .fetch_add(tick_duration_nanos - old_time, Ordering::Relaxed);

        let target_tick_nanos = self.tick_rate_manager.nanoseconds_per_tick();
        let idle_nanos = target_tick_nanos.saturating_sub(tick_duration_nanos);

        let sample_slice = [
            tick_duration_nanos.max(target_tick_nanos),
            tick_duration_nanos,
            0,
            idle_nanos,
        ];
        let packet = pumpkin_protocol::java::client::play::CDebugSample::new(
            &sample_slice,
            pumpkin_protocol::codec::var_int::VarInt(0),
        );
        for world in self.worlds.load().iter() {
            let players = world.players.load();
            let recipients = players
                .iter()
                .filter(|player| {
                    player.subscribed_debug_sample.load(Ordering::Relaxed)
                        && player.permission_lvl.load() >= pumpkin_util::PermissionLvl::Two
                })
                .filter_map(|player| player.client.java());
            World::broadcast_java_clients(&packet, recipients);
        }
    }

    /// Gets the rolling average tick time over the last 100 ticks, in nanoseconds.
    pub fn get_average_tick_time_nanos(&self) -> i64 {
        let tick_count = self.tick_count.load(Ordering::Relaxed);
        let sample_size = (tick_count as usize).min(100);
        if sample_size == 0 {
            return 0;
        }
        self.aggregated_tick_times_nanos.load(Ordering::Relaxed) / sample_size as i64
    }

    /// Returns the average Milliseconds Per Tick (MSPT).
    pub fn get_mspt(&self) -> f64 {
        let avg_nanos = self.get_average_tick_time_nanos();
        // Convert nanoseconds to decimal milliseconds
        avg_nanos as f64 / 1_000_000.0
    }

    /// Returns the Ticks Per Second (TPS).
    pub fn get_tps(&self) -> f64 {
        let mspt = self.get_mspt();
        if mspt <= 0.0 {
            return 0.0;
        }
        1000.0 / mspt
    }

    /// Returns a copy of the last 100 tick times.
    pub fn get_tick_times_nanos_copy(&self) -> [i64; 100] {
        *self
            .tick_times_nanos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub async fn register_service(self: &Arc<Self>, service_name: String) {
        let mut service_event =
            crate::plugin::api::events::server::service_register::ServiceRegisterEvent::new(
                service_name,
            );
        self.plugin_manager.fire(self, &mut service_event).await;
    }

    pub async fn unregister_service(self: &Arc<Self>, service_name: String) {
        let mut service_event =
            crate::plugin::api::events::server::service_unregister::ServiceUnregisterEvent::new(
                service_name,
            );
        self.plugin_manager.fire(self, &mut service_event).await;
    }

    pub async fn tab_complete(self: &Arc<Self>, buffer: String, completions: Vec<String>) {
        let mut tab_event = crate::plugin::api::events::server::tab_complete::TabCompleteEvent::new(
            buffer,
            completions,
        );
        self.plugin_manager.fire(self, &mut tab_event).await;
    }

    pub async fn enable_plugin(self: &Arc<Self>, plugin_name: String) {
        let mut enable_event =
            crate::plugin::api::events::server::plugin_enable::PluginEnableEvent::new(plugin_name);
        self.plugin_manager.fire(self, &mut enable_event).await;
    }

    pub async fn disable_plugin(self: &Arc<Self>, plugin_name: String) {
        let mut disable_event =
            crate::plugin::api::events::server::plugin_disable::PluginDisableEvent::new(
                plugin_name,
            );
        self.plugin_manager.fire(self, &mut disable_event).await;
    }
}

#[cfg(test)]
mod player_data_tests {
    use super::*;
    use crate::net::PacketRateLimiter;
    use crate::net::java::{JavaClient, pending::PendingConnection};
    use crate::plugin::EventHandler;
    use crate::plugin::api::events::EventPriority;
    use futures::future::BoxFuture;
    use pumpkin_protocol::ConnectionState;
    use pumpkin_util::world_seed::Seed;

    #[tokio::test]
    async fn player_data_failure_rejects_login_before_player_publication_or_save() {
        failed_persistence_rejects_login(AdmissionFailure::Player).await;
    }

    #[tokio::test]
    async fn advancement_failure_rejects_before_session_login_event_or_save() {
        failed_persistence_rejects_login(AdmissionFailure::Advancement).await;
    }

    #[tokio::test]
    async fn refused_spawn_releases_uuid_before_publication_or_any_family_snapshot() {
        failed_persistence_rejects_login(AdmissionFailure::Spawn).await;
    }

    #[tokio::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the real partial-spawn failure, packet exit, three final writers and UUID reuse together."
    )]
    async fn returning_spawn_refusal_closes_packet_progress_and_runs_existing_session_cleanup() {
        let fixture = crate::data::player_server::test::player_fixture().await;
        let world = fixture.player.world();
        let java = fixture.client.java().unwrap();
        java.set_player(&fixture.player);
        fixture
            .player
            .has_played_before
            .store(true, Ordering::Relaxed);
        // Existing-only view delivery keeps this fixture focused on the direct
        // spawn activation wait instead of unrelated simulation-distance loads.
        fixture
            .player
            .gamemode
            .store(pumpkin_util::GameMode::Spectator);
        world.level_info.rcu(|current| {
            let mut info = (**current).clone();
            info.game_rules.spectators_generate_chunks = false;
            info
        });
        fixture.player.config.rcu(|current| {
            let mut config = (**current).clone();
            config.view_distance = std::num::NonZero::new(2).unwrap();
            config
        });
        fixture
            .player
            .living_entity
            .entity
            .set_pos(pumpkin_util::math::vector3::Vector3::new(
                256.5, 70.0, 256.5,
            ));
        let path = world.level.level_folder.region_folder.join("r.0.0.mca");
        fs::write(&path, b"refused returning-player region").unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            world.spawn_java_player(
                &fixture.server.basic_config,
                &fixture.player,
                &fixture.server,
            ),
        )
        .await
        .unwrap();
        assert!(
            java.is_closed(),
            "a refused ready-chunk wait must close the connected session"
        );
        tokio::time::timeout(
            Duration::from_secs(1),
            java.progress_player_packets(&fixture.player, &fixture.server),
        )
        .await
        .unwrap();
        let exit_status = std::sync::atomic::AtomicI32::new(0);
        tokio::time::timeout(
            Duration::from_secs(10),
            crate::finish_player_session_with_status(
                &fixture.server,
                &fixture.player,
                &exit_status,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(exit_status.load(Ordering::Relaxed), 0);
        assert!(fixture.player.session.lock().unwrap().is_none());
        let uuid = fixture.player.gameprofile.id;
        drop(
            tokio::time::timeout(
                Duration::from_secs(1),
                fixture
                    .server
                    .player_data_storage
                    .reserve_player_session(uuid),
            )
            .await
            .unwrap(),
        );
        assert!(fixture.server.get_player_by_uuid(uuid).is_none());
        assert!(
            fixture
                .server
                .player_data_storage
                .load_data(&uuid)
                .unwrap()
                .is_some()
        );
        assert!(
            fixture
                .server
                .advancement_manager
                .advancement_path
                .join(format!("{uuid}.json"))
                .exists()
        );
        assert!(fixture.server.statistics_manager.player_path(uuid).exists());
        assert_eq!(fs::read(&path).unwrap(), b"refused returning-player region");
        crate::data::player_server::test::close_fixture(&fixture).await;
    }

    #[tokio::test]
    async fn ready_spawn_admits_first_and_returning_players_beside_a_refused_neighbor() {
        for returning in [false, true] {
            spawn_halo_admission_control(true, returning).await;
        }
    }

    #[tokio::test]
    async fn unready_spawn_refuses_required_halo_before_first_or_returning_publication() {
        for returning in [false, true] {
            spawn_halo_admission_control(false, returning).await;
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep real refusal, first/returning admission and publication/persistence controls together."
    )]
    async fn spawn_halo_admission_control(center_ready: bool, returning: bool) {
        use crate::data::player_server::test::{
            close_fixture, java_client, player_fixture, stop_chunk_workers,
        };
        use pumpkin_util::math::vector2::Vector2;
        use pumpkin_world::{
            chunk::ChunkData, chunk_system::ChunkLoading, level::ChunkUnavailable,
        };

        let fixture = player_fixture().await;
        let server = &fixture.server;
        let world = fixture.player.world();
        // Use the real final writer to seed the returning player's existing data.
        tokio::time::timeout(
            Duration::from_secs(10),
            crate::finish_player_session(server, &fixture.player),
        )
        .await
        .unwrap()
        .unwrap();
        let profile = GameProfile {
            id: if returning {
                fixture.player.gameprofile.id
            } else {
                uuid::Uuid::from_u128(103)
            },
            name: "SpawnHalo".into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };
        let center = Vector2::new(0, 0);
        let neighbor = Vector2::new(-1, 0);
        world.level.loaded_chunks.remove(&neighbor);
        let path = world.level.level_folder.region_folder.join("r.-1.0.mca");
        fs::write(&path, b"refused spawn neighbor region").unwrap();
        let failed = world
            .level
            .chunk_listener
            .add_single_chunk_listener(neighbor);
        {
            let mut loading = world.level.chunk_loading.lock().unwrap();
            // A read-only request isolates this refusal from the healthy center.
            loading.add_ticket(neighbor, ChunkLoading::MAX_LEVEL - 1);
            loading.send_change();
        }
        let refusal = tokio::time::timeout(Duration::from_secs(10), failed)
            .await
            .unwrap()
            .err()
            .expect("stored neighbor must be refused");
        assert!(
            matches!(refusal, ChunkUnavailable::ReadFailed { position, .. } if position == neighbor)
        );
        {
            let mut loading = world.level.chunk_loading.lock().unwrap();
            loading.remove_ticket(neighbor, ChunkLoading::MAX_LEVEL - 1);
            loading.send_change();
        }
        stop_chunk_workers(&fixture).await;
        let terrain = ChunkData::empty_sync(0, 0);
        terrain
            .post_processing_ready
            .store(center_ready, Ordering::Release);
        world.level.loaded_chunks.insert(center, terrain);
        for x in -1..=1 {
            for z in -1..=1 {
                let position = Vector2::new(x, z);
                if position != neighbor {
                    assert!(world.level.is_chunk_loaded(&position));
                }
            }
        }
        assert!(world.level.chunk_listener.failure(center).is_none());
        assert!(world.level.chunk_listener.failure(neighbor).is_some());
        assert!(!world.level.is_chunk_loaded(&neighbor));
        world.level_info.rcu(|current| {
            let mut info = (**current).clone();
            info.spawn_x = 0;
            info.spawn_z = 0;
            info.spawn_y = 65;
            info
        });
        let uuid = profile.id;
        let player_path = fixture
            .directory
            .path()
            .join("players/data")
            .join(format!("{uuid}.dat"));
        let family_paths = [
            player_path,
            server
                .advancement_manager
                .advancement_path
                .join(format!("{uuid}.json")),
            server.statistics_manager.player_path(uuid),
        ];
        let family_bytes_before = family_paths
            .each_ref()
            .map(|path| path.exists().then(|| fs::read(path).unwrap()));
        assert_eq!(family_bytes_before[0].is_some(), returning);
        let login = Arc::new(ObserveLogin(AtomicBool::new(false)));
        server.plugin_manager.register::<PlayerLoginEvent, _>(
            login.clone(),
            EventPriority::Normal,
            false,
        );
        let (client, _peer) = java_client(server, &profile).await;
        let admitted = tokio::time::timeout(
            Duration::from_secs(5),
            server.add_player(client.clone(), profile, None),
        )
        .await
        .unwrap();
        assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert!(!world.level.is_chunk_loaded(&neighbor));
        if center_ready {
            let (player, admitted_world) =
                admitted.expect("a ready center does not require the refused halo");
            assert!(!client.closed());
            assert!(login.0.load(Ordering::Relaxed));
            assert_eq!(player.has_played_before.load(Ordering::Relaxed), returning);
            assert!(Arc::ptr_eq(&admitted_world, &world));
            assert!(player.session.lock().unwrap().is_some());
            assert!(
                server
                    .get_player_by_uuid(uuid)
                    .is_some_and(|published| Arc::ptr_eq(&published, &player))
            );
            tokio::time::timeout(
                Duration::from_secs(10),
                crate::finish_player_session(server, &player),
            )
            .await
            .unwrap()
            .unwrap();
        } else {
            assert!(admitted.is_none());
            assert!(client.closed());
            assert!(!login.0.load(Ordering::Relaxed));
            assert!(server.get_player_by_uuid(uuid).is_none());
            drop(
                tokio::time::timeout(
                    Duration::from_secs(1),
                    server.player_data_storage.reserve_player_session(uuid),
                )
                .await
                .unwrap(),
            );
            server.player_data_storage.save_all_players(server).unwrap();
            server
                .advancement_manager
                .save_all_players(&[])
                .await
                .unwrap();
            server
                .statistics_manager
                .save_all_players(&[])
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), client.java().unwrap().await_tasks())
            .await
            .unwrap();
        close_fixture(&fixture).await;
        if !center_ready {
            let family_bytes_after = family_paths
                .each_ref()
                .map(|path| path.exists().then(|| fs::read(path).unwrap()));
            assert_eq!(family_bytes_after, family_bytes_before);
        }
        assert_eq!(fs::read(&path).unwrap(), b"refused spawn neighbor region");
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum AdmissionFailure {
        Player,
        Advancement,
        Spawn,
    }

    struct ObserveLogin(AtomicBool);

    impl EventHandler<PlayerLoginEvent> for ObserveLogin {
        fn handle<'a>(
            &'a self,
            _server: &'a Arc<Server>,
            _event: &'a PlayerLoginEvent,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                self.0.store(true, Ordering::Relaxed);
            })
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep each save failure with admission, publication and no-replacement controls."
    )]
    async fn failed_persistence_rejects_login(failure: AdmissionFailure) {
        let advancement_failure = failure == AdmissionFailure::Advancement;
        let spawn_failure = failure == AdmissionFailure::Spawn;
        let directory = tempfile::tempdir().unwrap();
        let basic = BasicConfiguration {
            default_level_name: directory.path().to_string_lossy().into_owned(),
            seed: Seed(0),
            allow_nether: false,
            allow_end: false,
            allow_chat_reports: false,
            use_favicon: false,
            ..BasicConfiguration::default()
        };
        let mut advanced = AdvancedConfiguration::default();
        advanced.networking.java.online_mode = false;
        advanced.networking.bedrock.online_mode = false;
        let data = VanillaData {
            banned_ip_list: std::sync::RwLock::default(),
            banned_player_list: std::sync::RwLock::default(),
            operator_config: std::sync::RwLock::default(),
            user_cache: std::sync::RwLock::default(),
            whitelist_config: std::sync::RwLock::default(),
        };
        let server = Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            data,
            Vec::new(),
        )
        .await
        .unwrap();
        let uuid = uuid::Uuid::from_u128(101);
        let path = directory
            .path()
            .join("players/data")
            .join(format!("{uuid}.dat"));
        let backup = path.with_extension("dat_old");
        let advancement_path = server
            .advancement_manager
            .advancement_path
            .join(format!("{uuid}.json"));
        let statistics_path = server.statistics_manager.player_path(uuid);
        let world = server.worlds.load().first().cloned().unwrap();
        let region_path = world.level.level_folder.region_folder.join("r.0.0.mca");
        if spawn_failure {
            world.level_info.rcu(|current| {
                let mut info = (**current).clone();
                info.spawn_x = 256;
                info.spawn_z = 256;
                info
            });
            fs::write(&region_path, b"refused spawn region").unwrap();
            fs::write(&advancement_path, b"{}").unwrap();
            fs::write(&statistics_path, br#"{"DataVersion":5023,"stats":{}}"#).unwrap();
        } else if advancement_failure {
            fs::write(&advancement_path, b"corrupt advancement").unwrap();
        } else {
            fs::write(&path, b"corrupt primary").unwrap();
            fs::write(&backup, b"corrupt backup").unwrap();
        }
        let login = Arc::new(ObserveLogin(AtomicBool::new(false)));
        server.plugin_manager.register::<PlayerLoginEvent, _>(
            login.clone(),
            EventPriority::Normal,
            false,
        );
        let profile = GameProfile {
            id: uuid,
            name: "BrokenPlayer".into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };

        // A temporary connected pair supplies the real Java client without a
        // login handshake, world ticks, external auth or a running game listener.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (remote, accepted) =
            tokio::join!(tokio::net::TcpStream::connect(address), listener.accept());
        let _remote = remote.unwrap();
        let (stream, address) = accepted.unwrap();
        drop(listener);
        let pending = PendingConnection::new(
            stream,
            address,
            0,
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(&server),
        );
        pending.connection_state.store(ConnectionState::Play);
        let mut java = JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
        java.start_outgoing_packet_task();
        let client = Arc::new(ClientPlatform::Java(java));

        assert!(
            tokio::time::timeout(
                Duration::from_secs(5),
                server.add_player(client.clone(), profile.clone(), None)
            )
            .await
            .unwrap()
            .is_none()
        );
        drop(
            tokio::time::timeout(
                Duration::from_secs(1),
                server.player_data_storage.reserve_player_session(uuid),
            )
            .await
            .unwrap(),
        );
        if spawn_failure {
            let (retry, _peer) =
                crate::data::player_server::test::java_client(&server, &profile).await;
            assert!(
                tokio::time::timeout(
                    Duration::from_secs(2),
                    server.add_player(retry.clone(), profile.clone(), None)
                )
                .await
                .unwrap()
                .is_none()
            );
            assert!(retry.closed());
            tokio::time::timeout(Duration::from_secs(5), retry.java().unwrap().await_tasks())
                .await
                .unwrap();
            assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        }
        let java = client.java().unwrap();
        assert!(java.is_closed());
        assert!(java.player.load().is_none());
        assert!(!login.0.load(Ordering::Relaxed));
        assert!(
            server
                .worlds
                .load()
                .iter()
                .all(|world| world.players.load().is_empty())
        );
        server
            .player_data_storage
            .save_all_players(&server)
            .unwrap();
        server
            .advancement_manager
            .save_all_players(&[])
            .await
            .unwrap();
        server
            .statistics_manager
            .save_all_players(&[])
            .await
            .unwrap();
        if advancement_failure {
            // Even callers bypassing admission cannot turn an unloaded or failed
            // advancement instance into a manager snapshot and overwrite the file.
            let world = server.worlds.load().first().cloned().unwrap();
            let unpublished = Arc::new(Player::new(
                client.clone(),
                profile,
                PlayerConfig::default(),
                &world,
                pumpkin_util::GameMode::Survival,
            ));
            assert!(matches!(
                server.advancement_manager.save_player(&unpublished).await,
                Err(crate::entity::player::advancement::AdvancementDataError::NotLoaded)
            ));
            assert!(unpublished.advancements.lock().unwrap().load().is_err());
            assert!(matches!(
                server.advancement_manager.save_player(&unpublished).await,
                Err(crate::entity::player::advancement::AdvancementDataError::NotLoaded)
            ));
            assert!(unpublished.session.lock().unwrap().is_none());
        }
        tokio::time::timeout(Duration::from_secs(5), java.await_tasks())
            .await
            .unwrap();
        server.shutdown().await;
        if spawn_failure {
            assert!(!path.exists());
            assert!(!backup.exists());
            assert_eq!(fs::read(&advancement_path).unwrap(), b"{}");
            assert_eq!(
                fs::read(&statistics_path).unwrap(),
                br#"{"DataVersion":5023,"stats":{}}"#
            );
            assert_eq!(fs::read(&region_path).unwrap(), b"refused spawn region");
        } else if advancement_failure {
            assert_eq!(fs::read(&advancement_path).unwrap(), b"corrupt advancement");
            assert!(!path.exists());
            assert!(!backup.exists());
            assert_eq!(
                fs::read_dir(advancement_path.parent().unwrap())
                    .unwrap()
                    .count(),
                1
            );
        } else {
            assert_eq!(fs::read(&path).unwrap(), b"corrupt primary");
            assert_eq!(fs::read(&backup).unwrap(), b"corrupt backup");
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
        }
    }
}
