use super::PathType;
use ferraloom_util::{BlockPos, collections::mix_long};

/// Port of PathTypeCache. Lookup and insertion are separate so classification
/// can call the world after ending the canonical cache borrow.
#[derive(Debug)]
pub struct PathTypeCache {
    positions: Box<[i64; 4096]>,
    path_types: Box<[Option<PathType>; 4096]>,
}
impl Default for PathTypeCache {
    fn default() -> Self {
        Self {
            positions: Box::new([0; 4096]),
            path_types: Box::new([None; 4096]),
        }
    }
}
impl PathTypeCache {
    /// Port of PathTypeCache#get; a matching initial zero key is still empty.
    pub fn get(&self, pos: BlockPos) -> Option<PathType> {
        let key = pos.as_long();
        let index = index(key);
        if self.positions[index] == key {
            self.path_types[index]
        } else {
            None
        }
    }
    /// Port of PathTypeCache#compute's write, after bounded classification ends.
    pub fn record_computed(&mut self, pos: BlockPos, path_type: PathType) {
        let key = pos.as_long();
        let index = index(key);
        self.positions[index] = key;
        self.path_types[index] = Some(path_type);
    }
    /// Port of PathTypeCache#invalidate: a colliding different key is retained.
    pub fn invalidate(&mut self, pos: BlockPos) {
        let key = pos.as_long();
        let index = index(key);
        if self.positions[index] == key {
            self.path_types[index] = None;
        }
    }
}
fn index(pos: i64) -> usize {
    mix_long(pos) as usize & 4095
}
