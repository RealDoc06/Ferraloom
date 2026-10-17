use super::Pos;
/// Java HashSet<BlockPos> bucket traversal matters when decorators sort by Y.
/// Keep per-bucket insertion order and Java's stable low/high split on resize.
#[derive(Debug)]
pub(super) struct Positions {
    buckets: Vec<Vec<Pos>>,
    len: usize,
}
impl Default for Positions {
    fn default() -> Self {
        Self {
            buckets: vec![Vec::new(); 16],
            len: 0,
        }
    }
}
fn hash(p: Pos) -> u32 {
    let h = p[1]
        .wrapping_add(p[2].wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(p[0]) as u32;
    h ^ (h >> 16)
}
impl Positions {
    pub fn insert(&mut self, p: Pos) {
        let index = hash(p) as usize & (self.buckets.len() - 1);
        if self.buckets[index].contains(&p) {
            return;
        }
        self.buckets[index].push(p);
        self.len += 1;
        if self.len > self.buckets.len() * 3 / 4
            || self.buckets[index].len() >= 9 && self.buckets.len() < 64
        {
            let mut buckets = vec![Vec::new(); self.buckets.len() * 2];
            for p in self.iter() {
                let i = hash(p) as usize & (buckets.len() - 1);
                buckets[i].push(p);
            }
            self.buckets = buckets;
        }
    }
    pub fn contains(&self, p: Pos) -> bool {
        self.buckets[hash(p) as usize & (self.buckets.len() - 1)].contains(&p)
    }
    pub fn pop_first(&mut self) -> Option<Pos> {
        let bucket = self.buckets.iter_mut().find(|bucket| !bucket.is_empty())?;
        self.len -= 1;
        Some(bucket.remove(0))
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = Pos> + '_ {
        self.buckets.iter().flatten().copied()
    }
    pub fn sorted(&self) -> Vec<Pos> {
        let mut p = self.iter().collect::<Vec<_>>();
        p.sort_by_key(|p| p[1]);
        p
    }
}
