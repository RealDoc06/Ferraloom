//! Default fastutil object map with live indexed entries and native traversal.
use crate::object_open_set::{CursorError, ObjectKey};
use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};
#[derive(Clone)]
struct Table<K, V> {
    keys: Vec<Option<K>>,
    values: Vec<Option<V>>,
}
impl<K, V> Table<K, V> {
    fn new(n: usize) -> Self {
        Self {
            keys: std::iter::repeat_with(|| None).take(n + 1).collect(),
            values: std::iter::repeat_with(|| None).take(n + 1).collect(),
        }
    }
}
/// An entry retains its array index, not a copy of its key/value. Clones share
/// invalidation when its entry iterator removes that entry.
#[derive(Clone)]
pub struct Entry(Arc<AtomicI32>);
impl Entry {
    fn new(index: i32) -> Self {
        Self(Arc::new(AtomicI32::new(index)))
    }
    fn index(&self) -> i32 {
        self.0.load(Ordering::Relaxed)
    }
    fn invalidate(&self) {
        self.0.store(-1, Ordering::Relaxed);
    }
}
pub struct Cursor<K> {
    position: i32,
    last: i32,
    remaining: i32,
    null: bool,
    wrapped: Option<Vec<Option<K>>>,
    entry: Option<Entry>,
}
impl<K> Cursor<K> {
    pub fn has_next(&self) -> bool {
        self.remaining != 0
    }
}
#[derive(Debug)]
pub enum TraversalError<E> {
    Cursor(CursorError),
    Callback(E),
}
/// The table is owned: fastutil's `forEach` keeps reading the key array it
/// started with, which a callback's rehash replaces, so a rehash during a
/// `for_each` keeps the replaced keys (`retired`, by the `generation` they
/// were current in) until the outermost `for_each` ends. Before a rehash
/// the walk reads the live keys, writes in place included, as Java's
/// captured array reference does.
#[derive(Clone)]
pub struct ObjectOpenMap<K, V> {
    table: Table<K, V>,
    size: i32,
    null: bool,
    generation: u64,
    walking: u32,
    retired: Vec<(u64, Vec<Option<K>>)>,
}
impl<K, V> Default for ObjectOpenMap<K, V> {
    fn default() -> Self {
        Self {
            table: Table::new(32),
            size: 0,
            null: false,
            generation: 0,
            walking: 0,
            retired: Vec::new(),
        }
    }
}
fn mixed(hash: i32) -> usize {
    let h = (hash as u32).wrapping_mul(0x9e3779b9);
    (h ^ (h >> 16)) as usize
}
impl<K: ObjectKey + Clone, V: Clone> ObjectOpenMap<K, V> {
    pub fn len(&self) -> i32 {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    fn n(&self) -> usize {
        self.table.keys.len() - 1
    }
    fn max_fill(&self) -> i32 {
        (self.n() * 3 / 4) as i32
    }
    fn find(&self, key: &K) -> usize {
        let table = &self.table;
        let mask = table.keys.len() - 2;
        let mut p = mixed(key.object_hash()) & mask;
        while table.keys[p].as_ref().is_some_and(|k| k != key) {
            p = (p + 1) & mask;
        }
        p
    }
    pub fn contains_key(&self, key: Option<&K>) -> bool {
        key.map_or(self.null, |k| self.table.keys[self.find(k)].is_some())
    }
    pub fn get(&self, key: Option<&K>) -> Option<V> {
        let p = key.map_or(self.n(), |k| self.find(k));
        let table = &self.table;
        if key.map_or(self.null, |_| table.keys[p].is_some()) {
            table.values[p].clone()
        } else {
            None
        }
    }
    pub fn entries(&self) -> Vec<(Option<K>, Option<V>)> {
        let table = &self.table;
        let n = table.keys.len() - 1;
        let mut entries = Vec::new();
        if self.null {
            entries.push((None, table.values[n].clone()));
        }
        for p in (0..n).rev() {
            if table.keys[p].is_some() {
                entries.push((table.keys[p].clone(), table.values[p].clone()));
            }
        }
        entries
    }
    pub fn put(&mut self, key: Option<K>, value: Option<V>) -> Option<V> {
        let p = key.as_ref().map_or(self.n(), |k| self.find(k));
        let existing = if key.is_some() {
            self.table.keys[p].is_some()
        } else {
            self.null
        };
        let previous = std::mem::replace(&mut self.table.values[p], value);
        if existing {
            return previous;
        }
        if key.is_none() {
            self.null = true;
        }
        self.table.keys[p] = key;
        let old = self.size;
        self.size = self.size.wrapping_add(1);
        if old >= self.max_fill() {
            self.rehash(
                (((self.size + 1) as f64 / 0.75).ceil() as usize)
                    .max(2)
                    .next_power_of_two(),
            );
        }
        None
    }
    fn rehash(&mut self, n: usize) {
        let old = &self.table;
        let old_n = old.keys.len() - 1;
        let mut table = Table::new(n);
        for p in (0..old_n).rev() {
            if let Some(key) = &old.keys[p] {
                let mut dest = mixed(key.object_hash()) & (n - 1);
                while table.keys[dest].is_some() {
                    dest = (dest + 1) & (n - 1);
                }
                table.keys[dest] = Some(key.clone());
                table.values[dest] = old.values[p].clone();
            }
        }
        table.values[n] = old.values[old_n].clone();
        let old = std::mem::replace(&mut self.table, table);
        // A `for_each` under way reads on in the keys it started with.
        if self.walking > 0 {
            self.retired.push((self.generation, old.keys));
        }
        self.generation += 1;
    }
    fn shift(
        &mut self,
        mut hole: usize,
        mut cursor: Option<&mut Cursor<K>>,
    ) -> Result<(), CursorError> {
        let table = &mut self.table;
        let mask = table.keys.len() - 2;
        loop {
            let mut p = (hole + 1) & mask;
            loop {
                let Some(key) = &table.keys[p] else {
                    *table
                        .keys
                        .get_mut(hole)
                        .ok_or(CursorError::IndexOutOfBounds)? = None;
                    table.values[hole] = None;
                    return Ok(());
                };
                let home = mixed(key.object_hash()) & mask;
                if if hole <= p {
                    hole >= home || home > p
                } else {
                    hole >= home && home > p
                } {
                    break;
                }
                p = (p + 1) & mask;
            }
            if p < hole {
                if let Some(c) = &mut cursor {
                    c.wrapped
                        .get_or_insert_with(Vec::new)
                        .push(table.keys[p].clone());
                }
            }
            let key = table.keys[p].clone();
            let value = table.values[p].clone();
            *table
                .keys
                .get_mut(hole)
                .ok_or(CursorError::IndexOutOfBounds)? = key;
            table.values[hole] = value;
            hole = p;
        }
    }
    pub fn remove(&mut self, key: Option<&K>) -> Option<V> {
        if !self.contains_key(key) {
            return None;
        }
        let p = key.map_or(self.n(), |k| self.find(k));
        let old = self.table.values[p].clone();
        self.size = self.size.wrapping_sub(1);
        if key.is_none() {
            self.null = false;
            self.table.keys[p] = None;
            self.table.values[p] = None;
        } else {
            self.shift(p, None).unwrap();
        }
        if self.n() > 32 && self.size < self.max_fill() / 4 {
            self.rehash(self.n() / 2);
        }
        old
    }
    pub fn clear(&mut self) {
        if self.size == 0 {
            return;
        }
        self.size = 0;
        self.null = false;
        self.table.keys.fill(None);
        self.table.values.fill(None);
    }
    pub fn cursor(&self) -> Cursor<K> {
        Cursor {
            position: self.n() as i32,
            last: -1,
            remaining: self.size,
            null: self.null,
            wrapped: None,
            entry: None,
        }
    }
    pub fn next_entry(&self, c: &mut Cursor<K>) -> Result<Entry, CursorError> {
        if !c.has_next() {
            return Err(CursorError::NoSuchElement);
        }
        c.remaining = c.remaining.wrapping_sub(1);
        let index = if c.null {
            c.null = false;
            c.last = self.n() as i32;
            c.last
        } else {
            loop {
                c.position = c.position.wrapping_sub(1);
                if c.position < 0 {
                    c.last = i32::MIN;
                    let key = c
                        .wrapped
                        .as_ref()
                        .ok_or(CursorError::MissingWrapped)?
                        .get(c.position.wrapping_neg().wrapping_sub(1) as usize)
                        .ok_or(CursorError::IndexOutOfBounds)?
                        .as_ref()
                        .ok_or(CursorError::MissingWrapped)?;
                    let p = self.find(key);
                    // Deleting a wrapped member externally can make native iteration
                    // nonterminating. Such invalidated wrapped iterators are excluded.
                    if self.table.keys[p].is_none() {
                        return Err(CursorError::MissingWrapped);
                    }
                    break p as i32;
                }
                if self
                    .table
                    .keys
                    .get(c.position as usize)
                    .ok_or(CursorError::IndexOutOfBounds)?
                    .is_some()
                {
                    c.last = c.position;
                    break c.position;
                }
            }
        };
        let entry = Entry::new(index);
        c.entry = Some(entry.clone());
        Ok(entry)
    }
    pub fn entry_key(&self, e: &Entry) -> Result<Option<K>, CursorError> {
        self.table
            .keys
            .get(e.index() as usize)
            .cloned()
            .ok_or(CursorError::IndexOutOfBounds)
    }
    pub fn entry_value(&self, e: &Entry) -> Result<Option<V>, CursorError> {
        self.table
            .values
            .get(e.index() as usize)
            .cloned()
            .ok_or(CursorError::IndexOutOfBounds)
    }
    pub fn set_entry_value(
        &mut self,
        e: &Entry,
        value: Option<V>,
    ) -> Result<Option<V>, CursorError> {
        Ok(std::mem::replace(
            self.table
                .values
                .get_mut(e.index() as usize)
                .ok_or(CursorError::IndexOutOfBounds)?,
            value,
        ))
    }
    pub fn remove_current(&mut self, c: &mut Cursor<K>) -> Result<(), CursorError> {
        if c.last == -1 {
            return Err(CursorError::NoCurrentEntry);
        }
        if c.last == self.n() as i32 {
            self.null = false;
            let n = self.table.keys.len() - 1;
            self.table.keys[n] = None;
            self.table.values[n] = None;
        } else if c.position >= 0 {
            self.shift(c.last as usize, Some(c))?;
        } else {
            let key = c
                .wrapped
                .as_mut()
                .ok_or(CursorError::MissingWrapped)?
                .get_mut(c.position.wrapping_neg().wrapping_sub(1) as usize)
                .ok_or(CursorError::IndexOutOfBounds)?
                .take();
            self.remove(key.as_ref());
            c.last = -1;
            if let Some(e) = &c.entry {
                e.invalidate();
            }
            return Ok(());
        }
        self.size = self.size.wrapping_sub(1);
        c.last = -1;
        if let Some(e) = &c.entry {
            e.invalidate();
        }
        Ok(())
    }
    pub fn for_each<E>(
        &mut self,
        mut action: impl FnMut(&mut Self, Option<K>, Option<V>) -> Result<(), E>,
    ) -> Result<(), TraversalError<E>> {
        if self.null {
            let value = self.table.values[self.n()].clone();
            action(self, None, value).map_err(TraversalError::Callback)?;
        }
        // Keys are captured after the null callback. Values remain live even if
        // a callback rehashes the map and replaces both native arrays.
        let (generation, n) = (self.generation, self.n());
        self.walking += 1;
        let walked = (|| {
            for p in (0..n).rev() {
                let key = if self.generation == generation {
                    self.table.keys[p].clone()
                } else {
                    let keys = self.retired.iter().find(|(g, _)| *g == generation);
                    keys.and_then(|(_, keys)| keys[p].clone())
                };
                if key.is_some() {
                    let value = (self.table.values.get(p).cloned())
                        .ok_or(TraversalError::Cursor(CursorError::IndexOutOfBounds))?;
                    action(self, key, value).map_err(TraversalError::Callback)?;
                }
            }
            Ok(())
        })();
        self.walking -= 1;
        if self.walking == 0 {
            self.retired.clear();
        }
        walked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct K(i32);
    impl ObjectKey for K {
        fn object_hash(&self) -> i32 {
            self.0
        }
    }

    fn map() -> ObjectOpenMap<K, i32> {
        let mut m = ObjectOpenMap::default();
        for k in 1..=3 {
            m.put(Some(K(k)), Some(k * 10));
        }
        m
    }
    fn keys(m: &ObjectOpenMap<K, i32>) -> Vec<K> {
        m.entries().into_iter().filter_map(|(k, _)| k).collect()
    }

    /// `forEach` walks the key array it started with: what a callback puts
    /// in it before a rehash may be met, what goes in the new array after
    /// one is not, and the keys there were are each met once.
    #[test]
    fn for_each_walks_the_keys_it_started_with() {
        let mut m = map();
        let before = keys(&m);
        let mut seen = Vec::new();
        let mut later = Vec::new();
        m.for_each(|m, key, _| {
            if seen.is_empty() {
                let generation = m.generation;
                let mut k = 100;
                while m.generation == generation {
                    m.put(Some(K(k)), Some(k));
                    k += 1;
                }
                for k in 1000..1010 {
                    m.put(Some(K(k)), Some(k));
                    later.push(K(k));
                }
            }
            seen.extend(key);
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(seen.iter().all(|k| !later.contains(k)));
        for k in &before {
            assert_eq!(seen.iter().filter(|s| *s == k).count(), 1, "{k:?}");
        }
        assert!(m.retired.is_empty());

        let mut m = map();
        let before = keys(&m);
        let last = before.last().cloned();
        let mut seen = Vec::new();
        m.for_each(|m, key, _| {
            if seen.is_empty() {
                m.remove(last.as_ref());
            }
            seen.extend(key);
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(seen, before[..2]);
    }
}
