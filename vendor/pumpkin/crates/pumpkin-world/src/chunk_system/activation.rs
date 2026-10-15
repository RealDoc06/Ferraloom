use super::ChunkLoading;
use crate::level::{Level, SyncChunk};
use pumpkin_data::chunk::ChunkStatus;
use pumpkin_util::math::vector2::Vector2;
use rustc_hash::FxHashSet;
use std::sync::{Mutex, atomic::Ordering};

/// Only actual simulation/send requests enter this set. Generation of a halo
/// does not activate its members, so the one-chunk halo cannot grow recursively.
#[derive(Default)]
pub(crate) struct ChunkActivation {
    pending: Mutex<FxHashSet<Vector2<i32>>>,
    consumer: Mutex<()>,
}

impl Level {
    /// Request the 3x3 FULL halo once and retain its ticket until processing succeeds.
    pub fn request_chunk_activation(&self, pos: Vector2<i32>) {
        let mut pending = self
            .chunk_activation
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.ready_chunk(pos).is_some() || !pending.insert(pos) {
            return;
        }
        let mut loading = self
            .chunk_loading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loading.add_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL - 1);
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

    /// One nonblocking pass over requested centers. FULL is a generation property;
    /// readiness is published only after the consumer succeeds with a stable halo.
    /// No lock in this path crosses an await. Concurrent consumers leave work to
    /// the current pass and are woken when it finishes.
    pub fn process_chunk_activations(&self, mut process: impl FnMut(&SyncChunk) -> bool) {
        let consumer = match self.chunk_activation.consumer.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let observed_revision = self.chunk_activation_revision.load(Ordering::Acquire);
        let mut centers: Vec<_> = self
            .chunk_activation
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .copied()
            .collect();
        centers.sort_unstable_by_key(|pos| (pos.x, pos.y));
        let mut completed = false;
        for pos in centers {
            let lifecycle = self
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
            if !process(&chunk) {
                continue;
            }
            // Unload takes the lifecycle write lock before removing map entries.
            // Drop our read guard before taking the live ticket mutex.
            drop(lifecycle);
            let mut pending = self
                .chunk_activation
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !chunk.is_post_processing_ready() {
                continue;
            }
            if pending.remove(&pos) {
                completed = true;
                let mut loading = self
                    .chunk_loading
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                loading.remove_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL - 1);
                loading.send_change();
            }
        }
        drop(consumer);
        if completed {
            self.chunk_activation_revision
                .fetch_add(1, Ordering::AcqRel);
        }
        // A waiter may have observed a load/request while this consumer was busy.
        // Wake it after releasing the consumer, without waking a deferred pass on
        // its own unchanged state (which would turn direct sends into a busy loop).
        if completed || self.chunk_activation_revision.load(Ordering::Acquire) != observed_revision
        {
            self.chunk_activation_changed.notify_waiters();
        }
    }
}
