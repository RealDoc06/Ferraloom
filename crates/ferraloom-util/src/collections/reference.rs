//! Identity collections use insertion order intentionally. System.identityHashCode
//! differs across vanilla runs, so traversal here is not a vanilla parity property.
//! Addresses are used for lookup only, never for iteration or externally visible IDs.
use super::{OpenHashKey, OpenHashMap, mix_long};
use std::{rc::Rc, sync::Arc};

/// Reference equality for owned/shared Java object references. Two independently
/// allocated equal values are distinct keys. Clones of Rc/Arc share a key.
pub trait ReferenceKey: Clone {
    fn reference_address(&self) -> usize;
}
impl<T: ?Sized> ReferenceKey for Rc<T> {
    fn reference_address(&self) -> usize {
        Rc::as_ptr(self).cast::<()>() as usize
    }
}
impl<T: ?Sized> ReferenceKey for Arc<T> {
    fn reference_address(&self) -> usize {
        Arc::as_ptr(self).cast::<()>() as usize
    }
}
impl<T: ?Sized> ReferenceKey for &T {
    fn reference_address(&self) -> usize {
        std::ptr::from_ref(*self).cast::<()>() as usize
    }
}
impl<T: ReferenceKey> ReferenceKey for Option<T> {
    fn reference_address(&self) -> usize {
        self.as_ref().map_or(0, ReferenceKey::reference_address)
    }
}
#[derive(Clone, Debug)]
struct Key<K>(K);
impl<K: ReferenceKey> PartialEq for Key<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0.reference_address() == other.0.reference_address()
    }
}
impl<K: ReferenceKey> Eq for Key<K> {}
#[derive(Clone, Copy, Debug)]
struct ReferenceHash;
impl<K: ReferenceKey> OpenHashKey<Key<K>> for ReferenceHash {
    fn hash(key: &Key<K>) -> usize {
        mix_long(key.0.reference_address() as i64) as usize
    }
    fn is_null(key: &Key<K>) -> bool {
        key.0.reference_address() == 0
    }
}
/// Deterministic insertion-ordered counterpart of fastutil Reference2* maps and
/// JDK IdentityHashMap. Hash table traversal is deliberately not exposed.
#[derive(Clone, Debug)]
pub struct ReferenceHashMap<K, V>(OpenHashMap<Key<K>, V, ReferenceHash, true>);
impl<K: ReferenceKey, V> Default for ReferenceHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: ReferenceKey, V> ReferenceHashMap<K, V> {
    pub fn new() -> Self {
        Self(OpenHashMap::new())
    }
    pub fn with_capacity(n: usize) -> Self {
        Self(OpenHashMap::with_capacity(n))
    }
    pub fn with_capacity_and_load_factor(n: usize, f: f32) -> Self {
        Self(OpenHashMap::with_capacity_and_load_factor(n, f))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(&Key(key.clone()))
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(&Key(key.clone()))
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.0.get_mut(&Key(key.clone()))
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(Key(key), value)
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.0.remove(&Key(key.clone()))
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn trim(&mut self) {
        self.0.trim();
    }
    pub fn trim_to(&mut self, n: usize) {
        self.0.trim_to(n);
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&K, &V)> + ExactSizeIterator {
        self.0.iter().map(|(k, v)| (&k.0, v))
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.iter().map(|(_, v)| v)
    }
    pub fn first(&self) -> Option<(&K, &V)> {
        self.0.first().map(|(k, v)| (&k.0, v))
    }
    pub fn last(&self) -> Option<(&K, &V)> {
        self.0.last().map(|(k, v)| (&k.0, v))
    }
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        self.0.pop_first().map(|(k, v)| (k.0, v))
    }
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        self.0.pop_last().map(|(k, v)| (k.0, v))
    }
    pub fn get_and_move_to_first(&mut self, key: &K) -> Option<&mut V> {
        self.0.get_and_move_to_first(&Key(key.clone()))
    }
    pub fn get_and_move_to_last(&mut self, key: &K) -> Option<&mut V> {
        self.0.get_and_move_to_last(&Key(key.clone()))
    }
    pub fn insert_and_move_to_first(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert_and_move_to_first(Key(key), value)
    }
    pub fn insert_and_move_to_last(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert_and_move_to_last(Key(key), value)
    }
}
impl<K: ReferenceKey, V> IntoIterator for ReferenceHashMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
            .map(|(k, v)| (k.0, v))
            .collect::<Vec<_>>()
            .into_iter()
    }
}
// The concrete iterator avoids exposing the private identity key wrapper.
pub struct ReferenceIter<'a, K, V>(super::open::Iter<'a, Key<K>, V, ReferenceHash, true>);
impl<'a, K, V> Iterator for ReferenceIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (&k.0, v))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<K, V> ExactSizeIterator for ReferenceIter<'_, K, V> {}
impl<K, V> DoubleEndedIterator for ReferenceIter<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(k, v)| (&k.0, v))
    }
}
impl<'a, K: ReferenceKey, V> IntoIterator for &'a ReferenceHashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = ReferenceIter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        ReferenceIter(self.0.iter())
    }
}
#[derive(Clone, Debug)]
pub struct ReferenceOpenHashSet<K>(ReferenceHashMap<K, ()>);
impl<K: ReferenceKey> Default for ReferenceOpenHashSet<K> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: ReferenceKey> ReferenceOpenHashSet<K> {
    pub fn new() -> Self {
        Self(ReferenceHashMap::new())
    }
    pub fn with_capacity(n: usize) -> Self {
        Self(ReferenceHashMap::with_capacity(n))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn insert(&mut self, key: K) -> bool {
        self.0.insert(key, ()).is_none()
    }
    pub fn contains(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }
    pub fn remove(&mut self, key: &K) -> bool {
        self.0.remove(key).is_some()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn trim(&mut self) {
        self.0.trim();
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.0.keys()
    }
    pub fn first(&self) -> Option<&K> {
        self.0.first().map(|(k, _)| k)
    }
    pub fn last(&self) -> Option<&K> {
        self.0.last().map(|(k, _)| k)
    }
    pub fn pop_first(&mut self) -> Option<K> {
        self.0.pop_first().map(|(k, _)| k)
    }
    pub fn pop_last(&mut self) -> Option<K> {
        self.0.pop_last().map(|(k, _)| k)
    }
    pub fn insert_and_move_to_first(&mut self, key: K) -> bool {
        self.0.insert_and_move_to_first(key, ()).is_none()
    }
    pub fn insert_and_move_to_last(&mut self, key: K) -> bool {
        self.0.insert_and_move_to_last(key, ()).is_none()
    }
}
impl<K: ReferenceKey> IntoIterator for ReferenceOpenHashSet<K> {
    type Item = K;
    type IntoIter = std::vec::IntoIter<K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
            .map(|(k, _)| k)
            .collect::<Vec<_>>()
            .into_iter()
    }
}
impl<'a, K: ReferenceKey> IntoIterator for &'a ReferenceOpenHashSet<K> {
    type Item = &'a K;
    type IntoIter = std::iter::Map<ReferenceIter<'a, K, ()>, fn((&'a K, &'a ())) -> &'a K>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter().map(|(k, _)| k)
    }
}
