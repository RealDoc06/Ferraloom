use crate::{
    entity::{NBTStorage, player::Player},
    server::Server,
};
use crossbeam::atomic::AtomicCell;
use pumpkin_inventory::screen_handler::ScreenHandler;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_world::data::player_data::{PlayerDataError, PlayerDataStorage, PlayerSnapshot};
use std::collections::HashSet;
use std::io;
use std::sync::{Arc, Mutex};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, error};

/// Helper for managing player data in the server context.
///
/// This struct provides server-wide access to the `PlayerDataStorage` and
/// convenience methods for player handling.
pub struct ServerPlayerData {
    storage: Arc<PlayerDataStorage>,
    save_interval: Duration,
    last_save: AtomicCell<Instant>,
    leaving: Mutex<HashSet<uuid::Uuid>>,
}

/// Keeps a departing UUID unavailable through callbacks and its final save.
/// Dropping a cancelled or failed leave also releases admission.
pub(crate) struct PlayerLeaveGuard<'a> {
    data: &'a ServerPlayerData,
    uuid: uuid::Uuid,
}

impl Drop for PlayerLeaveGuard<'_> {
    fn drop(&mut self) {
        self.data
            .leaving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.uuid);
    }
}

impl ServerPlayerData {
    /// Creates a new `ServerPlayerData` with specified configuration.
    pub fn new(data_path: impl Into<PathBuf>, save_interval: Duration, enabled: bool) -> Self {
        Self {
            storage: Arc::new(PlayerDataStorage::new(data_path, enabled)),
            save_interval,
            last_save: AtomicCell::new(Instant::now()),
            leaving: Mutex::new(HashSet::new()),
        }
    }

