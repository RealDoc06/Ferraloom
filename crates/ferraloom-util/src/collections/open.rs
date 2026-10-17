use super::hash::{
    DEFAULT_INITIAL_SIZE, DEFAULT_LOAD_FACTOR, ObjectHash, OpenHashKey, array_size, max_fill,
};
use std::{marker::PhantomData, mem};

#[derive(Clone, Debug)]
pub(super) struct Bucket<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

/// Port of fastutil 8.5.18 open hash maps. Unlinked iteration visits the null/zero
/// slot first, then table slots descending. Linked iteration follows insertion order.
/// `H` selects primitive versus boxed/object hashing; aliases cover vanilla's types.
#[derive(Clone, Debug)]
pub struct OpenHashMap<K, V, H = ObjectHash, const LINKED: bool = false> {
    pub(super) table: Vec<Option<Bucket<K, V>>>,
    size: usize,
    min_n: usize,
    f: f32,
    pub(super) first: Option<usize>,
    pub(super) last: Option<usize>,
    hash: PhantomData<H>,
}
impl<K: Eq, V, H: OpenHashKey<K>, const L: bool> Default for OpenHashMap<K, V, H, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: Eq, V, H: OpenHashKey<K>, const L: bool> OpenHashMap<K, V, H, L> {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_INITIAL_SIZE)
    }
    /// `expected` counts entries, just like fastutil's constructor.
    pub fn with_capacity(expected: usize) -> Self {
        Self::with_capacity_and_load_factor(expected, DEFAULT_LOAD_FACTOR)
    }
    pub fn with_capacity_and_load_factor(expected: usize, f: f32) -> Self {
        assert!(f > 0.0 && f < 1.0, "fastutil requires 0 < f < 1");
        let n = array_size(expected, f);
        Self {
            table: std::iter::repeat_with(|| None).take(n + 1).collect(),
            size: 0,
            min_n: n,
            f,
            first: None,
            last: None,
            hash: PhantomData,
        }
    }
    /// Number of probe slots (the dedicated null slot is additional).
    pub fn capacity(&self) -> usize {
        self.table.len() - 1
    }
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn load_factor(&self) -> f32 {
        self.f
    }
    pub(super) fn slot(&self, key: &K) -> usize {
        if H::is_null(key) {
            return self.capacity();
        }
        let mask = self.capacity() - 1;
        let mut pos = H::hash(key) & mask;
        while let Some(b) = &self.table[pos] {
            if b.key == *key {
                break;
            }
            pos = (pos + 1) & mask;
        }
        pos
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.table[self.slot(key)].is_some()
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.table[self.slot(key)].as_ref().map(|b| &b.value)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let pos = self.slot(key);
        self.table[pos].as_mut().map(|b| &mut b.value)
    }
    pub fn get_key_value(&self, key: &K) -> Option<(&K, &V)> {
        self.table[self.slot(key)]
            .as_ref()
            .map(|b| (&b.key, &b.value))
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.put(key, value, None)
    }
    pub(super) fn put(&mut self, key: K, value: V, at_first: Option<bool>) -> Option<V> {
        let pos = self.slot(&key);
        if let Some(b) = &mut self.table[pos] {
            let old = mem::replace(&mut b.value, value);
            if let Some(first) = at_first {
                self.move_index(pos, first);
            }
            return Some(old);
        }
        self.table[pos] = Some(Bucket {
            key,
            value,
            prev: None,
            next: None,
        });
        self.attach(pos, at_first == Some(true));
        let grow = self.size >= max_fill(self.capacity(), self.f);
        self.size += 1;
        if grow {
            // fastutil putAndMove/addAndMove uses size, ordinary put/add size+1.
            self.rehash(array_size(
                self.size + usize::from(at_first.is_none()),
                self.f,
            ));
        }
        None
    }
    /// Entry helper; computes the value only for an absent key.
    pub fn get_or_insert_with(&mut self, key: K, value: impl FnOnce() -> V) -> &mut V
    where
        K: Clone,
    {
        if !self.contains_key(&key) {
            self.insert(key.clone(), value());
        }
        self.get_mut(&key).unwrap()
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.remove_entry(key).map(|(_, v)| v)
    }
    pub fn remove_entry(&mut self, key: &K) -> Option<(K, V)> {
        let pos = self.slot(key);
        self.remove_index(pos, true)
    }
    pub(super) fn remove_index(&mut self, pos: usize, shrink: bool) -> Option<(K, V)> {
        self.table[pos].as_ref()?;
        self.detach(pos);
        let b = self.table[pos].take().unwrap();
        self.size -= 1;
        if pos != self.capacity() {
            self.shift_keys(pos);
        }
        let n = self.capacity();
        if shrink
            && n > self.min_n
            && n > DEFAULT_INITIAL_SIZE
            && self.size < max_fill(n, self.f) / 4
        {
            self.rehash(n / 2);
        }
        Some((b.key, b.value))
    }
    /// Port of shiftKeys, including chains that wrap past slot zero.
    fn shift_keys(&mut self, mut gap: usize) {
        let mask = self.capacity() - 1;
        loop {
            let mut pos = (gap + 1) & mask;
            loop {
                let Some(b) = &self.table[pos] else {
                    return;
                };
                let home = H::hash(&b.key) & mask;
                if if gap <= pos {
                    gap >= home || home > pos
                } else {
                    gap >= home && home > pos
                } {
                    break;
                }
                pos = (pos + 1) & mask;
            }
            self.table[gap] = self.table[pos].take();
            if L {
                let b = self.table[gap].as_ref().unwrap();
                let (prev, next) = (b.prev, b.next);
                if let Some(p) = prev {
                    self.table[p].as_mut().unwrap().next = Some(gap);
                } else {
                    self.first = Some(gap);
                }
                if let Some(n) = next {
                    self.table[n].as_mut().unwrap().prev = Some(gap);
                } else {
                    self.last = Some(gap);
                }
            }
            gap = pos;
        }
    }
    fn attach(&mut self, pos: usize, first: bool) {
        if !L {
            return;
        }
        if first {
            self.table[pos].as_mut().unwrap().next = self.first;
            if let Some(p) = self.first {
                self.table[p].as_mut().unwrap().prev = Some(pos);
            } else {
                self.last = Some(pos);
            }
            self.first = Some(pos);
        } else {
            self.table[pos].as_mut().unwrap().prev = self.last;
            if let Some(p) = self.last {
                self.table[p].as_mut().unwrap().next = Some(pos);
            } else {
                self.first = Some(pos);
            }
            self.last = Some(pos);
        }
    }
    fn detach(&mut self, pos: usize) {
        if !L {
            return;
        }
        let b = self.table[pos].as_ref().unwrap();
        let (prev, next) = (b.prev, b.next);
        if let Some(p) = prev {
            self.table[p].as_mut().unwrap().next = next;
        } else {
            self.first = next;
        }
        if let Some(n) = next {
            self.table[n].as_mut().unwrap().prev = prev;
        } else {
            self.last = prev;
        }
        let b = self.table[pos].as_mut().unwrap();
        b.prev = None;
        b.next = None;
    }
    pub(super) fn move_index(&mut self, pos: usize, first: bool) {
        if (first && self.first != Some(pos)) || (!first && self.last != Some(pos)) {
            self.detach(pos);
            self.attach(pos, first);
        }
    }
    pub fn clear(&mut self) {
        for b in &mut self.table {
            *b = None;
        }
        self.size = 0;
        self.first = None;
        self.last = None;
    }
    pub fn ensure_capacity(&mut self, expected: usize) {
        let n = array_size(expected, self.f);
        if n > self.capacity() {
            self.rehash(n);
        }
    }
    pub fn trim(&mut self) {
        self.trim_to(self.size);
    }
    /// Port of trim(int). Java deliberately divides in f32 here, unlike arraySize.
    pub fn trim_to(&mut self, expected: usize) {
        let n = ((expected as f32 / self.f).ceil() as usize).next_power_of_two();
        if n < self.capacity() && self.size <= max_fill(n, self.f) {
            self.rehash(n);
        }
    }
    fn rehash(&mut self, n: usize) {
        let new_table = std::iter::repeat_with(|| None).take(n + 1).collect();
        let mut old = mem::replace(&mut self.table, new_table);
        let old_first = self.first;
        self.first = None;
        self.last = None;
        if L {
            let mut next = old_first;
            while let Some(pos) = next {
                let mut b = old[pos].take().unwrap();
                next = b.next;
                b.prev = None;
                b.next = None;
                let slot = self.slot(&b.key);
                self.table[slot] = Some(b);
                self.attach(slot, false);
            }
        } else {
            let zero = old.pop().unwrap();
            self.table[n] = zero;
            for mut b in old.into_iter().rev().flatten() {
                b.prev = None;
                b.next = None;
                let pos = self.slot(&b.key);
                self.table[pos] = Some(b);
            }
        }
    }
    pub fn iter(&self) -> Iter<'_, K, V, H, L> {
        Iter {
            map: self,
            front: if L { self.first } else { Some(self.capacity()) },
            back: if L { self.last } else { Some(0) },
            remaining: self.size,
        }
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.iter().map(|(_, v)| v)
    }
    /// A removable Java-style iterator. Keys are snapshotted so backward shifts
    /// cannot skip/revisit wrapped keys. Removal never shrinks, like fastutil iterators.
    pub fn cursor(&mut self) -> Cursor<'_, K, V, H, L>
    where
        K: Clone,
    {
        let keys = self.keys().cloned().collect::<Vec<_>>().into_iter();
        Cursor {
            map: self,
            keys,
            current: None,
        }
    }
}
impl<K: Eq, V, H: OpenHashKey<K>> OpenHashMap<K, V, H, true> {
    pub fn first(&self) -> Option<(&K, &V)> {
        self.first.map(|p| {
            let b = self.table[p].as_ref().unwrap();
            (&b.key, &b.value)
        })
    }
    pub fn last(&self) -> Option<(&K, &V)> {
        self.last.map(|p| {
            let b = self.table[p].as_ref().unwrap();
            (&b.key, &b.value)
        })
    }
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        self.remove_index(self.first?, true)
    }
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        self.remove_index(self.last?, true)
    }
    pub fn get_and_move_to_first(&mut self, key: &K) -> Option<&mut V> {
        self.get_and_move(key, true)
    }
    pub fn get_and_move_to_last(&mut self, key: &K) -> Option<&mut V> {
        self.get_and_move(key, false)
    }
    fn get_and_move(&mut self, key: &K, first: bool) -> Option<&mut V> {
        let pos = self.slot(key);
        self.table[pos].as_ref()?;
        self.move_index(pos, first);
        self.table[pos].as_mut().map(|b| &mut b.value)
    }
    pub fn insert_and_move_to_first(&mut self, key: K, value: V) -> Option<V> {
        self.put(key, value, Some(true))
    }
    pub fn insert_and_move_to_last(&mut self, key: K, value: V) -> Option<V> {
        self.put(key, value, Some(false))
    }
}
pub struct Iter<'a, K, V, H, const L: bool> {
    map: &'a OpenHashMap<K, V, H, L>,
    front: Option<usize>,
    back: Option<usize>,
    remaining: usize,
}
impl<'a, K, V, H, const L: bool> Iterator for Iter<'a, K, V, H, L> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while let Some(pos) = self.front {
            self.front = if L {
                self.map.table[pos].as_ref().unwrap().next
            } else {
                pos.checked_sub(1)
            };
            if let Some(b) = &self.map.table[pos] {
                self.remaining -= 1;
                return Some((&b.key, &b.value));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V, H, const L: bool> DoubleEndedIterator for Iter<'_, K, V, H, L> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while let Some(pos) = self.back {
            self.back = if L {
                self.map.table[pos].as_ref().unwrap().prev
            } else {
                Some(pos + 1)
            };
            if let Some(b) = &self.map.table[pos] {
                self.remaining -= 1;
                return Some((&b.key, &b.value));
            }
        }
        None
    }
}
impl<K, V, H, const L: bool> ExactSizeIterator for Iter<'_, K, V, H, L> {}
impl<K, V, H, const L: bool> std::iter::FusedIterator for Iter<'_, K, V, H, L> {}
impl<'a, K: Eq, V, H: OpenHashKey<K>, const L: bool> IntoIterator for &'a OpenHashMap<K, V, H, L> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V, H, L>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<K: Eq, V, H: OpenHashKey<K>, const L: bool> IntoIterator for OpenHashMap<K, V, H, L> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(mut self) -> Self::IntoIter {
        let mut out = Vec::with_capacity(self.size);
        if L {
            let mut next = self.first;
            while let Some(p) = next {
                let b = self.table[p].take().unwrap();
                next = b.next;
                out.push((b.key, b.value));
            }
        } else {
            out.extend(
                self.table
                    .into_iter()
                    .rev()
                    .flatten()
                    .map(|b| (b.key, b.value)),
            );
        }
        out.into_iter()
    }
}
impl<K: Eq, V, H: OpenHashKey<K>, const L: bool> Extend<(K, V)> for OpenHashMap<K, V, H, L> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}
pub struct Cursor<'a, K, V, H, const L: bool> {
    map: &'a mut OpenHashMap<K, V, H, L>,
    keys: std::vec::IntoIter<K>,
    current: Option<K>,
}
impl<K: Eq + Clone, V, H: OpenHashKey<K>, const L: bool> Cursor<'_, K, V, H, L> {
    pub fn next_entry(&mut self) -> Option<(&K, &V)> {
        self.current = self.keys.next();
        self.map.get_key_value(self.current.as_ref()?)
    }
    pub fn remove(&mut self) -> Option<(K, V)> {
        let pos = self.map.slot(&self.current.take()?);
        self.map.remove_index(pos, false)
    }
}

