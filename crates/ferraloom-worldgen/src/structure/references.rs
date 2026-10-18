//! Structure reference sets use fastutil iteration order, not insertion or
//! numeric order. That order feeds piece placement and float density sums.

/// Port of the LongOpenHashSet subset used by ChunkAccess structure references.
/// Zero has a separate slot; other keys use linear probing at load factor 3/4.
#[derive(Clone, Debug)]
pub struct StructureReferences {
    keys: Vec<i64>,
    len: usize,
    has_zero: bool,
}
impl Default for StructureReferences {
    fn default() -> Self {
        Self::with_expected(16)
    }
}
impl StructureReferences {
    fn with_expected(expected: usize) -> Self {
        let capacity = ((expected * 4).div_ceil(3)).max(2).next_power_of_two();
        Self {
            keys: vec![0; capacity],
            len: 0,
            has_zero: false,
        }
    }
    /// The array constructor reserves by input length, including duplicates.
    pub fn from_array(values: &[i64]) -> Self {
        let mut set = Self::with_expected(values.len());
        for &value in values {
            set.insert(value);
        }
        set
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn contains(&self, key: i64) -> bool {
        if key == 0 {
            return self.has_zero;
        }
        let mut slot = mix(key) as usize & (self.keys.len() - 1);
        while self.keys[slot] != 0 {
            if self.keys[slot] == key {
                return true;
            }
            slot = (slot + 1) & (self.keys.len() - 1);
        }
        false
    }
    pub fn insert(&mut self, key: i64) -> bool {
        if key == 0 {
            if self.has_zero {
                return false;
            }
            self.has_zero = true;
        } else {
            let mut slot = mix(key) as usize & (self.keys.len() - 1);
            while self.keys[slot] != 0 {
                if self.keys[slot] == key {
                    return false;
                }
                slot = (slot + 1) & (self.keys.len() - 1);
            }
            self.keys[slot] = key;
        }
        let old_len = self.len;
        self.len += 1;
        if old_len >= (self.keys.len() * 3).div_ceil(4).min(self.keys.len() - 1) {
            let capacity = (((self.len + 1) * 4).div_ceil(3))
                .max(2)
                .next_power_of_two();
            let old_keys = std::mem::replace(&mut self.keys, vec![0; capacity]);
            for key in old_keys.into_iter().rev().filter(|&key| key != 0) {
                let mut slot = mix(key) as usize & (capacity - 1);
                while self.keys[slot] != 0 {
                    slot = (slot + 1) & (capacity - 1);
                }
                self.keys[slot] = key;
            }
        }
        true
    }
    pub fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        self.has_zero
            .then_some(0)
            .into_iter()
            .chain(self.keys.iter().rev().copied().filter(|&key| key != 0))
    }
}
fn mix(key: i64) -> u64 {
    let mut hash = (key as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    hash ^= hash >> 32;
    hash ^ (hash >> 16)
}

#[cfg(test)]
#[path = "references_tests.rs"]
mod tests;
