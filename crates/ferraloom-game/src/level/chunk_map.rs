use super::chunk_holder::ChunkHolder;
use ferraloom_util::ChunkPos;
use std::collections::{BTreeMap, HashSet};
#[derive(Default)]
pub struct ChunkMap {
    pub holders: BTreeMap<i64, ChunkHolder>,
    next_request: u64,
    holder_order: ferraloom_util::collections::LongLinkedOpenHashSet,
}
impl ChunkMap {
    /// Preserve holder insertion order independently of its lookup map.
    pub fn holder_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.holder_order.iter().map(|&key| ChunkPos::unpack(key))
    }
    pub fn ensure_holder(&mut self, pos: ChunkPos) -> &mut ChunkHolder {
        self.holders.entry(pos.pack()).or_insert_with(|| {
            self.holder_order.insert(pos.pack());
            ChunkHolder::new(pos, 0)
        })
    }
    pub fn request(&mut self, pos: ChunkPos) -> Option<u64> {
        let holder = self.ensure_holder(pos);
        if holder.generation_requested {
            return None;
        }
        let request = self.next_request;
        self.next_request = self.next_request.wrapping_add(1);
        let holder = self.holders.get_mut(&pos.pack()).unwrap();
        holder.request_id = request;
        holder.generation_requested = true;
        Some(request)
    }
}
/// Port of PlayerChunkSender: quota, one initial unacknowledged batch, then ten.
#[derive(Debug)]
pub struct PlayerChunkSender {
    pub pending: HashSet<ChunkPos>,
    pub sent: HashSet<ChunkPos>,
    desired: f32,
    quota: f32,
    unacknowledged: i32,
    max_unacknowledged: i32,
}
impl Default for PlayerChunkSender {
    fn default() -> Self {
        Self {
            pending: HashSet::new(),
            sent: HashSet::new(),
            desired: 9.,
            quota: 0.,
            unacknowledged: 0,
            max_unacknowledged: 1,
        }
    }
}
impl PlayerChunkSender {
    pub fn drop_chunk(&mut self, pos: ChunkPos) -> bool {
        if self.pending.remove(&pos) {
            false
        } else {
            self.sent.remove(&pos)
        }
    }
    pub fn acknowledge(&mut self, desired: f32) {
        self.unacknowledged = (self.unacknowledged - 1).max(0);
        self.desired = if desired.is_nan() {
            0.01
        } else {
            desired.clamp(0.01, 64.)
        };
        if self.unacknowledged == 0 {
            self.quota = 1.;
        }
        self.max_unacknowledged = 10;
    }
    pub fn next_batch(
        &mut self,
        center: ChunkPos,
        ready: impl Fn(ChunkPos) -> bool,
    ) -> Vec<ChunkPos> {
        if self.unacknowledged >= self.max_unacknowledged {
            return Vec::new();
        }
        self.quota = (self.quota + self.desired).min(self.desired.max(1.));
        if self.quota < 1. {
            return Vec::new();
        }
        let mut positions: Vec<_> = self.pending.iter().copied().collect();
        positions.sort_by_key(|pos| {
            let x = pos.x - center.x;
            let z = pos.z - center.z;
            (x * x + z * z, pos.pack())
        });
        // Select nearest pending positions before filtering readiness, as vanilla does.
        positions.truncate(self.quota.floor() as usize);
        positions.retain(|&pos| ready(pos));
        if !positions.is_empty() {
            self.unacknowledged += 1;
            self.quota -= positions.len() as f32;
            for pos in &positions {
                self.pending.remove(pos);
                self.sent.insert(*pos);
            }
        }
        positions
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_wait_for_acknowledgement() {
        let mut sender = PlayerChunkSender::default();
        for x in 0..30 {
            sender.pending.insert(ChunkPos::new(x, 0));
        }
        assert_eq!(sender.next_batch(ChunkPos::new(0, 0), |_| true).len(), 9);
        assert!(sender.next_batch(ChunkPos::new(0, 0), |_| true).is_empty());
        sender.acknowledge(f32::NAN);
        assert_eq!(sender.next_batch(ChunkPos::new(0, 0), |_| true).len(), 1);
    }
}
