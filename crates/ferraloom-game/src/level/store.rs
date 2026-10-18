//! Stable, single-owner storage for publication during a suspended shared read.
//! Elsa's safe frozen collections keep heap allocations stable across growth;
//! mutation/removal still require exclusive access to this store.
use elsa::{FrozenMap, FrozenVec};
use std::hash::Hash;

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
        self.keys.copy_iter().filter_map(|key| self.get(&key))
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
}