/// Port of fastutil open hash sets, using the same key table and growth as maps.
#[derive(Clone, Debug)]
pub struct OpenHashSet<K, H = ObjectHash, const L: bool = false>(
    pub(super) OpenHashMap<K, (), H, L>,
);
impl<K: Eq, H: OpenHashKey<K>, const L: bool> Default for OpenHashSet<K, H, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: Eq, H: OpenHashKey<K>, const L: bool> OpenHashSet<K, H, L> {
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
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    pub fn insert(&mut self, key: K) -> bool {
        self.0.insert(key, ()).is_none()
    }
    pub fn contains(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }
    pub fn get(&self, key: &K) -> Option<&K> {
        self.0.get_key_value(key).map(|(k, _)| k)
    }
    pub fn remove(&mut self, key: &K) -> bool {
        self.0.remove(key).is_some()
    }
    pub fn take(&mut self, key: &K) -> Option<K> {
        self.0.remove_entry(key).map(|(k, _)| k)
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
    pub fn ensure_capacity(&mut self, n: usize) {
        self.0.ensure_capacity(n);
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.0.keys()
    }
    pub fn cursor(&mut self) -> Cursor<'_, K, (), H, L>
    where
        K: Clone,
    {
        self.0.cursor()
    }
}
impl<K: Eq, H: OpenHashKey<K>> OpenHashSet<K, H, true> {
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
impl<K: Eq, H: OpenHashKey<K>, const L: bool> Extend<K> for OpenHashSet<K, H, L> {
    fn extend<I: IntoIterator<Item = K>>(&mut self, iter: I) {
        for k in iter {
            self.insert(k);
        }
    }
}
impl<K: Eq, H: OpenHashKey<K>, const L: bool> IntoIterator for OpenHashSet<K, H, L> {
    type Item = K;
    type IntoIter = std::iter::Map<std::vec::IntoIter<(K, ())>, fn((K, ())) -> K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().map(|(k, _)| k)
    }
}
impl<'a, K: Eq, H: OpenHashKey<K>, const L: bool> IntoIterator for &'a OpenHashSet<K, H, L> {
    type Item = &'a K;
    type IntoIter = std::iter::Map<Iter<'a, K, (), H, L>, fn((&'a K, &'a ())) -> &'a K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, _)| k)
    }
}
