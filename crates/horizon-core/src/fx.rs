//! The Fx hash (rustc's) for hot maps and sets keyed by small integers, such
//! as positions. Only for maps whose iteration order carries no meaning: the
//! default hasher's order is random anyway.
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Clone, Copy, Default)]
pub struct FxHasher(u64);
impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u32(u32::from(b));
        }
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.0 = (self.0.rotate_left(5) ^ u64::from(i)).wrapping_mul(0x51_7cc1_b727_220a_95);
    }
    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.write_u32(i as u32);
        self.write_u32((i >> 32) as u32);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}
pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuild>;
pub type FxHashSet<K> = std::collections::HashSet<K, FxBuild>;
