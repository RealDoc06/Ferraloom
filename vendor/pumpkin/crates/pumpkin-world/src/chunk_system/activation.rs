use super::ChunkLoading;
use crate::level::{Level, SyncChunk};
use pumpkin_data::chunk::ChunkStatus;
use pumpkin_util::math::vector2::Vector2;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Mutex, Weak, atomic::Ordering};

#[derive(Default)]
struct ActivationState {
    owners: FxHashMap<Vector2<i32>, usize>,
    pending: FxHashSet<Vector2<i32>>,
}

#[derive(Default)]
pub struct ChunkActivation {
    state: Mutex<ActivationState>,
    consumer: Mutex<()>,
}

/// One simulation, pending-delivery, or direct-wait owner's activation requests.
///
/// Completed chunks hold no activation ticket; dropping an owner cancels its
/// remaining requests without affecting other owners of the same position.
pub struct ChunkActivationDemand {
    level: Weak<Level>,
    requested: FxHashSet<Vector2<i32>>,
}

impl std::fmt::Debug for ChunkActivationDemand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChunkActivationDemand")
            .field("requested", &self.requested)
            .finish_non_exhaustive()
    }
}

impl ChunkActivationDemand {
    pub fn belongs_to(&self, level: &Arc<Level>) -> bool {
        Weak::ptr_eq(&self.level, &Arc::downgrade(level))
    }

    /// Reconcile one complete desired set, publishing ticket changes once per batch.
    /// Rechecking retained positions also notices newly invalidated ready chunks.
    pub fn replace(&mut self, positions: impl IntoIterator<Item = Vector2<i32>>) {
        let requested: FxHashSet<_> = positions.into_iter().collect();
        if let Some(level) = self.level.upgrade() {
            level.replace_activation_demand(&self.requested, &requested);
        }
        self.requested = requested;
    }

    pub fn remove(&mut self, positions: impl IntoIterator<Item = Vector2<i32>>) {
        let mut requested = self.requested.clone();
        for position in positions {
            requested.remove(&position);
        }
        self.replace(requested);
    }
}

impl Drop for ChunkActivationDemand {
    fn drop(&mut self) {
        if !self.requested.is_empty() {
            self.replace([]);
        }
    }
}

impl Level {
    #[must_use]
    pub fn activation_demand(self: &Arc<Self>) -> ChunkActivationDemand {
        ChunkActivationDemand {
            level: Arc::downgrade(self),
            requested: FxHashSet::default(),
        }
    }

    fn replace_activation_demand(
        &self,
        old: &FxHashSet<Vector2<i32>>,
        new: &FxHashSet<Vector2<i32>>,
    ) {
        let mut state = self
            .chunk_activation
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for position in old.difference(new) {
            if let Some(count) = state.owners.get_mut(position) {
                *count -= 1;
                if *count == 0 {
                    state.owners.remove(position);
                    if state.pending.remove(position) {
                        removed.push(*position);
                    }
                }
            }
        }
        for position in new.difference(old) {
            *state.owners.entry(*position).or_default() += 1;
        }
        for &position in new {
            if self.ready_chunk(position).is_some() {
                if state.pending.remove(&position) {
                    removed.push(position);
                }
            } else if state.pending.insert(position) {
                added.push(position);
            }
        }
        self.update_activation_tickets(&removed, &added);
    }

    /// The caller holds activation state, so cancellation and completion cannot
    /// publish contradictory ticket transitions. No activation lock crosses await.
    fn update_activation_tickets(&self, removed: &[Vector2<i32>], added: &[Vector2<i32>]) {
        if removed.is_empty() && added.is_empty() {
            return;
        }
        let mut loading = self
            .chunk_loading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Add before removing to keep overlapping halo dependencies pinned.
        for &position in added {
            loading.add_ticket(position, ChunkLoading::FULL_CHUNK_LEVEL - 1);
        }
        for &position in removed {
            loading.remove_ticket(position, ChunkLoading::FULL_CHUNK_LEVEL - 1);
        }
        loading.send_change();
        self.chunk_activation_revision
            .fetch_add(1, Ordering::AcqRel);
        self.chunk_activation_changed.notify_waiters();
    }

    #[must_use]
    pub fn ready_chunk(&self, pos: Vector2<i32>) -> Option<SyncChunk> {
        self.loaded_chunks.get(&pos).and_then(|chunk| {
            (chunk.status == ChunkStatus::Full && chunk.is_post_processing_ready())
                .then(|| chunk.value().clone())
        })
    }

    #[must_use]
    pub fn is_chunk_ready(&self, pos: &Vector2<i32>) -> bool {
        self.ready_chunk(*pos).is_some()
    }

