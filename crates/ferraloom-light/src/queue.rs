use ferraloom_util::core::Direction;

/// Port of `LightEngine.QueueEntry`. Bits 0..4 hold light, 4..10 directions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct QueueEntry(u16);

impl QueueEntry {
    const DIRECTIONS: u16 = 1008;
    const EMPTY: u16 = 1024;
    const EMISSION: u16 = 2048;

    pub fn decrease(level: u8) -> Self {
        Self(Self::DIRECTIONS | u16::from(level))
    }
    pub fn decrease_skip(level: u8, skip: Direction) -> Self {
        Self(Self::decrease(level).0 & !(1 << (skip as u8 + 4)))
    }
    pub fn emission(level: u8, empty: bool) -> Self {
        Self(Self::decrease(level).0 | Self::EMISSION | if empty { Self::EMPTY } else { 0 })
    }
    pub fn increase_skip(level: u8, empty: bool, skip: Direction) -> Self {
        Self(Self::decrease_skip(level, skip).0 | if empty { Self::EMPTY } else { 0 })
    }
    pub fn only(level: u8, direction: Direction) -> Self {
        Self(u16::from(level) | 1 << (direction as u8 + 4))
    }
    pub fn sky(directions: [bool; 5]) -> Self {
        let mut entry = Self(15);
        for (enabled, direction) in directions.into_iter().zip([
            Direction::Down,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ]) {
            if enabled {
                entry.0 |= 1 << (direction as u8 + 4);
            }
        }
        entry
    }
    pub fn level(self) -> u8 {
        (self.0 & 15) as u8
    }
    pub fn empty(self) -> bool {
        self.0 & Self::EMPTY != 0
    }
    pub fn is_from_emission(self) -> bool {
        self.0 & Self::EMISSION != 0
    }
    pub fn allows(self, direction: Direction) -> bool {
        self.0 & (1 << (direction as u8 + 4)) != 0
    }
}

/// The pending-node set uses fastutil's hash traversal, including its zero slot.
/// Port of `LongOpenHashSet` with load factor 0.5 and initial expected size 512.
#[derive(Debug)]
pub(crate) struct BlockNodes {
    keys: Vec<i64>,
    size: usize,
    zero: bool,
}
impl Default for BlockNodes {
    fn default() -> Self {
        Self {
            keys: vec![0; 1024],
            size: 0,
            zero: false,
        }
    }
}
impl BlockNodes {
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn insert(&mut self, key: i64) {
        if key == 0 {
            if self.zero {
                return;
            }
            self.zero = true;
        } else {
            let mask = self.keys.len() - 1;
            let mut slot = mix(key) & mask;
            while self.keys[slot] != 0 {
                if self.keys[slot] == key {
                    return;
                }
                slot = (slot + 1) & mask;
            }
            self.keys[slot] = key;
        }
        let old_size = self.size;
        self.size += 1;
        if old_size >= self.keys.len() / 2 {
            self.rehash(self.keys.len() * 2);
        }
    }
    pub fn take(&mut self) -> Vec<i64> {
        let mut result = Vec::with_capacity(self.size);
        if self.zero {
            result.push(0);
        }
        result.extend(self.keys.iter().rev().copied().filter(|k| *k != 0));
        self.keys = vec![0; 1024];
        self.zero = false;
        self.size = 0;
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
