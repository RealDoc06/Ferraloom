/// Lookup and traversal semantics of fastutil's `LongOpenHashSet`. Lighting
/// exposes its traversal through node checks and section-update notifications.
#[derive(Debug)]
pub(crate) struct LongSet {
    keys: Vec<i64>,
    size: usize,
    zero: bool,
    zero_key: i64,
    min_capacity: usize,
    load_quarters: usize,
}
impl Default for LongSet {
    fn default() -> Self {
        Self::new(16, 3)
    }
}
impl LongSet {
    pub fn new(expected: usize, load_quarters: usize) -> Self {
        let capacity = (expected * 4)
            .div_ceil(load_quarters)
            .next_power_of_two()
            .max(2);
        Self {
            keys: vec![0; capacity],
            size: 0,
            zero: false,
            zero_key: 0,
            min_capacity: capacity,
            load_quarters,
        }
    }
    fn max_fill(&self) -> usize {
        (self.keys.len() * self.load_quarters)
            .div_ceil(4)
            .min(self.keys.len() - 1)
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn contains(&self, key: &i64) -> bool {
        if *key == 0 {
            return self.zero;
        }
        let mask = self.keys.len() - 1;
        let mut slot = mix(*key) & mask;
        while self.keys[slot] != 0 {
            if self.keys[slot] == *key {
                return true;
            }
            slot = (slot + 1) & mask;
        }
        false
    }
    pub fn insert(&mut self, key: i64) -> bool {
        if key == 0 {
            if self.zero {
                return false;
            }
            self.zero = true;
        } else {
            let mask = self.keys.len() - 1;
            let mut slot = mix(key) & mask;
            while self.keys[slot] != 0 {
                if self.keys[slot] == key {
                    return false;
                }
                slot = (slot + 1) & mask;
            }
            self.keys[slot] = key;
        }
        let grow = self.size >= self.max_fill();
        self.size += 1;
        if grow {
            let capacity = ((self.size + 1) * 4)
                .div_ceil(self.load_quarters)
                .next_power_of_two();
            self.rehash(capacity);
        }
        true
    }
    pub fn remove(&mut self, key: &i64) -> bool {
        if *key == 0 {
            if !self.zero {
                return false;
            }
            self.zero = false;
        } else {
            let mask = self.keys.len() - 1;
            let mut slot = mix(*key) & mask;
            loop {
                if self.keys[slot] == 0 {
                    return false;
                }
                if self.keys[slot] == *key {
                    break;
                }
                slot = (slot + 1) & mask;
            }
            self.shift_keys(slot);
        }
        self.size -= 1;
        if self.keys.len() > self.min_capacity
            && self.size < self.max_fill() / 4
            && self.keys.len() > 16
        {
            self.rehash(self.keys.len() / 2);
        }
        true
    }
    fn shift_keys(&mut self, mut last: usize) {
        let mask = self.keys.len() - 1;
        loop {
            let mut pos = (last + 1) & mask;
            let key = loop {
                let key = self.keys[pos];
                if key == 0 {
                    self.keys[last] = 0;
                    return;
                }
                let slot = mix(key) & mask;
                let crosses_gap = if last <= pos {
                    last >= slot || slot > pos
                } else {
                    last >= slot && slot > pos
                };
                if crosses_gap {
                    break key;
                }
                pos = (pos + 1) & mask;
            };
            self.keys[last] = key;
            last = pos;
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &i64> {
        std::iter::once(&self.zero_key)
            .take(usize::from(self.zero))
            .chain(self.keys.iter().rev().filter(|k| **k != 0))
    }
    pub fn drain(&mut self) -> std::vec::IntoIter<i64> {
        let keys: Vec<_> = self.iter().copied().collect();
        self.keys.fill(0);
        self.size = 0;
        self.zero = false;
        keys.into_iter()
    }
    pub fn clear_and_trim(&mut self, expected: usize) -> Vec<i64> {
        let result = self.drain().collect();
        let capacity = (expected * 4)
            .div_ceil(self.load_quarters)
            .next_power_of_two()
            .max(2);
        if capacity < self.keys.len() {
            self.rehash(capacity);
        }
        result
    }
    fn rehash(&mut self, capacity: usize) {
        let mut next = vec![0; capacity];
        for key in self.keys.iter().rev().copied().filter(|k| *k != 0) {
            let mut slot = mix(key) & (capacity - 1);
            while next[slot] != 0 {
                slot = (slot + 1) & (capacity - 1);
            }
            next[slot] = key;
        }
        self.keys = next;
    }
}
fn mix(key: i64) -> usize {
    let h = (key as u64).wrapping_mul(0x9e3779b97f4a7c15);
    let h = h ^ (h >> 32);
    (h ^ (h >> 16)) as usize
}
