use crate::{
    entity::{NBTStorage, player::Player},
    server::Server,
};
use crossbeam::atomic::AtomicCell;
use pumpkin_inventory::screen_handler::ScreenHandler;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_world::data::player_data::{PlayerDataError, PlayerDataStorage, PlayerSnapshot};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard};
use tracing::{debug, error};

/// Helper for managing player data in the server context.
///
/// This struct provides server-wide access to the `PlayerDataStorage` and
/// convenience methods for player handling.
pub struct ServerPlayerData {
    storage: Arc<PlayerDataStorage>,
    save_interval: Duration,
    last_save: AtomicCell<Instant>,
    sessions: Mutex<HashMap<uuid::Uuid, Weak<PlayerSessionSlot>>>,
}

#[derive(Default)]
struct PlayerSessionSlot {
    gate: Arc<AsyncMutex<()>>,
    player: Mutex<Weak<Player>>,
    leaving: AtomicBool,
    changed: Notify,
}

/// Exclusive ownership from admission through the final player, advancement and
/// statistics saves. Move this lease from the Player into the disconnect future.
pub(crate) struct PlayerSession {
    slot: Arc<PlayerSessionSlot>,
    _guard: OwnedMutexGuard<()>,
}

impl PlayerSession {
    pub(crate) fn publish(&self, player: &Arc<Player>) {
        *self
            .slot
            .player
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::downgrade(player);
        self.slot.changed.notify_waiters();
    }

    pub(crate) fn begin_leave(&self) {
        self.slot.leaving.store(true, Ordering::Release);
    }
}

impl Drop for PlayerSession {
    fn drop(&mut self) {
        *self
            .slot
            .player
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Weak::new();
        self.slot.leaving.store(false, Ordering::Release);
        self.slot.changed.notify_waiters();
    }
}

