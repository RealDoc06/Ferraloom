use crate::{
    entity::{NBTStorage, player::Player},
    server::Server,
};
use crossbeam::atomic::AtomicCell;
use pumpkin_inventory::screen_handler::ScreenHandler;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_world::data::player_data::{PlayerDataError, PlayerDataStorage};
use std::sync::Arc;
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
}

impl ServerPlayerData {
    /// Creates a new `ServerPlayerData` with specified configuration.
    pub fn new(data_path: impl Into<PathBuf>, save_interval: Duration, enabled: bool) -> Self {
        Self {
            storage: Arc::new(PlayerDataStorage::new(data_path, enabled)),
            save_interval,
            last_save: AtomicCell::new(Instant::now()),
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

        let mut nbt = NbtCompound::new();
        player.write_nbt(&mut nbt);

        self.storage.save_player_data(&player.gameprofile.id, nbt)?;
        Ok(())
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
            let mut snapshots = Vec::new();
            for world in server.worlds.load().iter() {
                for player in world.players.load().iter() {
                    let mut nbt = NbtCompound::new();
                    player.write_nbt(&mut nbt);
                    snapshots.push((player.gameprofile.id, nbt));
                }
            }

            if snapshots.is_empty() {
                return;
            }

            let storage = self.storage.clone();
            rayon::spawn(move || {
                for (uuid, nbt) in snapshots {
                    if let Err(e) = storage.save_player_data(&uuid, nbt) {
                        error!("Failed to save player data for {uuid}: {e}");
                    }
                }
                debug!("Periodic player data save completed");
            });
        }
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

        let uuid = player.gameprofile.id;
        let mut nbt = NbtCompound::new();
        player.write_nbt(&mut nbt);

        self.storage.save_player_data(&uuid, nbt)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::data::player_server::ServerPlayerData;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_world::data::player_data::{PlayerDataError, PlayerDataStorage};
    use std::time::Duration;
    use std::time::Instant;
    use tempfile::tempdir;
    use uuid::Uuid;

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
        player_data
            .storage
            .save_player_data(&uuid, previous.clone())
            .unwrap();
        player_data
            .storage
            .save_player_data(&uuid, NbtCompound::new())
            .unwrap();
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
        storage.save_player_data(&uuid, nbt).unwrap();

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
        let save_result = storage.save_player_data(&uuid, nbt);
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
        storage.save_player_data(&uuid, nbt).unwrap();

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
