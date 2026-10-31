//! Tick scheduler membership, invalidated by resident key changes or resets.
use std::collections::HashSet;

type Key = (i32, i32);

/// Borrowed resident membership used by scheduled-tick registration.
/// Keys retain their owner's traversal order; reading a view owns no chunks.
pub trait LoadedTickChunks {
    fn membership_revision(&self) -> u64;
    fn contains_chunk(&self, key: &Key) -> bool;
    fn chunk_keys(&self) -> impl Iterator<Item = Key>;
}

#[derive(Default)]
pub struct TickChunkMembership {
    keys: HashSet<Key>,
    revision: Option<u64>,
    added: Vec<Key>,
    removed: Vec<Key>,
    /// Loaded chunks with saved ticks that have not started ticking yet
    /// (`LevelChunkTicks.pendingTicks`, unpacked by `startTickingChunk`).
    waiting: HashSet<Key>,
}

impl TickChunkMembership {
    /// A scheduler reset must re-register resident chunks even if the world did
    /// not change. Keep this invalidation with the keys so callers cannot forget.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.revision = None;
        self.added.clear();
        self.removed.clear();
        self.waiting.clear();
    }

    /// Returns false in O(1) when resident membership is unchanged. On change,
    /// update the existing set and record only registration/removal deltas.
    pub fn synchronize(&mut self, chunks: &impl LoadedTickChunks) -> bool {
        let revision = chunks.membership_revision();
        if self.revision == Some(revision) {
            return false;
        }
        self.added.clear();
        self.removed.clear();
        self.keys.retain(|key| {
            if chunks.contains_chunk(key) {
                true
            } else {
                self.removed.push(*key);
                self.waiting.remove(key);
                false
            }
        });
        for key in chunks.chunk_keys() {
            if self.keys.insert(key) {
                self.added.push(key);
            }
        }
        self.revision = Some(revision);
        true
    }

    /// Allows registration to borrow the owning level without cloning its set.
    pub fn take_added(&mut self) -> Vec<Key> {
        std::mem::take(&mut self.added)
    }

    /// Retain scratch capacity after registration.
    pub fn recycle_added(&mut self, mut added: Vec<Key>) {
        added.clear();
        self.added = added;
    }

    pub fn removed(&self) -> &[Key] {
        &self.removed
    }

    /// `key` was registered with saved ticks to unpack once it ticks.
    pub fn wait(&mut self, key: Key) {
        self.waiting.insert(key);
    }

    /// The waiting chunks that `ticking` says now tick, no longer waiting.
    pub fn start_ticking(&mut self, mut ticking: impl FnMut(Key) -> bool) -> Vec<Key> {
        if self.waiting.is_empty() {
            return Vec::new();
        }
        let started: Vec<Key> = self
            .waiting
            .iter()
            .copied()
            .filter(|&k| ticking(k))
            .collect();
        for key in &started {
            self.waiting.remove(key);
        }
        started
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Resident {
        revision: u64,
        keys: Vec<Key>,
    }
    impl LoadedTickChunks for Resident {
        fn membership_revision(&self) -> u64 {
            self.revision
        }
        fn contains_chunk(&self, key: &Key) -> bool {
            self.keys.contains(key)
        }
        fn chunk_keys(&self) -> impl Iterator<Item = Key> {
            self.keys.iter().copied()
        }
    }

    #[test]
    fn loaded_chunks_keep_owner_order_and_wait_until_they_tick() {
        let mut owner = Resident {
            revision: 1,
            keys: vec![(2, 0), (0, 0), (1, 0)],
        };
        let mut cache = TickChunkMembership::default();
        assert!(cache.synchronize(&owner));
        let added = cache.take_added();
        assert_eq!(added, owner.keys);
        cache.recycle_added(added);
        assert!(!cache.synchronize(&owner));
        for &key in &owner.keys {
            cache.wait(key);
        }
        assert_eq!(cache.start_ticking(|k| k == (0, 0)), vec![(0, 0)]);
        assert!(cache.start_ticking(|k| k == (0, 0)).is_empty());
        owner.keys.retain(|&key| key != (1, 0));
        owner.revision += 1;
        cache.synchronize(&owner);
        assert_eq!(cache.removed(), &[(1, 0)]);
        assert_eq!(cache.start_ticking(|_| true), vec![(2, 0)]);
    }
}