impl ServerPlayerData {
    /// Creates a new `ServerPlayerData` with specified configuration.
    pub fn new(data_path: impl Into<PathBuf>, save_interval: Duration, enabled: bool) -> Self {
        Self {
            storage: Arc::new(PlayerDataStorage::new(data_path, enabled)),
            save_interval,
            last_save: AtomicCell::new(Instant::now()),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Reserve before loading any player persistence family. A replaced owner is closed,
    /// then its final save must release the lease before the new load can start.
    pub(crate) async fn reserve_player_session(&self, uuid: uuid::Uuid) -> PlayerSession {
        let slot = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .get(&uuid)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| {
                    // Owners and waiters retain their slots. Only expired UUIDs
                    // can be discarded without allowing two admissions at once.
                    sessions.retain(|_, slot| slot.strong_count() != 0);
                    let slot = Arc::new(PlayerSessionSlot::default());
                    sessions.insert(uuid, Arc::downgrade(&slot));
                    slot
                })
        };
        let lock = slot.gate.clone().lock_owned();
        tokio::pin!(lock);
        loop {
            let changed = slot.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            // Do not retain the old Player across an await: its destructor may
            // itself be what releases an unpublished or abandoned session.
            {
                let player = slot
                    .player
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .upgrade();
                if let Some(player) = player
                    && !player.client.closed()
                {
                    player.kick(crate::net::DisconnectReason::LoggedInOtherLocation, &pumpkin_util::text::TextComponent::translate_cross(
                        pumpkin_data::translation::java::MULTIPLAYER_DISCONNECT_DUPLICATE_LOGIN,
                        pumpkin_data::translation::bedrock::DISCONNECTIONSCREEN_LOGGEDINOTHERLOCATION,
                        [],
                    ));
                }
            }
            tokio::select! {
                guard = &mut lock => return PlayerSession { slot: slot.clone(), _guard: guard },
                () = &mut changed => {},
            }
        }
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
        let leaving = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(uuid)
            .and_then(Weak::upgrade)
            .is_some_and(|slot| slot.leaving.load(Ordering::Acquire));
        if leaving {
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
pub(crate) mod test {
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

    pub struct PlayerFixture {
        pub(crate) directory: tempfile::TempDir,
        pub(crate) server: Arc<Server>,
        pub(crate) player: Arc<Player>,
        pub(crate) client: Arc<ClientPlatform>,
        pub(crate) _remote: tokio::net::TcpStream,
    }

    pub async fn java_client(
        server: &Arc<Server>,
        profile: &GameProfile,
    ) -> (Arc<ClientPlatform>, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (remote, accepted) =
            tokio::join!(tokio::net::TcpStream::connect(address), listener.accept());
        let remote = remote.unwrap();
        let (stream, address) = accepted.unwrap();
        let pending = PendingConnection::new(
            stream,
            address,
            0,
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(server),
        );
        pending.connection_state.store(ConnectionState::Play);
        let mut java = JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
        java.start_outgoing_packet_task();
        (Arc::new(ClientPlatform::Java(java)), remote)
    }

    pub async fn player_fixture() -> PlayerFixture {
        player_fixture_with_hardcore(false).await
    }

    pub async fn player_fixture_with_hardcore(hardcore: bool) -> PlayerFixture {
        let directory = tempdir().unwrap();
        let basic = BasicConfiguration {
            default_level_name: directory.path().to_string_lossy().into_owned(),
            seed: Seed(0),
            hardcore,
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
        let (client, remote) = java_client(&server, &profile).await;
        let world = server.worlds.load().first().cloned().unwrap();
        // Session-replacement tests exercise real admission, including spawn reads,
        // with existing Full terrain instead of depending on world generation time.
        for x in -1..=1 {
            for z in -1..=1 {
                world.level.loaded_chunks.insert(
                    pumpkin_util::math::vector2::Vector2::new(x, z),
                    pumpkin_world::chunk::ChunkData::empty_sync(x, z),
                );
            }
        }

        let player = Arc::new(Player::new(
            client.clone(),
            profile,
            PlayerConfig::default(),
            &world,
            pumpkin_util::GameMode::Survival,
        ));
        // This fixture bypasses admission, which loads progress before publication.
        player.advancements.lock().unwrap().load().unwrap();
        server.statistics_manager.load_player(&player).unwrap();
        player
            .screen_handler_sync_handler
            .store_player(player.clone());
        let session = server
            .player_data_storage
            .reserve_player_session(uuid)
            .await;
        session.publish(&player);
        *player.session.lock().unwrap() = Some(session);
        // Exercise the real world registry without Server::add_player's user-cache IO.
        world.add_player(&player).unwrap();
        PlayerFixture {
            directory,
            server,
            player,
            client,
            _remote: remote,
        }
    }

    /// Stop terrain workers while leaving world callback admission open for packet tests.
    pub async fn stop_chunk_workers(fixture: &PlayerFixture) {
        for world in fixture.server.worlds.load().iter() {
            world
                .level
                .shut_down_chunk_system
                .store(true, Ordering::Relaxed);
            world.level.level_channel.notify();
            let handles = std::mem::take(&mut *world.level.thread_tracker.lock().unwrap());
            tokio::time::timeout(
                Duration::from_secs(10),
                tokio::task::spawn_blocking(move || {
                    for handle in handles {
                        handle.join().unwrap();
                    }
                }),
            )
            .await
            .unwrap()
            .unwrap();
            world.level.chunk_system_tasks.close();
            tokio::time::timeout(
                Duration::from_secs(10),
                world.level.chunk_system_tasks.wait(),
            )
            .await
            .unwrap();
            assert!(!world.level.cancel_token.is_cancelled());
        }
    }

    pub async fn close_fixture(fixture: &PlayerFixture) {
        if fixture
            .server
            .get_player_by_uuid(fixture.player.gameprofile.id)
            .is_some_and(|player| Arc::ptr_eq(&player, &fixture.player))
        {
            crate::finish_player_session(&fixture.server, &fixture.player)
                .await
                .unwrap();
        }
        let java = fixture.client.java().unwrap();
        java.close();
        tokio::time::timeout(Duration::from_secs(5), java.await_tasks())
            .await
            .unwrap();
        fixture.server.shutdown().await;
    }

    struct ReloginDuringLeave {
        observed: AtomicBool,
        pause: AtomicBool,
        resume: tokio::sync::Notify,
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
                event
                    .player
                    .advancements
                    .lock()
                    .unwrap()
                    .progress
                    .get_mut_or_start_progress(pumpkin_data::Advancement::STORY_ROOT)
                    .grant_progress("crafting_table");
                self.observed.store(true, Ordering::Relaxed);
                if self.pause.swap(false, Ordering::Relaxed) {
                    self.resume.notified().await;
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
            pause: AtomicBool::new(false),
            resume: tokio::sync::Notify::new(),
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
        assert!(loaded.get("Statistics").is_none());
        fixture
            .server
            .statistics_manager
            .load_player(&rejoined)
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture.server.statistics_manager.player_path(uuid)).unwrap(),
        )
        .unwrap();
        assert_eq!(json["stats"]["minecraft:custom"]["minecraft:leave_game"], 1);
        assert_eq!(rejoined.experience_level.load(Ordering::Relaxed), 9);
        assert_eq!(
            rejoined.stats.lock().unwrap().get(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::LeaveGame as i32
            ),
            1
        );
        {
            let mut advancements = rejoined.advancements.lock().unwrap();
            advancements.load().unwrap();
            assert!(
                advancements
                    .progress
                    .get_mut_or_start_progress(pumpkin_data::Advancement::STORY_ROOT)
                    .is_done()
            );
        };
        let rejoined = Arc::new(rejoined);
        rejoined
            .screen_handler_sync_handler
            .store_player(rejoined.clone());
        let session = data.reserve_player_session(uuid).await;
        session.publish(&rejoined);
        *rejoined.session.lock().unwrap() = Some(session);
        world.add_player(&rejoined).unwrap();

        // A failed final write also releases admission. Repair the deliberately
        // blocked temp file and verify the existing save can be loaded again.
        let path = data.storage.get_player_data_path(&uuid);
        let previous = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let advancement_path = rejoined.advancements.lock().unwrap().path.clone();
        std::fs::remove_file(&advancement_path).unwrap();
        assert!(
            crate::finish_player_session(&fixture.server, &rejoined)
                .await
                .is_err()
        );
        assert!(
            advancement_path.is_file(),
            "advancements must still save after player.dat fails"
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
            pause: AtomicBool::new(true),
            resume: tokio::sync::Notify::new(),
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
        // Transport drain precedes the leave event. Observe the real paused
        // callback before cancelling, rather than assuming one poll reaches it.
        tokio::time::timeout(Duration::from_secs(5), async {
            while !handler.observed.load(Ordering::Relaxed) {
                tokio::task::yield_now().await;
                assert!(futures::poll!(&mut leave).is_pending());
            }
        })
        .await
        .expect("leave callback must reach its cancellation barrier");
        assert!(handler.observed.load(Ordering::Relaxed));
        drop(leave);
        assert_eq!(
            data.load_data(&uuid).unwrap().unwrap().get_int("XpLevel"),
            Some(1)
        );
        close_fixture(&fixture).await;
    }

    /// Common admission writes usercache through a cwd-based legacy API. Run
    /// that one test in a separate process, keeping the test runner's cwd intact.
    fn isolated_admission_test(name: &str) -> bool {
        const CHILD: &str = "FERRALOOM_SESSION_ADMISSION_TEST";
        if std::env::var(CHILD).is_ok_and(|value| value == name) {
            return false;
        }
        let directory = tempdir().unwrap();
        let output = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(CHILD, name)
            .current_dir(directory.path())
            .stdout(output.as_file().try_clone().unwrap())
            .stderr(output.as_file().try_clone().unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "admission test timed out: {}",
                    std::fs::read_to_string(output.path()).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let status = child.wait().unwrap();
        let output = std::fs::read_to_string(output.path()).unwrap();
        assert!(
            status.success() && output.contains("1 passed"),
            "isolated admission test failed: {output}"
        );
        true
    }

    #[tokio::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Keep paused final saves, duplicate admission and cancellation in one session scenario."
    )]
    async fn server_admission_waits_for_final_data_and_cleans_exact_duplicate() {
        if isolated_admission_test(
            "data::player_server::test::server_admission_waits_for_final_data_and_cleans_exact_duplicate",
        ) {
            return;
        }
        let fixture = player_fixture().await;
        let data = &fixture.server.player_data_storage;
        let uuid = fixture.player.gameprofile.id;
        fixture.player.experience_level.store(1, Ordering::Relaxed);
        data.extract_data_and_save_player(&fixture.player).unwrap();
        let path = data.storage.get_player_data_path(&uuid);
        let before = std::fs::read(&path).unwrap();

        // A legacy caller can bypass common admission. Its cleanup must remove
        // that exact object without releasing or persisting the admitted owner.
        let world = fixture.player.world();
        let duplicate = Arc::new(Player::new(
            fixture.client.clone(),
            fixture.player.gameprofile.clone(),
            PlayerConfig::default(),
            &world,
            pumpkin_util::GameMode::Survival,
        ));
        world.add_player(&duplicate).unwrap();
        crate::finish_player_session(&fixture.server, &duplicate)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &fixture.server.get_player_by_uuid(uuid).unwrap(),
            &fixture.player
        ));
        assert_eq!(world.players.load().len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let handler = Arc::new(ReloginDuringLeave {
            observed: AtomicBool::new(false),
            pause: AtomicBool::new(true),
            resume: tokio::sync::Notify::new(),
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
        let profile = fixture.player.gameprofile.clone();
        let (cancelled_client, _cancelled_remote) = java_client(&fixture.server, &profile).await;
        let mut cancelled = Box::pin(fixture.server.add_player(
            cancelled_client.clone(),
            profile.clone(),
            None,
        ));
        assert!(futures::poll!(&mut cancelled).is_pending());
        cancelled_client.java().unwrap().close();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), cancelled)
                .await
                .unwrap()
                .is_none()
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            cancelled_client.java().unwrap().await_tasks(),
        )
        .await
        .unwrap();
        let (client, _remote) = java_client(&fixture.server, &profile).await;
        let mut admission = Box::pin(fixture.server.add_player(
            client.clone(),
            profile.clone(),
            None,
        ));
        assert!(futures::poll!(&mut admission).is_pending());
        assert!(
            !client.closed(),
            "a pending final save must wait instead of kicking the new client"
        );
        assert!(fixture.server.get_player_by_uuid(uuid).is_none());
        handler.resume.notify_one();
        leave.await.unwrap();
        let (rejoined, _) = tokio::time::timeout(Duration::from_secs(5), admission)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rejoined.experience_level.load(Ordering::Relaxed), 9);
        assert_eq!(
            rejoined.stats.lock().unwrap().get(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::LeaveGame as i32
            ),
            1
        );
        assert!(
            rejoined
                .advancements
                .lock()
                .unwrap()
                .progress
                .get_mut_or_start_progress(pumpkin_data::Advancement::STORY_ROOT)
                .is_done()
        );
        assert_eq!(world.players.load().len(), 1);

        // Repeated cleanup of an old Player cannot remove or unlock its successor.
        crate::finish_player_session(&fixture.server, &fixture.player)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &fixture.server.get_player_by_uuid(uuid).unwrap(),
            &rejoined
        ));
        let (next_client, _next_remote) = java_client(&fixture.server, &profile).await;
        let mut next = Box::pin(
            fixture
                .server
                .add_player(next_client.clone(), profile, None),
        );
        assert!(futures::poll!(&mut next).is_pending());
        assert!(
            client.closed(),
            "an active duplicate is closed before waiting for its final saves"
        );
        crate::finish_player_session(&fixture.server, &fixture.player)
            .await
            .unwrap();
        assert!(futures::poll!(&mut next).is_pending());
        drop(next);
        next_client.java().unwrap().close();
        tokio::time::timeout(
            Duration::from_secs(5),
            next_client.java().unwrap().await_tasks(),
        )
        .await
        .unwrap();
        crate::finish_player_session(&fixture.server, &rejoined)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), client.java().unwrap().await_tasks())
            .await
            .unwrap();
        close_fixture(&fixture).await;
    }

    #[tokio::test]
    async fn server_pending_session_reservations_release_on_timeout_and_cancellation() {
        let directory = tempdir().unwrap();
        let data = ServerPlayerData::new(directory.path(), Duration::from_secs(30), true);
        let uuid = Uuid::from_u128(103);
        let first = data.reserve_player_session(uuid).await;
        let mut cancelled = Box::pin(data.reserve_player_session(uuid));
        assert!(futures::poll!(&mut cancelled).is_pending());
        let expired_uuid = Uuid::from_u128(104);
        drop(data.reserve_player_session(expired_uuid).await);
        let unrelated = data.reserve_player_session(Uuid::from_u128(105)).await;
        assert!(!data.sessions.lock().unwrap().contains_key(&expired_uuid));
        assert!(futures::poll!(&mut cancelled).is_pending());
        drop(unrelated);
        drop(cancelled);
        assert!(
            tokio::time::timeout(Duration::from_millis(1), data.reserve_player_session(uuid))
                .await
                .is_err()
        );
        drop(first);
        let next = tokio::time::timeout(Duration::from_secs(1), data.reserve_player_session(uuid))
            .await
            .unwrap();
        drop(next);
        assert!(data.load_data(&uuid).unwrap().is_none());
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
