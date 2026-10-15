use crate::entity::player::Player;
use crate::entity::player::advancement::{AdvancementDataError, PlayerAdvancement};
use pumpkin_data::Advancement;
use pumpkin_util::identifier::Identifier;
use std::fs::create_dir_all;
use std::io;
use std::path::PathBuf;
use std::slice;
use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

/// Manages player advancements, including data creation and saving.
pub struct AdvancementManager {
    pub advancement_path: PathBuf,
    pub save_enabled: bool,
    save_gate: Arc<tokio::sync::Mutex<()>>,
}

/// Share the durable replacement path with standalone advancement saves.
pub(crate) fn write_advancement_snapshots(snapshots: Vec<(PathBuf, String)>) -> io::Result<()> {
    for (path, json) in snapshots {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        pumpkin_world::nbt_io::write_atomic(&path, json.as_bytes())?;
    }
    Ok(())
}

impl AdvancementManager {
    /// Creates a new instance of `AdvancementManager` using the player data path.
    pub fn new(player_data_path: impl Into<PathBuf>, save_enabled: bool) -> Self {
        let path = player_data_path.into().join("advancements");
        if !path.exists()
            && let Err(e) = create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }
        Self {
            advancement_path: path,
            save_enabled,
            save_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Retrieves the list of all available advancements in the game.
    #[must_use]
    #[inline]
    pub fn get_advancements(&self) -> Vec<Identifier> {
        Advancement::get_identifier_list().to_vec()
    }

    /// Creates and returns a new instance of `PlayerAdvancement` with the configured path.
    #[inline]
    #[must_use]
    pub fn new_player_advancement(self: Arc<Self>, owner: Uuid) -> PlayerAdvancement {
        PlayerAdvancement::new(self, owner)
    }

    /// Finish any previous session's blocking save before loading its progress.
    pub(crate) async fn wait_for_pending_saves(&self) {
        drop(self.save_gate.lock().await);
    }

    /// Saves the advancements of all provided players.
    pub async fn save_all_players(
        &self,
        players: &[Arc<Player>],
    ) -> Result<(), AdvancementDataError> {
        if !self.save_enabled {
            return Ok(());
        }
        // Order capture as well as replacement, so an older save-all snapshot
        // cannot land after a departing player's final advancement snapshot.
        let save_guard = self.save_gate.clone().lock_owned().await;
        let mut to_write = Vec::with_capacity(players.len());
        for player in players {
            let guard = player
                .advancements
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let json = serde_json::to_string_pretty(&*guard).map_err(AdvancementDataError::Json)?;
            to_write.push((guard.path.clone(), json));
        }

        tokio::task::spawn_blocking(move || {
            // Retain ordering until IO finishes even if the awaiting task stops.
            let _save_guard = save_guard;
            write_advancement_snapshots(to_write)
        })
        .await
        .map_err(|error| AdvancementDataError::Io(io::Error::other(error)))?
        .map_err(AdvancementDataError::Io)
    }

    /// Saves the advancements of a specific player.
    pub async fn save_player(&self, player: &Arc<Player>) -> Result<(), AdvancementDataError> {
        self.save_all_players(slice::from_ref(player)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn advancement_manager_new() {
        let path = PathBuf::from("test_data");
        let manager = AdvancementManager::new(path, true);
        assert_eq!(
            manager.advancement_path,
            PathBuf::from("test_data/advancements")
        );
    }

    #[test]
    fn get_advancement_path() {
        let path = PathBuf::from("world/playerdata");
        let manager = AdvancementManager::new(path, true);
        let advancement_path = manager.advancement_path;
        assert!(advancement_path.ends_with("advancements"));
    }

    #[tokio::test]
    async fn atomic_advancement_batch_replaces_json_and_loads_progress() {
        let directory = tempfile::tempdir().unwrap();
        let manager = Arc::new(AdvancementManager::new(directory.path(), true));
        let uuid = Uuid::new_v4();
        let mut progress = PlayerAdvancement::new(manager.clone(), uuid);
        progress.save().await.unwrap();
        let previous = std::fs::read(&progress.path).unwrap();
        let advancement = Advancement::STORY_ROOT;
        assert!(
            progress
                .progress
                .get_mut_or_start_progress(advancement)
                .grant_progress("crafting_table")
        );
        let path = progress.path.clone();
        let json = serde_json::to_string_pretty(&progress).unwrap();
        tokio::task::spawn_blocking(move || write_advancement_snapshots(vec![(path, json)]))
            .await
            .unwrap()
            .unwrap();

        let saved = std::fs::read(&progress.path).unwrap();
        assert_ne!(saved, previous);
        assert!(
            serde_json::from_slice::<serde_json::Value>(&saved)
                .unwrap()
                .is_object()
        );
        let mut loaded = PlayerAdvancement::new(manager.clone(), uuid);
        loaded.load().unwrap();
        assert!(
            loaded
                .progress
                .get_mut_or_start_progress(advancement)
                .is_done()
        );
        assert_eq!(
            std::fs::read_dir(&manager.advancement_path)
                .unwrap()
                .count(),
            1
        );
    }

    #[cfg(unix)]
    struct FileSizeLimit {
        previous: libc::rlimit,
        previous_signal: libc::sighandler_t,
    }

    #[cfg(unix)]
    impl FileSizeLimit {
        fn new(bytes: libc::rlim_t) -> Self {
            let mut previous = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            // Called only in the isolated child test. Other tests never inherit
            // a changed process-wide file-size limit or signal handler.
            unsafe {
                assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut previous), 0);
                let previous_signal = libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                assert_ne!(previous_signal, libc::SIG_ERR);
                let limited = libc::rlimit {
                    rlim_cur: bytes.min(previous.rlim_cur),
                    rlim_max: previous.rlim_max,
                };
                assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limited), 0);
                Self {
                    previous,
                    previous_signal,
                }
            }
        }
    }

    #[cfg(unix)]
    impl Drop for FileSizeLimit {
        fn drop(&mut self) {
            // Restore the child's limits even when an assertion unwinds.
            unsafe {
                let _ = libc::setrlimit(libc::RLIMIT_FSIZE, &self.previous);
                libc::signal(libc::SIGXFSZ, self.previous_signal);
            }
        }
    }

    /// A real partial-write failure must leave the previous complete JSON in
    /// place. Process isolation keeps RLIMIT_FSIZE away from parallel tests.
    #[cfg(unix)]
    #[test]
    fn advancement_atomic_write_failure_keeps_previous_json() {
        const CHILD_DIRECTORY: &str = "FERRALOOM_ADVANCEMENT_ATOMIC_TEST_DIR";
        if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let directory = PathBuf::from(directory);
                    let manager = Arc::new(AdvancementManager::new(directory.clone(), true));
                    let mut progress = PlayerAdvancement::new(manager.clone(), Uuid::new_v4());
                    progress
                        .progress
                        .get_mut_or_start_progress(Advancement::STORY_ROOT)
                        .grant_progress("crafting_table");
                    progress.save().await.unwrap();
                    let previous = std::fs::read(&progress.path).unwrap();
                    for identifier in Advancement::get_identifier_list() {
                        let advancement =
                            Advancement::from_minecraft_name(&identifier.to_string()).unwrap();
                        for criterion in progress
                            .progress
                            .get_mut_or_start_progress(advancement)
                            .criteria
                            .values_mut()
                        {
                            criterion.grant();
                        }
                    }
                    assert!(serde_json::to_vec_pretty(&progress).unwrap().len() > 4096);
                    let limit = FileSizeLimit::new(1024);
                    let result = progress.save().await;
                    drop(limit);
                    assert!(matches!(result, Err(AdvancementDataError::Io(_))));
                    assert_eq!(std::fs::read(&progress.path).unwrap(), previous);
                    assert!(
                        serde_json::from_slice::<serde_json::Value>(&previous)
                            .unwrap()
                            .is_object()
                    );
                    assert_eq!(
                        std::fs::read_dir(&manager.advancement_path)
                            .unwrap()
                            .count(),
                        1
                    );
                    std::fs::write(directory.join("child-finished"), b"passed\n").unwrap();
                });
            return;
        }

        // The parent owns cleanup even if the child panics or its watchdog fires.
        let directory = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "data::advancement_data::tests::advancement_atomic_write_failure_keeps_previous_json", "--nocapture", "--test-threads=1"])
            .env(CHILD_DIRECTORY, directory.path())
            .stdin(std::process::Stdio::null())
            .spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert!(
                        status.success(),
                        "advancement write-failure child failed: {status}"
                    );
                    assert_eq!(
                        std::fs::read(directory.path().join("child-finished")).unwrap(),
                        b"passed\n",
                        "write-failure child must execute its assertions"
                    );
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("failed to wait for advancement write-failure child: {error}");
                }
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("advancement write-failure child exceeded 20 seconds");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
