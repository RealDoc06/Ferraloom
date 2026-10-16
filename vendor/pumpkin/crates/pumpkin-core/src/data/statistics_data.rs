use crate::entity::player::{Player, statistics::StatisticsDataError};
use std::{fs, io, path::PathBuf, slice, sync::Arc};
use tracing::error;
use uuid::Uuid;

/// Canonical world/players/stats persistence. Capture and physical replacement
/// share one gate, including after cancellation of the awaiting save future.
pub struct StatisticsManager {
    pub statistics_path: PathBuf,
    pub save_enabled: bool,
    save_gate: Arc<tokio::sync::Mutex<()>>,
}

impl StatisticsManager {
    #[must_use]
    pub fn new(player_data_path: impl Into<PathBuf>, save_enabled: bool) -> Self {
        let statistics_path = player_data_path.into().join("stats");
        if save_enabled && let Err(error) = fs::create_dir_all(&statistics_path) {
            error!(
                "Failed to create statistics directory {}: {error}",
                statistics_path.display()
            );
        }
        Self {
            statistics_path,
            save_enabled,
            save_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    #[must_use]
    pub fn player_path(&self, uuid: Uuid) -> PathBuf {
        self.statistics_path.join(format!("{uuid}.json"))
    }

    pub(crate) async fn wait_for_pending_saves(&self) {
        drop(self.save_gate.lock().await);
    }

    /// Admission waits for pending IO before calling this, and publishes only
    /// after all three persistence families have loaded successfully.
    pub(crate) fn load_player(&self, player: &Player) -> Result<(), StatisticsDataError> {
        player
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .load(&self.player_path(player.gameprofile.id), self.save_enabled)
    }

    pub async fn save_all_players(
        &self,
        players: &[Arc<Player>],
    ) -> Result<(), StatisticsDataError> {
        if !self.save_enabled {
            return Ok(());
        }
        let save_guard = self.save_gate.clone().lock_owned().await;
        let mut snapshots = Vec::with_capacity(players.len());
        let mut first_error = None;
        for player in players {
            let result = player
                .stats
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .snapshot();
            match result {
                Ok(Some(bytes)) => snapshots.push((self.player_path(player.gameprofile.id), bytes)),
                Ok(None) => {}
                Err(error) => {
                    error!(
                        "Failed to capture statistics for {}: {error}",
                        player.gameprofile.id
                    );
                    first_error.get_or_insert(error);
                }
            }
        }
        let written = tokio::task::spawn_blocking(move || {
            let _save_guard = save_guard;
            write_statistics_snapshots(snapshots)
        })
        .await
        .map_err(|error| StatisticsDataError::Io(io::Error::other(error)))?;
        if let Err(error) = written {
            first_error.get_or_insert(StatisticsDataError::Io(error));
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn save_player(&self, player: &Arc<Player>) -> Result<(), StatisticsDataError> {
        self.save_all_players(slice::from_ref(player)).await
    }
}

fn write_statistics_snapshots(snapshots: Vec<(PathBuf, Vec<u8>)>) -> io::Result<()> {
    let mut first_error = None;
    for (path, bytes) in snapshots {
        // No directory recreation here: a vanished parent is a save failure.
        if let Err(error) = pumpkin_world::nbt_io::write_atomic(&path, &bytes) {
            error!("Failed to save statistics {}: {error}", path.display());
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests;
