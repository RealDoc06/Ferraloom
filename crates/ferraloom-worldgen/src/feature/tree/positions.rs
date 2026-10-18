use super::Pos;
use ferraloom_util::{BlockPos, collections::JavaHashSet};

/// Vanilla HashSet<BlockPos> order matters when decorators stably sort by Y.
#[derive(Debug, Default)]
pub(super) struct Positions(JavaHashSet<BlockPos>);

impl Positions {
    pub fn insert(&mut self, [x, y, z]: Pos) {
        self.0.insert(BlockPos::new(x, y, z));
    }
    pub fn contains(&self, [x, y, z]: Pos) -> bool {
        self.0.contains(&BlockPos::new(x, y, z))
    }
    pub fn pop_first(&mut self) -> Option<Pos> {
        // Iterator.remove preserves a tree bin's root position; Set.remove may
        // move it. TreeFeature creates a fresh iterator for every queued node.
        let mut iterator = self.0.cursor();
        iterator.next_entry()?;
        let (p, ()) = iterator.remove()?;
        Some([p.x, p.y, p.z])
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = Pos> + '_ {
        self.0.iter().map(|p| [p.x, p.y, p.z])
    }
    pub fn sorted(&self) -> Vec<Pos> {
        let mut p = self.iter().collect::<Vec<_>>();
        p.sort_by_key(|p| p[1]);
        p
    }
}
