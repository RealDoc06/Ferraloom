use super::Server;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tracing::error;

#[derive(Default)]
pub(super) struct SaveCoordinator {
    lock: Arc<AsyncMutex<()>>,
    elapsed: AtomicU64,
    autosave_error: Mutex<Option<String>>,
}

impl SaveCoordinator {
    pub(super) async fn lock(&self) -> OwnedMutexGuard<()> {
        self.lock.clone().lock_owned().await
    }

    /// Count server ticks, including frozen ticks. If a save is still running,
    /// coalesce this interval instead of creating tasks waiting for the lock.
    fn tick(&self, interval: u64) -> Option<OwnedMutexGuard<()>> {
        if interval == 0 {
            return None;
        }
        let previous = self
            .elapsed
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |elapsed| {
                Some(if elapsed >= interval - 1 {
                    0
                } else {
                    elapsed + 1
                })
            })
            .expect("autosave counter update always succeeds");
        if previous < interval - 1 {
            return None;
        }
        self.lock.clone().try_lock_owned().ok()
    }
}

impl Server {
    pub async fn save_all(&self, flush: bool) -> Result<(), String> {
        let _guard = self.saving.lock().await;
        self.save_all_locked(flush, false).await
    }

    pub(super) fn schedule_autosave(self: &Arc<Self>) {
        if self.tasks.is_closed() {
            return;
        }
        let Some(guard) = self.saving.tick(self.advanced_config.world.autosave_ticks) else {
            return;
        };
        let server = self.clone();
        self.spawn_task(async move {
            let _guard = guard;
            if let Err(error) = server.save_all_locked(true, true).await {
                error!("Server autosave failed: {error}");
                server
                    .saving
                    .autosave_error
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_or_insert(error);
            }
        });
    }

    pub(super) fn report_autosave_failure(&self) {
        if let Some(error) = self
            .saving
            .autosave_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            error!("A server autosave failed before shutdown: {error}");
            crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
        }
    }

    /// The caller owns the server save lock through snapshots and physical flush.
    /// Manual saves override save-off; autosaves always write global state but
    /// skip both snapshotting and flushing worlds with saving disabled.
    async fn save_all_locked(&self, flush: bool, automatic: bool) -> Result<(), String> {
        let worlds: Vec<_> = self
            .worlds
            .load()
            .iter()
            .filter(|world| !automatic || world.level.save_enabled.load(Ordering::Relaxed))
            .cloned()
            .collect();
        self.management_hub.broadcast_server_saving();
        let mut first_error = None;
        let mut record_error = |result: Result<(), String>| {
            if let Err(error) = result {
                error!("{error}");
                first_error.get_or_insert(error);
            }
        };
        record_error(
            self.save_world_info()
                .map_err(|error| format!("Failed to save world info: {error}")),
        );
        record_error(
            self.player_data_storage
                .save_all_players(self)
                .map_err(|error| format!("Failed to save player data: {error}")),
        );
        record_error(
            self.advancement_manager
                .save_all_players(&self.get_all_players())
                .await
                .map_err(|error| format!("Failed to save player advancements: {error}")),
        );
        record_error(
            self.save_scoreboard()
                .map_err(|error| format!("Failed to save scoreboard: {error}")),
        );
        record_error(
            self.save_command_storage()
                .map_err(|error| format!("Failed to save command storage: {error}")),
        );
        for world in &worlds {
            record_error(world.save().await.map_err(|error| {
                format!("Failed to save {}: {error}", world.dimension.minecraft_name)
            }));
        }
        if flush {
            for world in &worlds {
                record_error(world.level.save_and_flush().await.map_err(|error| {
                    format!(
                        "Failed to flush {}: {error}",
                        world.dimension.minecraft_name
                    )
                }));
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        self.management_hub.broadcast_server_saved();
        Ok(())
    }
}

#[cfg(test)]
#[path = "saving_tests.rs"]
mod tests;
