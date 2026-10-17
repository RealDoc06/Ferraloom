use crate::NbtString;

/// Iteration order of vanilla's Java HashMap<String, Tag>, including collision tree bins.
/// Values remain in CompoundTag's BTreeMap; only observable iteration is reproduced here.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyOrder {
    buckets: Vec<Bucket>,
    len: usize,
}
#[derive(Clone, Debug, Default)]
struct Bucket {
    nodes: Vec<Node>,
    order: Vec<usize>,
    root: Option<usize>,
    tree: bool,
}
#[derive(Clone, Debug)]
struct Node {
    key: NbtString,
    hash: i32,
    parent: Option<usize>,
    children: [Option<usize>; 2],
    red: bool,
}
fn hash(key: &NbtString) -> i32 {
    let hash = key
        .as_utf16()
        .iter()
        .fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(u32::from(*c)));
    (hash ^ (hash >> 16)) as i32
}
impl KeyOrder {
    pub(crate) fn keys(&self) -> Vec<&NbtString> {
        self.buckets
            .iter()
            .flat_map(|b| b.order.iter().map(|i| &b.nodes[*i].key))
            .collect()
    }
    pub(crate) fn insert(&mut self, key: NbtString) {
        if self.buckets.is_empty() {
            self.buckets.resize_with(16, Bucket::default)
        }
        let index = hash(&key) as usize & (self.buckets.len() - 1);
        let bucket = &mut self.buckets[index];
        bucket.insert(key);
        let treeify = !bucket.tree && bucket.order.len() >= 9;
        if treeify {
            if self.buckets.len() < 64 {
                self.resize()
            } else {
                self.buckets[index].treeify()
            }
        }
        self.len += 1;
        if self.len > self.buckets.len() * 3 / 4 {
            self.resize()
        }
    }
    pub(crate) fn remove(&mut self, key: &NbtString) {
        if self.buckets.is_empty() {
            return;
        }
        let index = hash(key) as usize & (self.buckets.len() - 1);
        if self.buckets[index].remove(key) {
            self.len -= 1;
            self.buckets[index].compact();
        }
    }
    fn resize(&mut self) {
        let capacity = self.buckets.len();
        let old = std::mem::take(&mut self.buckets);
        self.buckets.resize_with(capacity * 2, Bucket::default);
        for (index, bucket) in old.into_iter().enumerate() {
            let (low, high) = bucket.split(capacity);
            self.buckets[index] = low;
            self.buckets[index + capacity] = high;
        }
    }
}
impl Bucket {
    fn insert(&mut self, key: NbtString) {
        let index = self.nodes.len();
        self.nodes.push(Node {
            hash: hash(&key),
            key,
            parent: None,
            children: [None, None],
            red: false,
        });
        if self.tree {
            let parent = self.link(index);
            let offset = parent
                .and_then(|p| self.order.iter().position(|i| *i == p))
                .map_or(self.order.len(), |i| i + 1);
            self.order.insert(offset, index);
            self.front();
        } else {
            self.order.push(index)
        }
    }
    fn front(&mut self) {
        if let Some(root) = self.root
            && let Some(index) = self.order.iter().position(|i| *i == root)
        {
            self.order.remove(index);
            self.order.insert(0, root);
        }
    }
    fn treeify(&mut self) {
        self.tree = true;
        self.root = None;
        for i in self.order.clone() {
            self.nodes[i].parent = None;
            self.nodes[i].children = [None, None];
            self.nodes[i].red = false;
            self.link(i);
        }
        self.front();
    }
    fn link(&mut self, index: usize) -> Option<usize> {
        let Some(mut parent) = self.root else {
            self.root = Some(index);
            return None;
        };
        loop {
            let side = usize::from(
                (self.nodes[index].hash, &self.nodes[index].key)
                    > (self.nodes[parent].hash, &self.nodes[parent].key),
            );
            if let Some(child) = self.nodes[parent].children[side] {
                parent = child;
            } else {
                self.nodes[parent].children[side] = Some(index);
                self.nodes[index].parent = Some(parent);
                self.balance_insert(index);
                return Some(parent);
            }
        }
    }
    fn red(&self, index: Option<usize>) -> bool {
        index.is_some_and(|i| self.nodes[i].red)
    }
    fn color(&mut self, index: Option<usize>, red: bool) {
        if let Some(i) = index {
            self.nodes[i].red = red
        }
    }
    fn rotate(&mut self, p: usize, side: usize) {
        let r = self.nodes[p].children[side].unwrap();
        let middle = self.nodes[r].children[1 - side];
        self.nodes[p].children[side] = middle;
        if let Some(middle) = middle {
            self.nodes[middle].parent = Some(p)
        }
        let parent = self.nodes[p].parent;
        self.nodes[r].parent = parent;
        if let Some(parent) = parent {
            let slot = usize::from(self.nodes[parent].children[1] == Some(p));
            self.nodes[parent].children[slot] = Some(r);
        } else {
            self.root = Some(r);
            self.nodes[r].red = false;
        }
        self.nodes[r].children[1 - side] = Some(p);
        self.nodes[p].parent = Some(r);
    }
    fn balance_insert(&mut self, mut x: usize) {
        self.nodes[x].red = true;
        loop {
            let Some(mut parent) = self.nodes[x].parent else {
                self.nodes[x].red = false;
                self.root = Some(x);
                return;
            };
            if !self.nodes[parent].red {
                return;
            }
            let Some(mut grand) = self.nodes[parent].parent else {
                return;
            };
            let side = usize::from(self.nodes[grand].children[1] == Some(parent));
            let uncle = self.nodes[grand].children[1 - side];
            if self.red(uncle) {
                self.color(uncle, false);
                self.nodes[parent].red = false;
                self.nodes[grand].red = true;
                x = grand;
                continue;
            }
            if self.nodes[parent].children[1 - side] == Some(x) {
                x = parent;
                self.rotate(x, 1 - side);
                parent = self.nodes[x].parent.unwrap();
                grand = self.nodes[parent].parent.unwrap();
            }
            self.nodes[parent].red = false;
            self.nodes[grand].red = true;
            self.rotate(grand, side);
        }
    }
    fn balance_delete(&mut self, mut x: usize) {
        loop {
            if Some(x) == self.root {
                return;
            }
            let Some(parent) = self.nodes[x].parent else {
                self.nodes[x].red = false;
                return;
            };
            if self.nodes[x].red {
                self.nodes[x].red = false;
                return;
            }
            let side = usize::from(self.nodes[parent].children[1] == Some(x));
            let mut sibling = self.nodes[parent].children[1 - side];
            if self.red(sibling) {
                self.color(sibling, false);
                self.nodes[parent].red = true;
                self.rotate(parent, 1 - side);
                sibling = self.nodes[parent].children[1 - side];
            }
            let Some(s) = sibling else {
                x = parent;
                continue;
            };
            let near = self.nodes[s].children[side];
            let far = self.nodes[s].children[1 - side];
            if !self.red(near) && !self.red(far) {
                self.nodes[s].red = true;
                x = parent;
                continue;
            }
            if !self.red(far) {
                self.color(near, false);
                self.nodes[s].red = true;
                self.rotate(s, side);
                sibling = self.nodes[parent].children[1 - side];
            }
            if let Some(s) = sibling {
                self.nodes[s].red = self.nodes[parent].red;
                let far = self.nodes[s].children[1 - side];
                self.color(far, false);
            }
            self.nodes[parent].red = false;
            self.rotate(parent, 1 - side);
            x = self.root.unwrap();
        }
    }
    fn remove(&mut self, key: &NbtString) -> bool {
        let Some(offset) = self.order.iter().position(|i| self.nodes[*i].key == *key) else {
            return false;
        };
        let p = self.order.remove(offset);
        if !self.tree || self.order.is_empty() {
            return true;
        }
        let root = self.root.unwrap();
        let left = self.nodes[root].children[0];
        if self.nodes[root].children[1].is_none()
            || left.is_none_or(|l| self.nodes[l].children[0].is_none())
        {
            self.tree = false;
            self.root = None;
            return true;
        }
        let [left, right] = self.nodes[p].children;
        let replacement = if let (Some(left), Some(right)) = (left, right) {
            let mut s = right;
            while let Some(l) = self.nodes[s].children[0] {
                s = l
            }
            let red = self.nodes[s].red;
            self.nodes[s].red = self.nodes[p].red;
            self.nodes[p].red = red;
            let sr = self.nodes[s].children[1];
            let pp = self.nodes[p].parent;
            if s == right {
                self.nodes[p].parent = Some(s);
                self.nodes[s].children[1] = Some(p);
            } else {
                let sp = self.nodes[s].parent.unwrap();
                self.nodes[p].parent = Some(sp);
                let side = usize::from(self.nodes[sp].children[1] == Some(s));
                self.nodes[sp].children[side] = Some(p);
                self.nodes[s].children[1] = Some(right);
                self.nodes[right].parent = Some(s);
            }
            self.nodes[p].children = [None, sr];
            if let Some(sr) = sr {
                self.nodes[sr].parent = Some(p)
            }
            self.nodes[s].children[0] = Some(left);
            self.nodes[left].parent = Some(s);
            self.nodes[s].parent = pp;
            if let Some(pp) = pp {
                let side = usize::from(self.nodes[pp].children[1] == Some(p));
                self.nodes[pp].children[side] = Some(s);
            } else {
                self.root = Some(s);
            }
            sr.unwrap_or(p)
        } else {
            left.or(right).unwrap_or(p)
        };
        if replacement != p {
            let parent = self.nodes[p].parent;
            self.nodes[replacement].parent = parent;
            if let Some(parent) = parent {
                let side = usize::from(self.nodes[parent].children[1] == Some(p));
                self.nodes[parent].children[side] = Some(replacement);
            } else {
                self.root = Some(replacement);
                self.nodes[replacement].red = false;
            }
            self.nodes[p].children = [None, None];
            self.nodes[p].parent = None;
        }
        if !self.nodes[p].red {
            self.balance_delete(replacement)
        }
        if replacement == p {
            if let Some(parent) = self.nodes[p].parent {
                let side = usize::from(self.nodes[parent].children[1] == Some(p));
                self.nodes[parent].children[side] = None;
            }
            self.nodes[p].parent = None;
        }
        self.front();
        true
    }
    fn compact(&mut self) {
        if self.nodes.len() <= self.order.len() * 2 + 8 {
            return;
        }
        let mut indices = vec![None; self.nodes.len()];
        for (new, old) in self.order.iter().enumerate() {
            indices[*old] = Some(new);
        }
        let nodes = self
            .order
            .iter()
            .map(|old| {
                let mut node = self.nodes[*old].clone();
                node.parent = node.parent.and_then(|i| indices[i]);
                node.children = node.children.map(|i| i.and_then(|i| indices[i]));
                node
            })
            .collect();
        self.root = self.root.and_then(|i| indices[i]);
        self.nodes = nodes;
        self.order = (0..self.nodes.len()).collect();
    }
    fn split(mut self, bit: usize) -> (Self, Self) {
        let (low, high): (Vec<_>, Vec<_>) = self
            .order
            .iter()
            .copied()
            .partition(|i| self.nodes[*i].hash as usize & bit == 0);
        if low.is_empty() || high.is_empty() {
            if self.order.len() <= 6 {
                self.tree = false;
                self.root = None;
            }
            return if low.is_empty() {
                (Self::default(), self)
            } else {
                (self, Self::default())
            };
        }
        let build = |order: Vec<usize>| {
            let mut b = Self::default();
            for i in order {
                b.insert(self.nodes[i].key.clone());
            }
            if self.tree && b.order.len() > 6 {
                b.treeify()
            }
            b
        };
        (build(low), build(high))
    }
}