    /// Call before removing the player from a world. The guard owns no mutex
    /// lock, so callbacks, disk IO and awaits can safely run while it is alive.
    pub(crate) fn begin_player_leave(
        &self,
        uuid: uuid::Uuid,
    ) -> Result<PlayerLeaveGuard<'_>, PlayerDataError> {
        let inserted = self
            .leaving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(uuid);
        if !inserted {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Player leave is already in progress",
            )
            .into());
        }
        Ok(PlayerLeaveGuard { data: self, uuid })
    }

    /// Handles a player leaving the server.
    ///
    /// This function saves player data when they disconnect.
    ///
    /// # Arguments
    ///
    /// * `player` - The player who left.
    ///
    /// # Returns
    ///
    /// A Result indicating success or the error that occurred.
    pub fn handle_player_leave(&self, player: &Arc<Player>) -> Result<(), PlayerDataError> {
        player
            .player_screen_handler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .on_closed(player.as_ref());
        player.on_handled_screen_closed();

        self.extract_data_and_save_player(player)
    }

    /// Performs periodic maintenance tasks.
    ///
    /// This function is called synchronously on the server tick loop to check
    /// if it is time to save player data.
    pub fn tick(&self, server: &Server) {
        let now = Instant::now();

        // Only save players periodically based on save_interval
        let last_save = self.last_save.load();
        let should_save = now.duration_since(last_save) >= self.save_interval;

        if should_save && self.storage.is_save_enabled() {
            self.last_save.store(now);
            // Snapshot all online players periodically across all worlds
            let snapshots = self.snapshot_players(server);

            if snapshots.is_empty() {
                return;
            }

            let storage = self.storage.clone();
            rayon::spawn(move || {
                for snapshot in snapshots {
                    let uuid = snapshot.uuid();
                    if let Err(e) = storage.save_player_data(snapshot) {
                        error!("Failed to save player data for {uuid}: {e}");
                    }
                }
                debug!("Periodic player data save completed");
            });
        }
    }

    /// Capture before queueing disk IO, keeping the original version with the NBT.
    fn snapshot_players(&self, server: &Server) -> Vec<PlayerSnapshot> {
        let mut snapshots = Vec::new();
        for world in server.worlds.load().iter() {
            for player in world.players.load().iter() {
                snapshots.push(
                    self.storage
                        .snapshot_player_data(player.gameprofile.id, |nbt| {
                            player.write_nbt(nbt);
                        }),
                );
            }
        }
        snapshots
    }

    /// Saves all players' data immediately.
    ///
    /// This function immediately saves all online players' data to disk.
    /// Useful for server shutdown or backup operations.
    pub fn save_all_players(&self, server: &Server) -> Result<(), PlayerDataError> {
        let mut total_players = 0;

        // Save players from all worlds
        for world in server.worlds.load().iter() {
            for player in world.players.load().iter() {
                self.extract_data_and_save_player(player)?;
                total_players += 1;
            }
        }

        debug!("Saved data for {total_players} online players");
        Ok(())
    }

    /// Loads returning-player data. Only absent files or disabled saving permit
    /// defaults; storage errors must prevent the caller from admitting the player.
    pub fn load_data(&self, uuid: &uuid::Uuid) -> Result<Option<NbtCompound>, PlayerDataError> {
        if self
            .leaving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(uuid)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Previous player session is still saving",
            )
            .into());
        }
        let (should_load, data) = self.storage.load_player_data(uuid)?;
        Ok(should_load.then_some(data))
    }

    /// Extracts and saves data from a player.
    ///
    /// This function extracts NBT data from a player and saves it to disk.
    ///
    /// # Arguments
    ///
    /// * `player` - The player to extract and save data for.
    ///
    /// # Returns
    ///
    /// A Result indicating success or the error that occurred.
    pub fn extract_data_and_save_player(&self, player: &Player) -> Result<(), PlayerDataError> {
        if !self.storage.is_save_enabled() {
            return Ok(());
        }

        let snapshot = self
            .storage
            .snapshot_player_data(player.gameprofile.id, |nbt| {
                player.write_nbt(nbt);
            });
        self.storage.save_player_data(snapshot)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::data::VanillaData;
    use crate::data::player_server::ServerPlayerData;
    use crate::entity::{NBTStorage, player::Player};
    use crate::net::java::{JavaClient, pending::PendingConnection};
    use crate::net::{ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig};
    use crate::plugin::EventHandler;
    use crate::plugin::api::events::{EventPriority, player::PlayerLeaveEvent};
    use crate::server::Server;
    use arc_swap::ArcSwap;
    use futures::future::BoxFuture;
    use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_protocol::ConnectionState;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::data::player_data::{PlayerDataError, PlayerDataStorage};
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use std::time::Instant;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn save(
        storage: &PlayerDataStorage,
        uuid: Uuid,
        data: NbtCompound,
    ) -> Result<(), PlayerDataError> {
        let snapshot = storage.snapshot_player_data(uuid, |nbt| *nbt = data);
        storage.save_player_data(snapshot)
    }

    struct PlayerFixture {
        _directory: tempfile::TempDir,
        server: Arc<Server>,
        player: Arc<Player>,
        client: Arc<ClientPlatform>,
        _remote: tokio::net::TcpStream,
    }

    async fn player_fixture() -> PlayerFixture {
        let directory = tempdir().unwrap();
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
        let vanilla_data = VanillaData {
            banned_ip_list: Default::default(),
            banned_player_list: Default::default(),
            operator_config: Default::default(),
            user_cache: Default::default(),
            whitelist_config: Default::default(),
        };
        let server = Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            vanilla_data,
            Vec::new(),
        )
        .await
        .unwrap();
        let uuid = Uuid::from_u128(102);
        let profile = GameProfile {
            id: uuid,
            name: "QueuedPlayer".into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };
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
        let world = server.worlds.load().first().cloned().unwrap();
        let player = Arc::new(Player::new(
            client.clone(),
            profile,
            PlayerConfig::default(),
            &world,
            pumpkin_util::GameMode::Survival,
        ));
        player
            .screen_handler_sync_handler
            .store_player(player.clone());
        // Exercise the real world registry without Server::add_player's user-cache IO.
        world.add_player(&player).unwrap();
        PlayerFixture {
            _directory: directory,
            server,
            player,
            client,
            _remote,
        }
    }

    async fn close_fixture(fixture: &PlayerFixture) {
        let java = fixture.client.java().unwrap();
        java.close();
        tokio::time::timeout(Duration::from_secs(5), java.await_tasks())
            .await
            .unwrap();
        fixture.server.shutdown().await;
    }

    struct ReloginDuringLeave {
        observed: AtomicBool,
        pause: bool,
    }

    impl EventHandler<PlayerLeaveEvent> for ReloginDuringLeave {
        fn handle<'a>(
            &'a self,
            server: &'a Arc<Server>,
            event: &'a PlayerLeaveEvent,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                let uuid = event.player.gameprofile.id;
                assert!(server.get_player_by_uuid(uuid).is_none());
                let Err(PlayerDataError::Io(error)) = server.player_data_storage.load_data(&uuid)
                else {
                    panic!("removed UUID must remain unavailable until its final save completes");
                };
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
                // Leave callbacks may still change the final persisted state.
                event.player.experience_level.store(9, Ordering::Relaxed);
                self.observed.store(true, Ordering::Relaxed);
                if self.pause {
                    std::future::pending::<()>().await;
                }
            })
        }
    }

    #[tokio::test]
    async fn server_immediate_relogin_loads_final_leave_callback_changes() {
        let fixture = player_fixture().await;
        let data = &fixture.server.player_data_storage;
        let uuid = fixture.player.gameprofile.id;
        fixture.player.experience_level.store(1, Ordering::Relaxed);
        data.extract_data_and_save_player(&fixture.player).unwrap();
        let handler = Arc::new(ReloginDuringLeave {
            observed: AtomicBool::new(false),
            pause: false,
        });
        fixture
            .server
            .plugin_manager
            .register::<PlayerLeaveEvent, _>(handler.clone(), EventPriority::Normal, false);
        crate::finish_player_session(&fixture.server, &fixture.player)
            .await
            .unwrap();
        assert!(handler.observed.load(Ordering::Relaxed));
        assert!(fixture.server.get_player_by_uuid(uuid).is_none());

        let mut loaded = data.load_data(&uuid).unwrap().unwrap();
        let world = fixture.player.world();
        let mut rejoined = Player::new(
            fixture.client.clone(),
            fixture.player.gameprofile.clone(),
            PlayerConfig::default(),
            &world,
            pumpkin_util::GameMode::Survival,
        );
        rejoined.read_nbt(&mut loaded);
        assert_eq!(rejoined.experience_level.load(Ordering::Relaxed), 9);
        let rejoined = Arc::new(rejoined);
        rejoined
            .screen_handler_sync_handler
            .store_player(rejoined.clone());
        world.add_player(&rejoined).unwrap();

        // A failed final write also releases admission. Repair the deliberately
        // blocked temp file and verify the existing save can be loaded again.
        let path = data.storage.get_player_data_path(&uuid);
        let previous = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            crate::finish_player_session(&fixture.server, &rejoined)
                .await
                .is_err()
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, previous).unwrap();
        assert_eq!(
            data.load_data(&uuid).unwrap().unwrap().get_int("XpLevel"),
            Some(9)
        );
        close_fixture(&fixture).await;
    }

    #[tokio::test]
    async fn server_cancelled_leave_releases_login_admission() {
        let fixture = player_fixture().await;
        let data = &fixture.server.player_data_storage;
        let uuid = fixture.player.gameprofile.id;
        fixture.player.experience_level.store(1, Ordering::Relaxed);
        data.extract_data_and_save_player(&fixture.player).unwrap();
        let handler = Arc::new(ReloginDuringLeave {
            observed: AtomicBool::new(false),
            pause: true,
        });
        fixture
            .server
            .plugin_manager
            .register::<PlayerLeaveEvent, _>(handler.clone(), EventPriority::Normal, false);
        let mut leave = Box::pin(crate::finish_player_session(
            &fixture.server,
            &fixture.player,
        ));
        assert!(futures::poll!(&mut leave).is_pending());
        assert!(handler.observed.load(Ordering::Relaxed));
        drop(leave);
        assert_eq!(
            data.load_data(&uuid).unwrap().unwrap().get_int("XpLevel"),
            Some(1)
        );
        close_fixture(&fixture).await;
    }

    #[tokio::test]
    async fn server_queued_periodic_snapshot_cannot_roll_back_disconnect_or_save_all() {
        let fixture = player_fixture().await;
        let server = &fixture.server;
        let player = &fixture.player;
        let uuid = player.gameprofile.id;
        let data = &server.player_data_storage;
        player.experience_level.store(1, Ordering::Relaxed);
        data.extract_data_and_save_player(player).unwrap();
        let path = data.storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");

        for (new_level, disconnect) in [(2, true), (4, false)] {
            // Hold exactly the payload captured by tick(), without scheduling or
            // blocking any Rayon worker, then drain it after the newer save.
            let queued = data.snapshot_players(server);
            assert_eq!(queued.len(), 1);
            player.experience_level.store(new_level, Ordering::Relaxed);
            if disconnect {
                data.handle_player_leave(player).unwrap();
            } else {
                data.save_all_players(server).unwrap();
            }
            let primary_bytes = std::fs::read(&path).unwrap();
            let backup_bytes = std::fs::read(&backup).unwrap();
            for snapshot in queued {
                data.storage.save_player_data(snapshot).unwrap();
            }
            assert_eq!(std::fs::read(&path).unwrap(), primary_bytes);
            assert_eq!(std::fs::read(&backup).unwrap(), backup_bytes);
            assert_eq!(
                data.load_data(&uuid).unwrap().unwrap().get_int("XpLevel"),
                Some(new_level)
            );
        }

        close_fixture(&fixture).await;
    }

    #[test]
    fn server_player_load_propagates_corruption_and_io_errors_without_defaulting() {
        let directory = tempdir().unwrap();
        let player_data = ServerPlayerData::new(directory.path(), Duration::from_secs(30), true);
        let uuid = Uuid::from_u128(99);
        assert_eq!(player_data.load_data(&uuid).unwrap(), None);
        let path = player_data.storage.get_player_data_path(&uuid);
        std::fs::write(&path, b"corrupt player").unwrap();
        assert!(matches!(
            player_data.load_data(&uuid),
            Err(PlayerDataError::Nbt(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt player");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(
            player_data.load_data(&uuid),
            Err(PlayerDataError::Io(_))
        ));
        assert!(path.is_dir());
    }

    #[test]
    fn server_player_load_returns_recovered_data_and_keeps_disabled_defaults() {
        let directory = tempdir().unwrap();
        let player_data = ServerPlayerData::new(directory.path(), Duration::from_secs(30), true);
        let uuid = Uuid::from_u128(100);
        let mut previous = NbtCompound::new();
        previous.put_int("InventoryMarker", 42);
        save(&player_data.storage, uuid, previous.clone()).unwrap();
        save(&player_data.storage, uuid, NbtCompound::new()).unwrap();
        let path = player_data.storage.get_player_data_path(&uuid);
        std::fs::write(&path, b"corrupt player").unwrap();
        assert_eq!(player_data.load_data(&uuid).unwrap(), Some(previous));

        std::fs::write(&path, b"disabled save preserves this").unwrap();
        let disabled = ServerPlayerData::new(directory.path(), Duration::from_secs(30), false);
        assert_eq!(disabled.load_data(&uuid).unwrap(), None);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"disabled save preserves this"
        );
    }

    #[tokio::test]
    async fn player_data_storage_new() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let storage = PlayerDataStorage::new(path.clone(), true);

        assert_eq!(storage.get_data_path().as_path(), path.as_path());
        // Note: save_enabled might be configured differently in your actual code
    }

    #[tokio::test]
    async fn player_data_storage_get_player_data_path() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let storage = PlayerDataStorage::new(path.clone(), true);

        let uuid = Uuid::new_v4();
        let expected_path = path.join(format!("{uuid}.dat"));

        assert_eq!(storage.get_player_data_path(&uuid), expected_path);
    }

    #[tokio::test]
    async fn player_data_storage_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let storage = PlayerDataStorage::new(path, true); // Ensure saving is enabled for this test

        let uuid = Uuid::new_v4();

        // Create test data
        let mut nbt = NbtCompound::new();
        nbt.put_string("TestKey", "TestValue".to_string());
        nbt.put_int("TestInt", 42);

        // Save the data
        save(&storage, uuid, nbt).unwrap();

        // Load the data
        let (load_success, loaded_nbt) = storage.load_player_data(&uuid).unwrap();

        assert!(load_success);
        assert_eq!(loaded_nbt.get_string("TestKey").unwrap(), "TestValue");
        assert_eq!(loaded_nbt.get_int("TestInt").unwrap(), 42);
    }

    #[tokio::test]
    async fn player_data_storage_load_nonexistent() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let storage = PlayerDataStorage::new(path, true); // Ensure saving is enabled for this test

        let uuid = Uuid::new_v4();

        // Try to load non-existent data
        let (load_success, empty_nbt) = storage.load_player_data(&uuid).unwrap();

        assert!(!load_success);
        assert_eq!(empty_nbt.child_tags.len(), 0);
    }

    #[tokio::test]
    async fn player_data_storage_disabled() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let storage = PlayerDataStorage::new(path, false);

        let uuid = Uuid::new_v4();
        let mut nbt = NbtCompound::new();
        nbt.put_string("TestKey", "TestValue".to_string());

        // Save should succeed but do nothing
        let save_result = save(&storage, uuid, nbt);
        assert!(save_result.is_ok());

        // Load should return empty data
        let (load_success, empty_nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(!load_success);
        assert_eq!(empty_nbt.child_tags.len(), 0);
    }

    #[tokio::test]
    async fn server_player_data_new() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        let save_interval = Duration::from_mins(5);

        let player_data = ServerPlayerData::new(path, save_interval, true);

        assert_eq!(player_data.save_interval, save_interval);
        assert!(
            Instant::now().duration_since(player_data.last_save.load()) < Duration::from_secs(1)
        );
    }

    #[tokio::test]
    async fn player_data_file_structure() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let uuid = Uuid::new_v4();
        let storage = PlayerDataStorage::new(path, true);

        // Create and save player data
        let mut nbt = NbtCompound::new();
        nbt.put_string("name", "TestPlayer".to_string());
        nbt.put_int("level", 42);
        save(&storage, uuid, nbt).unwrap();

        // Verify the file exists
        let player_data_path = storage.get_player_data_path(&uuid);
        assert!(player_data_path.exists());

        // Load it again and verify content
        let (success, loaded_data) = storage.load_player_data(&uuid).unwrap();
        assert!(success);
        assert_eq!(loaded_data.get_string("name").unwrap(), "TestPlayer");
        assert_eq!(loaded_data.get_int("level").unwrap(), 42);
    }
}
