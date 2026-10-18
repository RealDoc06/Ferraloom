//! Stable, single-owner storage for publication during a suspended shared read.
//! Elsa's safe frozen collections keep heap allocations stable across growth;
//! mutation/removal still require exclusive access to this store.
use elsa::{FrozenMap, FrozenVec};
use std::{cell::Cell, collections::HashSet, hash::Hash};

pub(crate) struct InsertOnlyMap<K, V> {
    entries: FrozenMap<K, Box<V>>,
    keys: FrozenVec<K>,
}

impl<K: Eq + Hash, V> Default for InsertOnlyMap<K, V> {
    fn default() -> Self {
        Self {
            entries: FrozenMap::new(),
            keys: FrozenVec::new(),
        }
    }
}

impl<K: Copy + Eq + Hash, V> InsertOnlyMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries.as_mut().get_mut(key).map(Box::as_mut)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// No callback runs between checking and insertion. Copy primitive keys have
    /// no gameplay Hash/Eq callbacks; shared insertion never replaces an entry.
    pub fn publish(&self, key: K, value: V) -> Result<&V, V> {
        if self.contains_key(&key) {
            return Err(value);
        }
        let value = self.entries.insert(key, Box::new(value));
        self.keys.push(key);
        Ok(value)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let previous = self.entries.as_mut().insert(key, Box::new(value));
        if previous.is_none() {
            self.keys.push(key);
        }
        previous.map(|value| *value)
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let value = self.entries.as_mut().remove(key)?;
        self.keys.as_mut().retain(|entry| entry != key);
        Some(*value)
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.entries
            .as_mut()
            .iter_mut()
            .map(|(key, value)| (key, value.as_mut()))
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        (0..self.keys.len())
            .filter_map(|index| self.keys.get_copy(index))
            .filter_map(|key| self.get(&key))
    }
    fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries
            .as_mut()
            .retain(|key, value| keep(key, value.as_ref()));
        self.keys
            .as_mut()
            .retain(|key| self.entries.get(key).is_some());
    }
}

/// Light imports can replace a snapshot during managedBlock. New versions are
/// immutable; existing shared references remain valid until an exclusive prune.
pub(super) struct SnapshotMap<K, V> {
    current: InsertOnlyMap<K, Cell<u64>>,
    versions: InsertOnlyMap<u64, V>,
    next: Cell<u64>,
}
impl<K: Copy + Eq + Hash, V> Default for SnapshotMap<K, V> {
    fn default() -> Self {
        Self {
            current: InsertOnlyMap::default(),
            versions: InsertOnlyMap::default(),
            next: Cell::new(0),
        }
    }
}
impl<K: Copy + Eq + Hash, V> SnapshotMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.versions.get(&self.current.get(key)?.get())
    }
    pub fn insert(&self, key: K, value: V) {
        let generation = self
            .next
            .get()
            .checked_add(1)
            .expect("snapshot identities exhausted");
        self.next.set(generation);
        assert!(self.versions.publish(generation, value).is_ok());
        if let Some(current) = self.current.get(&key) {
            current.set(generation);
        } else {
            assert!(self.current.publish(key, Cell::new(generation)).is_ok());
        }
    }
    pub fn prune(&mut self) {
        let live: HashSet<_> = self.current.values().map(Cell::get).collect();
        self.versions
            .retain(|generation, _| live.contains(generation));
    }
    pub fn remove(&mut self, key: &K) {
        self.current.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_keeps_borrowed_records_stable_and_rejects_replacement() {
        let mut store = InsertOnlyMap::default();
        let first = store.publish(0_u64, String::from("original")).unwrap();
        for key in 1..1024 {
            store.publish(key, key.to_string()).unwrap();
        }
        assert_eq!(
            store.publish(0, String::from("replacement")),
            Err(String::from("replacement"))
        );
        assert!(std::ptr::eq(first, store.get(&0).unwrap()));
        assert_eq!(first, "original");
        // The shared borrows end before phase-boundary exclusive removal.
        assert_eq!(store.remove(&0), Some(String::from("original")));
        store.publish(0, String::from("new generation")).unwrap();
        assert_eq!(store.values().count(), 1024);
        store.get_mut(&0).unwrap().push('!');
        assert_eq!(store.get(&0).unwrap(), "new generation!");
    }
    #[test]
    fn imported_snapshot_versions_keep_existing_reads_valid_until_exclusive_prune() {
        let mut snapshots = SnapshotMap::default();
        snapshots.insert(0_u64, String::from("before"));
        let before = snapshots.get(&0).unwrap();
        snapshots.insert(0, String::from("after"));
        assert_eq!(before, "before");
        assert_eq!(snapshots.get(&0).unwrap(), "after");
        snapshots.prune();
        assert_eq!(snapshots.versions.values().count(), 1);
        assert_eq!(snapshots.get(&0).unwrap(), "after");
        snapshots.remove(&0);
        snapshots.prune();
        assert!(snapshots.get(&0).is_none());
        assert_eq!(snapshots.versions.values().count(), 0);
    }
}
