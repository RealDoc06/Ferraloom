#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/*
TODO
1. add proto chunk dirty flag
2. better priority
5. add lifetime to loading ticket
6. solve entity not unload problem
*/

pub type HashMapType<K, V> = rustc_hash::FxHashMap<K, V>;
pub type HashSetType<K> = rustc_hash::FxHashSet<K>;
pub type ChunkPos = pumpkin_util::math::vector2::Vector2<i32>;
pub type ChunkLevel = HashMapType<ChunkPos, i8>;
pub type IOLock = std::sync::Arc<(
    std::sync::Mutex<HashMapType<ChunkPos, IoLockState>>,
    tokio::sync::Notify,
)>;

#[derive(Default)]
pub struct IoLockState {
    pub(super) writes: u8,
    pub(super) poi: Option<std::sync::Arc<()>>,
}

/// One scheduler-owned read reservation, separate from ordinary queued writers.
/// The worker carries its identity, never an owning guard or a terrain handle.
pub(crate) struct PoiReadReservation {
    lock: IOLock,
    positions: Vec<ChunkPos>,
    pub(crate) identity: std::sync::Arc<()>,
}

impl PoiReadReservation {
    pub(crate) fn try_new(lock: &IOLock, positions: Vec<ChunkPos>) -> Option<Self> {
        let mut entries = lock
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if positions.iter().any(|pos| entries.contains_key(pos)) {
            return None;
        }
        let identity = std::sync::Arc::new(());
        for pos in &positions {
            entries.insert(
                *pos,
                IoLockState {
                    writes: 0,
                    poi: Some(identity.clone()),
                },
            );
        }
        Some(Self {
            lock: lock.clone(),
            positions,
            identity,
        })
    }
}

impl Drop for PoiReadReservation {
    fn drop(&mut self) {
        let mut entries = self
            .lock
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for pos in &self.positions {
            if let Some(entry) = entries.get_mut(pos)
                && entry
                    .poi
                    .as_ref()
                    .is_some_and(|id| std::sync::Arc::ptr_eq(id, &self.identity))
            {
                entry.poi = None;
                if entry.writes == 0 {
                    entries.remove(pos);
                }
            }
        }
        drop(entries);
        self.lock.1.notify_waiters();
    }
}

pub(crate) mod activation;
pub mod channel;
pub mod chunk_holder;
pub mod chunk_listener;
pub mod chunk_loading;
pub mod chunk_state;
pub mod dag;
pub mod generation;
pub mod generation_cache;
pub mod schedule;
pub mod unload;
pub mod worker_logic;

#[cfg(test)]
mod tests;

pub use channel::LevelChannel;
pub use chunk_holder::ChunkHolder;
pub use chunk_listener::ChunkListener;
pub use chunk_loading::ChunkLoading;
pub use chunk_state::{Chunk, StagedChunkEnum};
pub use dag::DAG;
pub use generation::generate_single_chunk;
pub use generation_cache::Cache;
pub use schedule::GenerationSchedule;