    /// A nonblocking pass processes only owned centers, without activating their
    /// halo members. Successful centers release tickets together after the pass.
    pub fn process_chunk_activations(&self, mut process: impl FnMut(&SyncChunk) -> bool) {
        let consumer = match self.chunk_activation.consumer.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let observed_revision = self.chunk_activation_revision.load(Ordering::Acquire);
        let mut centers: Vec<_> = self
            .chunk_activation
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .iter()
            .copied()
            .collect();
        centers.sort_unstable_by_key(|pos| (pos.x, pos.y));
        let mut completed = Vec::new();
        for pos in centers {
            let _lifecycle = self
                .chunk_lifecycle
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let full_halo = (-1..=1).all(|dx| {
                (-1..=1).all(|dz| {
                    self.loaded_chunks
                        .get(&pos.add_raw(dx, dz))
                        .is_some_and(|chunk| chunk.status == ChunkStatus::Full)
                })
            });
            if !full_halo {
                continue;
            }
            let Some(chunk) = self
                .loaded_chunks
                .get(&pos)
                .map(|entry| entry.value().clone())
            else {
                continue;
            };
            if process(&chunk) {
                completed.push((pos, chunk));
            }
        }
        {
            let mut state = self
                .chunk_activation
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut removed = Vec::new();
            for (pos, processed) in completed {
                // A cancelled/replaced cache entry must not complete a newer request.
                if self
                    .ready_chunk(pos)
                    .is_some_and(|current| Arc::ptr_eq(&current, &processed))
                    && state.pending.remove(&pos)
                {
                    removed.push(pos);
                }
            }
            self.update_activation_tickets(&removed, &[]);
        };
        drop(consumer);
        // Requests/publications can arrive while another consumer owns the pass.
        if self.chunk_activation_revision.load(Ordering::Acquire) != observed_revision {
            self.chunk_activation_changed.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkData;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::dimension::Dimension;
    use pumpkin_util::math::position::BlockPos;

    async fn level() -> (tempfile::TempDir, Arc<Level>) {
        let directory = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().into(),
            0,
            Dimension::OVERWORLD,
        );
        // Control publication explicitly; no generation timing is part of these tests.
        level.shutdown().await.unwrap();
        (directory, level)
    }

    #[tokio::test]
    async fn activation_demand_shared_unready_request_releases_only_after_last_owner() {
        let (_directory, level) = level().await;
        let position = Vector2::new(0, 0);
        let mut simulation = level.activation_demand();
        simulation.replace([position]);
        let mut sending = level.activation_demand();
        sending.replace([position]);
        assert_eq!(
            level.chunk_loading.lock().unwrap().ticket.get(&position),
            Some(&vec![ChunkLoading::FULL_CHUNK_LEVEL - 1])
        );
        drop(sending);
        assert!(
            level
                .chunk_loading
                .lock()
                .unwrap()
                .ticket
                .contains_key(&position)
        );
        drop(simulation);
        assert!(level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert!(level.chunk_loading.lock().unwrap().pos_level.is_empty());
        let state = level.chunk_activation.state.lock().unwrap();
        assert!(state.owners.is_empty());
        assert!(state.pending.is_empty());
    }

    #[tokio::test]
    async fn activation_demand_cancelled_wait_drops_its_unready_halo_ticket() {
        let (_directory, level) = level().await;
        let (started, ready) = tokio::sync::oneshot::channel();
        let waiting = {
            let level = level.clone();
            tokio::spawn(async move {
                let mut demand = level.activation_demand();
                demand.replace([Vector2::new(0, 0)]);
                started.send(()).unwrap();
                std::future::pending::<()>().await;
            })
        };
        ready.await.unwrap();
        assert!(!level.chunk_loading.lock().unwrap().ticket.is_empty());
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        assert!(level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert!(
            level
                .chunk_activation
                .state
                .lock()
                .unwrap()
                .owners
                .is_empty()
        );
    }

    #[tokio::test]
    async fn activation_demand_completion_releases_tickets_and_same_owner_rearms_new_marks() {
        let (_directory, level) = level().await;
        let center = Vector2::new(0, 0);
        for x in -1..=1 {
            for z in -1..=1 {
                level
                    .loaded_chunks
                    .insert(Vector2::new(x, z), ChunkData::empty_sync(x, z));
            }
        }
        let mut demand = level.activation_demand();
        demand.replace([center]);
        level.process_chunk_activations(|chunk| chunk.process_post_processing(|_| true));
        assert!(level.is_chunk_ready(&center));
        assert!(
            !level.is_chunk_ready(&Vector2::new(1, 0)),
            "halo never becomes another owner"
        );
        assert!(level.chunk_loading.lock().unwrap().ticket.is_empty());
        let chunk = level.ready_chunk(center).unwrap();
        chunk.mark_pos_for_postprocessing(&BlockPos::new(1, 100, 1));
        demand.replace([center]);
        assert!(
            level
                .chunk_loading
                .lock()
                .unwrap()
                .ticket
                .contains_key(&center)
        );
        level.process_chunk_activations(|chunk| chunk.process_post_processing(|_| true));
        assert!(level.chunk_loading.lock().unwrap().ticket.is_empty());
        drop(demand);
        assert!(
            level
                .chunk_activation
                .state
                .lock()
                .unwrap()
                .owners
                .is_empty()
        );
    }

    #[tokio::test]
    async fn activation_demand_set_and_completion_publish_one_ticket_batch_each() {
        let (_directory, level) = level().await;
        for x in -2..=2 {
            for z in -2..=2 {
                level
                    .loaded_chunks
                    .insert(Vector2::new(x, z), ChunkData::empty_sync(x, z));
            }
        }
        let positions: Vec<_> = (-1..=1)
            .flat_map(|x| (-1..=1).map(move |z| Vector2::new(x, z)))
            .collect();
        let mut demand = level.activation_demand();
        let before = level.chunk_activation_revision.load(Ordering::Acquire);
        demand.replace(positions.iter().copied());
        assert_eq!(
            level.chunk_activation_revision.load(Ordering::Acquire),
            before + 1
        );
        assert_eq!(level.chunk_loading.lock().unwrap().ticket.len(), 9);
        demand.replace(positions);
        assert_eq!(
            level.chunk_activation_revision.load(Ordering::Acquire),
            before + 1,
            "unchanged requests do not wake themselves"
        );
        level.process_chunk_activations(|chunk| chunk.process_post_processing(|_| true));
        assert_eq!(
            level.chunk_activation_revision.load(Ordering::Acquire),
            before + 2
        );
        assert!(level.chunk_loading.lock().unwrap().ticket.is_empty());
        demand.replace([]);
        assert!(
            level
                .chunk_activation
                .state
                .lock()
                .unwrap()
                .owners
                .is_empty()
        );
    }
}
