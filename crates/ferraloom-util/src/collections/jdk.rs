//! JDK 25 HashMap/LinkedHashMap table, bin and red-black tree traversal semantics.
//! Non-Comparable keys with equal hashes use a stable arena-index tie break.
//! OpenJDK uses identityHashCode there, so that subcase has no cross-run parity order.
use super::JavaHash;
use std::{cmp::Ordering, mem};

#[derive(Clone, Debug, Default)]
struct Bin {
    order: Vec<usize>,
    root: Option<usize>,
}
#[derive(Clone, Copy, Debug, Default)]
struct Tree {
    parent: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
    red: bool,
}
#[derive(Clone, Debug)]
pub(super) struct Node<K, V> {
    key: K,
    value: V,
    hash: i32,
    tree: Tree,
    pub(super) prev: Option<usize>,
    pub(super) next: Option<usize>,
}

/// Port of java.util.HashMap; LINKED selects java.util.LinkedHashMap.
/// JDK capacity is a table capacity, unlike fastutil's expected-entry constructor.
#[derive(Clone, Debug)]
pub struct JavaHashMap<K, V, const LINKED: bool = false> {
    bins: Vec<Bin>,
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    size: usize,
    threshold: usize,
    f: f32,
    pub(super) first: Option<usize>,
    pub(super) last: Option<usize>,
    pub(super) access_order: bool,
}
impl<K: Eq + JavaHash, V, const L: bool> Default for JavaHashMap<K, V, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: Eq + JavaHash, V, const L: bool> JavaHashMap<K, V, L> {
    pub fn new() -> Self {
        Self {
            bins: Vec::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            size: 0,
            threshold: 0,
            f: 0.75,
            first: None,
            last: None,
            access_order: false,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_load_factor(capacity, 0.75)
    }
    pub fn with_capacity_and_load_factor(capacity: usize, f: f32) -> Self {
        assert!(f > 0.0 && !f.is_nan(), "invalid JDK load factor");
        let mut map = Self::new();
        map.threshold = capacity.clamp(1, 1 << 30).next_power_of_two();
        map.f = f;
        map
    }
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn capacity(&self) -> usize {
        self.bins.len()
    }
    pub(super) fn n(&self, id: usize) -> &Node<K, V> {
        self.nodes[id].as_ref().unwrap()
    }
    pub(super) fn n_mut(&mut self, id: usize) -> &mut Node<K, V> {
        self.nodes[id].as_mut().unwrap()
    }
    fn hash(key: &K) -> i32 {
        let h = key.java_hash_code() as u32;
        (h ^ (h >> 16)) as i32
    }
    fn find_tree(&self, root: usize, key: &K, hash: i32) -> Option<usize> {
        let n = self.n(root);
        if hash == n.hash && *key == n.key {
            return Some(root);
        }
        let order = hash
            .cmp(&n.hash)
            .then_with(|| key.java_compare(&n.key).unwrap_or(Ordering::Equal));
        match order {
            Ordering::Less => self.find_tree(n.tree.left?, key, hash),
            Ordering::Greater => self.find_tree(n.tree.right?, key, hash),
            Ordering::Equal => n
                .tree
                .right
                .and_then(|p| self.find_tree(p, key, hash))
                .or_else(|| n.tree.left.and_then(|p| self.find_tree(p, key, hash))),
        }
    }
    pub(super) fn find(&self, key: &K) -> Option<usize> {
        if self.bins.is_empty() {
            return None;
        }
        let hash = Self::hash(key);
        let b = &self.bins[hash as usize & (self.capacity() - 1)];
        if let Some(root) = b.root {
            self.find_tree(root, key, hash)
        } else {
            b.order
                .iter()
                .copied()
                .find(|&id| self.n(id).hash == hash && self.n(id).key == *key)
        }
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.find(key).is_some()
    }
    /// A lookup without LinkedHashMap access-order side effects (like containsKey).
    pub fn peek(&self, key: &K) -> Option<&V> {
        Some(&self.n(self.find(key)?).value)
    }
    /// Java get may move an accessed entry; use peek for read-only lookup.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let id = self.find(key)?;
        self.access(id);
        Some(&self.n(id).value)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let id = self.find(key)?;
        self.access(id);
        Some(&mut self.n_mut(id).value)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(id) = self.find(&key) {
            let old = mem::replace(&mut self.n_mut(id).value, value);
            self.access(id);
            return Some(old);
        }
        if self.bins.is_empty() {
            self.resize();
        }
        let hash = Self::hash(&key);
        let bin = hash as usize & (self.capacity() - 1);
        let node = Node {
            key,
            value,
            hash,
            tree: Tree::default(),
            prev: None,
            next: None,
        };
        let id = if let Some(id) = self.free.pop() {
            self.nodes[id] = Some(node);
            id
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        };
        if L {
            self.link_last(id);
        }
        if let Some(root) = self.bins[bin].root {
            let (parent, right) = self.tree_parent(root, id);
            self.set_child(parent, right, Some(id));
            self.n_mut(id).tree.parent = Some(parent);
            // putTreeVal splices immediately after its tree parent in bin traversal.
            let offset = self.bins[bin]
                .order
                .iter()
                .position(|&p| p == parent)
                .unwrap()
                + 1;
            self.bins[bin].order.insert(offset, id);
            let root = self.balance_insert(root, id);
            self.move_root(bin, root);
        } else {
            self.bins[bin].order.push(id);
            // putVal's binCount includes the old first node: treeify on ninth insertion.
            if self.bins[bin].order.len() >= 9 {
                if self.capacity() < 64 {
                    self.resize();
                } else {
                    self.treeify(bin);
                }
            }
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        None
    }
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
        self.remove_id(self.find(key)?, true)
    }
    pub(super) fn remove_id(&mut self, id: usize, movable: bool) -> Option<(K, V)> {
        let bin = self.n(id).hash as usize & (self.capacity() - 1);
        let pos = self.bins[bin].order.iter().position(|&p| p == id)?;
        self.bins[bin].order.remove(pos);
        if let Some(root) = self.bins[bin].root {
            if self.bins[bin].order.is_empty() {
                self.bins[bin].root = None;
            } else if movable
                && (self.child(root, true).is_none()
                    || self
                        .child(root, false)
                        .is_none_or(|left| self.child(left, false).is_none()))
            {
                self.bins[bin].root = None;
            } else {
                let root = self.delete_tree(root, id);
                if movable {
                    self.move_root(bin, root);
                } else {
                    self.bins[bin].root = Some(root);
                }
            }
        }
        if L {
            self.unlink(id);
        }
        let n = self.nodes[id].take().unwrap();
        self.free.push(id);
        self.size -= 1;
        Some((n.key, n.value))
    }
    pub fn clear(&mut self) {
        for b in &mut self.bins {
            b.order.clear();
            b.root = None;
        }
        self.nodes.clear();
        self.free.clear();
        self.size = 0;
        self.first = None;
        self.last = None;
    }
    fn resize(&mut self) {
        let old_cap = self.capacity();
        if old_cap >= 1 << 30 {
            self.threshold = i32::MAX as usize;
            return;
        }
        let old_threshold = self.threshold;
        let new_cap = if old_cap > 0 {
            old_cap * 2
        } else if old_threshold > 0 {
            old_threshold
        } else {
            16
        };
        self.threshold = if old_cap >= 16 && new_cap < 1 << 30 {
            old_threshold * 2
        } else if old_cap == 0 && old_threshold == 0 {
            12
        } else {
            let ft = new_cap as f32 * self.f;
            if new_cap < 1 << 30 && ft < (1usize << 30) as f32 {
                ft as usize
            } else {
                i32::MAX as usize
            }
        };
        let old = mem::replace(&mut self.bins, vec![Bin::default(); new_cap]);
        for (index, b) in old.into_iter().enumerate() {
            if b.order.is_empty() {
                continue;
            }
            if b.order.len() == 1 {
                let dest = self.n(b.order[0]).hash as usize & (new_cap - 1);
                self.bins[dest].order = b.order;
                self.bins[dest].root = b.root;
            } else {
                let (lo, hi): (Vec<_>, Vec<_>) = b
                    .order
                    .into_iter()
                    .partition(|&id| self.n(id).hash as usize & old_cap == 0);
                let split = !lo.is_empty() && !hi.is_empty();
                self.bins[index].order = lo;
                self.bins[index + old_cap].order = hi;
                if let Some(root) = b.root {
                    for dest in [index, index + old_cap] {
                        if self.bins[dest].order.len() > 6 {
                            if split {
                                self.treeify(dest);
                            } else {
                                self.bins[dest].root = Some(root);
                            }
                        }
                    }
                }
            }
        }
    }
    fn tree_parent(&self, root: usize, id: usize) -> (usize, bool) {
        let mut p = root;
        loop {
            let a = self.n(id);
            let b = self.n(p);
            let direction = a
                .hash
                .cmp(&b.hash)
                .then_with(|| a.key.java_compare(&b.key).unwrap_or(Ordering::Equal))
                .then(id.cmp(&p));
            let right = direction == Ordering::Greater;
            match self.child(p, right) {
                Some(c) => p = c,
                None => return (p, right),
            }
        }
    }
    fn treeify(&mut self, bin: usize) {
        let order = self.bins[bin].order.clone();
        let mut root = order[0];
        self.n_mut(root).tree = Tree::default();
        for &id in &order[1..] {
            self.n_mut(id).tree = Tree::default();
            let (parent, right) = self.tree_parent(root, id);
            self.set_child(parent, right, Some(id));
            self.n_mut(id).tree.parent = Some(parent);
            root = self.balance_insert(root, id);
        }
        self.move_root(bin, root);
    }
    fn move_root(&mut self, bin: usize, root: usize) {
        self.bins[bin].root = Some(root);
        let order = &mut self.bins[bin].order;
        let pos = order.iter().position(|&p| p == root).unwrap();
        if pos != 0 {
            order.remove(pos);
            order.insert(0, root);
        }
    }
    fn child(&self, id: usize, right: bool) -> Option<usize> {
        if right {
            self.n(id).tree.right
        } else {
            self.n(id).tree.left
        }
    }
    fn set_child(&mut self, id: usize, right: bool, child: Option<usize>) {
        if right {
            self.n_mut(id).tree.right = child;
        } else {
            self.n_mut(id).tree.left = child;
        }
    }
    fn red(&self, id: Option<usize>) -> bool {
        id.is_some_and(|p| self.n(p).tree.red)
    }
    // left=true rotates left; expressing both rotations together keeps their mirror invariants visible.
    fn rotate(&mut self, mut root: usize, p: usize, left: bool) -> usize {
        let r = self.child(p, left).unwrap();
        let inside = self.child(r, !left);
        self.set_child(p, left, inside);
        if let Some(inside) = inside {
            self.n_mut(inside).tree.parent = Some(p);
        }
        let parent = self.n(p).tree.parent;
        self.n_mut(r).tree.parent = parent;
        if let Some(parent) = parent {
            let side = self.child(parent, true) == Some(p);
            self.set_child(parent, side, Some(r));
        } else {
            root = r;
            self.n_mut(r).tree.red = false;
        }
        self.set_child(r, !left, Some(p));
        self.n_mut(p).tree.parent = Some(r);
        root
    }
    /// Port of HashMap.TreeNode#balanceInsertion, preserving its exact rotations.
    fn balance_insert(&mut self, mut root: usize, mut x: usize) -> usize {
        self.n_mut(x).tree.red = true;
        loop {
            let Some(mut parent) = self.n(x).tree.parent else {
                self.n_mut(x).tree.red = false;
                return x;
            };
            let Some(mut grand) = self.n(parent).tree.parent else {
                return root;
            };
            if !self.n(parent).tree.red {
                return root;
            }
            let parent_right = self.child(grand, true) == Some(parent);
            let uncle = self.child(grand, !parent_right);
            if self.red(uncle) {
                self.n_mut(uncle.unwrap()).tree.red = false;
                self.n_mut(parent).tree.red = false;
                self.n_mut(grand).tree.red = true;
                x = grand;
            } else {
                if self.child(parent, !parent_right) == Some(x) {
                    x = parent;
                    root = self.rotate(root, x, !parent_right);
                    parent = self.n(x).tree.parent.unwrap();
                    grand = self.n(parent).tree.parent.unwrap();
                }
                self.n_mut(parent).tree.red = false;
                self.n_mut(grand).tree.red = true;
                root = self.rotate(root, grand, parent_right);
            }
        }
    }
    /// Port of HashMap.TreeNode#balanceDeletion. Null children are black.
    fn balance_delete(&mut self, mut root: usize, mut x: usize) -> usize {
        loop {
            if x == root {
                return root;
            }
            let Some(mut parent) = self.n(x).tree.parent else {
                self.n_mut(x).tree.red = false;
                return x;
            };
            if self.n(x).tree.red {
                self.n_mut(x).tree.red = false;
                return root;
            }
            let x_right = self.child(parent, true) == Some(x);
            let mut sibling = self.child(parent, !x_right);
            if self.red(sibling) {
                self.n_mut(sibling.unwrap()).tree.red = false;
                self.n_mut(parent).tree.red = true;
                root = self.rotate(root, parent, !x_right);
                parent = self.n(x).tree.parent.unwrap();
                sibling = self.child(parent, !x_right);
            }
            let Some(mut sib) = sibling else {
                x = parent;
                continue;
            };
            let near = self.child(sib, x_right);
            let far = self.child(sib, !x_right);
            if !self.red(near) && !self.red(far) {
                self.n_mut(sib).tree.red = true;
                x = parent;
                continue;
            }
            if !self.red(far) {
                if let Some(near) = near {
                    self.n_mut(near).tree.red = false;
                }
                self.n_mut(sib).tree.red = true;
                root = self.rotate(root, sib, x_right);
                parent = self.n(x).tree.parent.unwrap();
                sib = self.child(parent, !x_right).unwrap();
            }
            self.n_mut(sib).tree.red = self.n(parent).tree.red;
            if let Some(far) = self.child(sib, !x_right) {
                self.n_mut(far).tree.red = false;
            }
            self.n_mut(parent).tree.red = false;
            root = self.rotate(root, parent, !x_right);
            x = root;
        }
    }
    /// Port of removeTreeNode's structural deletion. Node payloads stay pinned,
    /// because bin traversal and LinkedHashMap insertion links refer to the nodes.
    fn delete_tree(&mut self, mut root: usize, p: usize) -> usize {
        let t = self.n(p).tree;
        let replacement = match (t.left, t.right) {
            (Some(left), Some(right)) => {
                let mut s = right;
                while let Some(next) = self.child(s, false) {
                    s = next;
                }
                let st = self.n(s).tree;
                self.n_mut(s).tree.red = t.red;
                self.n_mut(p).tree.red = st.red;
                if s == right {
                    self.n_mut(p).tree.parent = Some(s);
                    self.n_mut(s).tree.right = Some(p);
                } else {
                    self.n_mut(p).tree.parent = st.parent;
                    let sp = st.parent.unwrap();
                    let side = self.child(sp, true) == Some(s);
                    self.set_child(sp, side, Some(p));
                    self.n_mut(s).tree.right = Some(right);
                    self.n_mut(right).tree.parent = Some(s);
                }
                self.n_mut(p).tree.left = None;
                self.n_mut(p).tree.right = st.right;
                if let Some(sr) = st.right {
                    self.n_mut(sr).tree.parent = Some(p);
                }
                self.n_mut(s).tree.left = Some(left);
                self.n_mut(left).tree.parent = Some(s);
                self.n_mut(s).tree.parent = t.parent;
                if let Some(pp) = t.parent {
                    let side = self.child(pp, true) == Some(p);
                    self.set_child(pp, side, Some(s));
                } else {
                    root = s;
                }
                st.right.unwrap_or(p)
            }
            (Some(child), None) | (None, Some(child)) => child,
            (None, None) => p,
        };
        if replacement != p {
            let parent = self.n(p).tree.parent;
            self.n_mut(replacement).tree.parent = parent;
            if let Some(parent) = parent {
                let side = self.child(parent, true) == Some(p);
                self.set_child(parent, side, Some(replacement));
            } else {
                root = replacement;
                self.n_mut(replacement).tree.red = false;
            }
            let color = self.n(p).tree.red;
            self.n_mut(p).tree = Tree {
                red: color,
                ..Tree::default()
            };
        }
        if !self.n(p).tree.red {
            root = self.balance_delete(root, replacement);
        }
        if replacement == p {
            if let Some(parent) = self.n(p).tree.parent {
                let side = self.child(parent, true) == Some(p);
                self.set_child(parent, side, None);
            }
            self.n_mut(p).tree.parent = None;
        }
        root
    }
    pub(super) fn link_last(&mut self, id: usize) {
        self.n_mut(id).prev = self.last;
        self.n_mut(id).next = None;
        if let Some(last) = self.last {
            self.n_mut(last).next = Some(id);
        } else {
            self.first = Some(id);
        }
        self.last = Some(id);
    }
    pub(super) fn unlink(&mut self, id: usize) {
        let (prev, next) = (self.n(id).prev, self.n(id).next);
        if let Some(prev) = prev {
            self.n_mut(prev).next = next;
        } else {
            self.first = next;
        }
        if let Some(next) = next {
            self.n_mut(next).prev = prev;
        } else {
            self.last = prev;
        }
    }
    fn access(&mut self, id: usize) {
        if L && self.access_order && self.last != Some(id) {
            self.unlink(id);
            self.link_last(id);
        }
    }
    pub fn iter(&self) -> JavaIter<'_, K, V, L> {
        JavaIter {
            map: self,
            bin: 0,
            offset: 0,
            next: self.first,
            remaining: self.size,
        }
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> + ExactSizeIterator {
        self.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl Iterator<Item = &V> + ExactSizeIterator {
        self.iter().map(|(_, v)| v)
    }
    /// Removals through Java iterators never resize or move a tree root to front.
    pub fn cursor(&mut self) -> JavaCursor<'_, K, V, L> {
        let ids = if L {
            let mut ids = Vec::with_capacity(self.size);
            let mut next = self.first;
            while let Some(id) = next {
                ids.push(id);
                next = self.n(id).next;
            }
            ids
        } else {
            self.bins
                .iter()
                .flat_map(|b| b.order.iter().copied())
                .collect()
        };
        JavaCursor {
            map: self,
            ids: ids.into_iter(),
            current: None,
        }
    }
}
impl<K: Eq + JavaHash, V> JavaHashMap<K, V, true> {
    pub fn with_access_order(capacity: usize, f: f32, access_order: bool) -> Self {
        let mut map = Self::with_capacity_and_load_factor(capacity, f);
        map.access_order = access_order;
        map
    }
    pub fn first(&self) -> Option<(&K, &V)> {
        let n = self.n(self.first?);
        Some((&n.key, &n.value))
    }
    pub fn last(&self) -> Option<(&K, &V)> {
        let n = self.n(self.last?);
        Some((&n.key, &n.value))
    }
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        self.remove_id(self.first?, true)
    }
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        self.remove_id(self.last?, true)
    }
    pub fn insert_last(&mut self, key: K, value: V) -> Option<V> {
        let existing = self.find(&key);
        let old = self.insert(key, value);
        if let Some(id) = existing {
            if self.last != Some(id) {
                self.unlink(id);
                self.link_last(id);
            }
        }
        old
    }
    pub fn insert_first(&mut self, key: K, value: V) -> Option<V> {
        let existing = self.find(&key);
        let old = self.insert(key, value);
        let id = existing.or(self.last).unwrap();
        self.unlink(id);
        self.n_mut(id).prev = None;
        self.n_mut(id).next = self.first;
        if let Some(first) = self.first {
            self.n_mut(first).prev = Some(id);
        } else {
            self.last = Some(id);
        }
        self.first = Some(id);
        old
    }
}
pub struct JavaIter<'a, K, V, const L: bool> {
    map: &'a JavaHashMap<K, V, L>,
    bin: usize,
    offset: usize,
    next: Option<usize>,
    remaining: usize,
}
impl<'a, K: Eq + JavaHash, V, const L: bool> Iterator for JavaIter<'a, K, V, L> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let id = if L {
            let id = self.next?;
            self.next = self.map.n(id).next;
            id
        } else {
            loop {
                let bin = self.map.bins.get(self.bin)?;
                if let Some(&id) = bin.order.get(self.offset) {
                    self.offset += 1;
                    break id;
                }
                self.bin += 1;
                self.offset = 0;
            }
        };
        self.remaining -= 1;
        let n = self.map.n(id);
        Some((&n.key, &n.value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K: Eq + JavaHash, V, const L: bool> ExactSizeIterator for JavaIter<'_, K, V, L> {}
impl<K: Eq + JavaHash, V, const L: bool> std::iter::FusedIterator for JavaIter<'_, K, V, L> {}
impl<'a, K: Eq + JavaHash, V, const L: bool> IntoIterator for &'a JavaHashMap<K, V, L> {
    type Item = (&'a K, &'a V);
    type IntoIter = JavaIter<'a, K, V, L>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<K: Eq + JavaHash, V, const L: bool> IntoIterator for JavaHashMap<K, V, L> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(mut self) -> Self::IntoIter {
        let ids = if L {
            let mut ids = Vec::with_capacity(self.size);
            let mut next = self.first;
            while let Some(id) = next {
                ids.push(id);
                next = self.n(id).next;
            }
            ids
        } else {
            self.bins
                .iter()
                .flat_map(|b| b.order.iter().copied())
                .collect()
        };
        ids.into_iter()
            .map(|id| {
                let n = self.nodes[id].take().unwrap();
                (n.key, n.value)
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
pub struct JavaCursor<'a, K, V, const L: bool> {
    map: &'a mut JavaHashMap<K, V, L>,
    ids: std::vec::IntoIter<usize>,
    current: Option<usize>,
}
impl<K: Eq + JavaHash, V, const L: bool> JavaCursor<'_, K, V, L> {
    pub fn next_entry(&mut self) -> Option<(&K, &V)> {
        self.current = self.ids.next();
        let n = self.map.n(self.current?);
        Some((&n.key, &n.value))
    }
    pub fn remove(&mut self) -> Option<(K, V)> {
        self.map.remove_id(self.current.take()?, false)
    }
}
/// Port of java.util.HashSet/LinkedHashSet, backed by the corresponding Java map.
#[derive(Clone, Debug)]
pub struct JavaHashSet<K, const L: bool = false>(pub(super) JavaHashMap<K, (), L>);
impl<K: Eq + JavaHash, const L: bool> Default for JavaHashSet<K, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: Eq + JavaHash, const L: bool> JavaHashSet<K, L> {
    pub fn new() -> Self {
        Self(JavaHashMap::new())
    }
    pub fn with_capacity(n: usize) -> Self {
        Self(JavaHashMap::with_capacity(n))
    }
    pub fn with_capacity_and_load_factor(n: usize, f: f32) -> Self {
        Self(JavaHashMap::with_capacity_and_load_factor(n, f))
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
    pub fn remove(&mut self, key: &K) -> bool {
        self.0.remove(key).is_some()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = &K> + ExactSizeIterator {
        self.0.keys()
    }
    pub fn cursor(&mut self) -> JavaCursor<'_, K, (), L> {
        self.0.cursor()
    }
}
impl<K: Eq + JavaHash> JavaHashSet<K, true> {
    pub fn insert_first(&mut self, key: K) -> bool {
        self.0.insert_first(key, ()).is_none()
    }
    pub fn insert_last(&mut self, key: K) -> bool {
        self.0.insert_last(key, ()).is_none()
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
}
impl<K: Eq + JavaHash, const L: bool> IntoIterator for JavaHashSet<K, L> {
    type Item = K;
    type IntoIter = std::iter::Map<std::vec::IntoIter<(K, ())>, fn((K, ())) -> K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().map(|(k, _)| k)
    }
}
impl<'a, K: Eq + JavaHash, const L: bool> IntoIterator for &'a JavaHashSet<K, L> {
    type Item = &'a K;
    type IntoIter = std::iter::Map<JavaIter<'a, K, (), L>, fn((&'a K, &'a ())) -> &'a K>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, _)| k)
    }
}
