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
    /// Port of ChunkMap's final unloaded-holder removal. The runtime must first
    /// await a successful save, release generation jobs and recheck tickets.
    /// Demotions must have settled before removing either lookup or traversal.
    pub fn remove_unloaded_holder(&mut self, pos: ChunkPos) -> Option<ChunkHolder> {
        let key = pos.pack();
        let holder = self.holders.get(&key)?;
        if holder.ticket_level <= super::distance_manager::MAX_LOADED_LEVEL
            || holder.confirmed_full_status()
                != super::distance_manager::FullChunkStatus::Inaccessible
            || holder.pending_full_status().is_some()
        {
            return None;
        }
        let holder = self.holders.remove(&key)?;
        self.holder_order.remove(&key);
        Some(holder)
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
    #[test]
    fn unloading_waits_for_ticket_demotion_and_reloading_gets_a_new_request_in_order() {
        let mut chunks = ChunkMap::default();
        let a = ChunkPos::new(0, 0);
        let b = ChunkPos::new(1, 0);
        let first = chunks.request(a).unwrap();
        chunks.request(b).unwrap();
        let holder = chunks.holders.get_mut(&a.pack()).unwrap();
        holder.ticket_level = 31;
        holder.update_full_status();
        holder.confirm_full_status(holder.pending_full_status().unwrap());
        holder.ticket_level = 45;
        assert!(chunks.remove_unloaded_holder(a).is_none());
        let holder = chunks.holders.get_mut(&a.pack()).unwrap();
        holder.update_full_status();
        holder.ticket_level = 44;
        assert!(chunks.remove_unloaded_holder(a).is_none());
        chunks.holders.get_mut(&a.pack()).unwrap().ticket_level = 45;
        assert_eq!(chunks.remove_unloaded_holder(a).unwrap().request_id, first);
        assert_eq!(chunks.holder_positions().collect::<Vec<_>>(), vec![b]);
        assert!(chunks.remove_unloaded_holder(a).is_none());
        let second = chunks.request(a).unwrap();
        assert_ne!(first, second);
        assert_eq!(chunks.request(a), None);
        assert_eq!(chunks.holder_positions().collect::<Vec<_>>(), vec![b, a]);
    }
}
