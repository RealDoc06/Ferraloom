use super::chunk_holder::ChunkHolder;
use super::generation::{
    ChunkPyramid, ChunkStatus, GenerationSchedule, GenerationTaskChange, GenerationTaskToken,
};
use ferraloom_util::ChunkPos;
use std::collections::{BTreeMap, HashSet};
#[derive(Default)]
pub struct ChunkMap {
    pub holders: BTreeMap<i64, ChunkHolder>,
    next_task: u64,
    next_holder_epoch: u64,
    holder_order: ferraloom_util::collections::LongLinkedOpenHashSet,
}
impl ChunkMap {
    /// Preserve holder insertion order independently of its lookup map.
    pub fn holder_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.holder_order.iter().map(|&key| ChunkPos::unpack(key))
    }
    /// Preserve ChunkMap#updateChunkScheduling's holder object identity until
    /// final removal. An incarnation is distinct from each generation request.
    pub fn ensure_holder(&mut self, pos: ChunkPos) -> &mut ChunkHolder {
        self.holders.entry(pos.pack()).or_insert_with(|| {
            self.next_holder_epoch = self
                .next_holder_epoch
                .checked_add(1)
                .expect("chunk holder incarnations exhausted");
            self.holder_order.insert(pos.pack());
            let mut holder = ChunkHolder::new(pos);
            holder.epoch = self.next_holder_epoch;
            holder
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
            || !holder.generation.generation_references_released()
            || holder.generation.current_task().is_some()
        {
            return None;
        }
        let holder = self.holders.remove(&key)?;
        self.holder_order.remove(&key);
        Some(holder)
    }
    /// Port of GenerationChunkHolder#scheduleChunkGenerationTask. Task IDs
    /// model native task-object identity; per-status future identity stays on
    /// the holder when an increased target cancels and replaces its task.
    pub fn schedule_generation_task(
        &mut self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> GenerationSchedule {
        self.ensure_holder(pos);
        let next_task = &mut self.next_task;
        self.holders
            .get_mut(&pos.pack())
            .unwrap()
            .generation
            .schedule(status, &mut |target| allocate_task(next_task, target))
    }

    /// Port of updateHighestAllowedStatus at updateFutures' native slot, after
    /// all ordered ticket-level changes have been applied to their holders.
    pub fn update_highest_allowed_status(&mut self, pos: ChunkPos) -> GenerationTaskChange {
        let holder = self.ensure_holder(pos);
        let level = holder.ticket_level;
        let allowed = if level <= 33 {
            Some(ChunkStatus::Full)
        } else {
            ChunkPyramid::generation()
                .step_to(ChunkStatus::Full)
                .accumulated
                .get((level - 33) as usize)
        };
        let next_task = &mut self.next_task;
        self.holders
            .get_mut(&pos.pack())
            .unwrap()
            .generation
            .update_highest_allowed_status(allowed, &mut |target| allocate_task(next_task, target))
    }
}

fn allocate_task(next: &mut u64, target: ChunkStatus) -> GenerationTaskToken {
    let id = *next;
    *next = next
        .checked_add(1)
        .expect("generation task identities exhausted");
    GenerationTaskToken { id, target }
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
    fn allow(chunks: &mut ChunkMap, pos: ChunkPos, level: i32) -> GenerationTaskChange {
        chunks.ensure_holder(pos).ticket_level = level;
        chunks.update_highest_allowed_status(pos)
    }

    #[test]
    fn holder_incarnation_survives_replaced_tasks_and_recreation_changes_it() {
        let mut chunks = ChunkMap::default();
        let pos = ChunkPos::ZERO;
        let other = ChunkPos::new(1, 0);
        allow(&mut chunks, pos, 33);
        let epoch = chunks.ensure_holder(pos).epoch;
        let other_epoch = chunks.ensure_holder(other).epoch;
        assert_ne!(epoch, 0);
        assert!(other_epoch > epoch);
        let first = chunks.schedule_generation_task(pos, ChunkStatus::Biomes);
        let upgraded = chunks.schedule_generation_task(pos, ChunkStatus::Full);
        assert_eq!(upgraded.task.cancel, first.task.schedule);
        assert_eq!(chunks.ensure_holder(pos).epoch, epoch);
        let holder = chunks.ensure_holder(pos);
        holder.ticket_level = 31;
        holder.update_full_status();
        let promotion = holder.pending_full_status().unwrap();
        assert!(holder.confirm_full_status(promotion).is_some());
        assert_eq!(holder.epoch, epoch);
        let cancelled = allow(&mut chunks, pos, 45);
        assert_eq!(cancelled.cancel, upgraded.task.schedule);
        chunks.ensure_holder(pos).update_full_status();
        assert_eq!(chunks.remove_unloaded_holder(pos).unwrap().epoch, epoch);
        let recreated = chunks.ensure_holder(pos).epoch;
        assert!(recreated > other_epoch);
        assert_eq!(chunks.ensure_holder(other).epoch, other_epoch);
    }

    #[test]
    fn partial_ticket_schedules_its_status_and_upgrade_keeps_that_future() {
        let mut chunks = ChunkMap::default();
        let pos = ChunkPos::ZERO;
        allow(&mut chunks, pos, 36);
        assert_eq!(
            chunks
                .ensure_holder(pos)
                .generation
                .highest_allowed_status(),
            Some(ChunkStatus::Biomes)
        );
        let biomes = chunks.schedule_generation_task(pos, ChunkStatus::Biomes);
        assert!(biomes.task.schedule.is_some());
        let disallowed = chunks.schedule_generation_task(pos, ChunkStatus::Full);
        assert_eq!(
            disallowed.future,
            super::super::generation::GenerationFutureId::UNLOADED
        );
        assert_eq!(disallowed.task, GenerationTaskChange::default());
        allow(&mut chunks, pos, 33);
        let full = chunks.schedule_generation_task(pos, ChunkStatus::Full);
        assert_eq!(full.task.cancel, biomes.task.schedule);
        assert_eq!(
            chunks
                .ensure_holder(pos)
                .generation
                .future_id(ChunkStatus::Biomes),
            Some(biomes.future)
        );
        let downgraded = allow(&mut chunks, pos, 35);
        assert_eq!(downgraded.cancel, full.task.schedule);
        assert_eq!(downgraded.schedule.unwrap().target, ChunkStatus::Biomes);
        assert!(
            !chunks
                .ensure_holder(pos)
                .generation
                .remove_task(full.task.schedule.unwrap())
        );
        assert_eq!(
            chunks.ensure_holder(pos).generation.current_task(),
            downgraded.schedule
        );
    }

    #[test]
    fn unloading_waits_for_claims_and_recreation_appends_in_native_holder_order() {
        let mut chunks = ChunkMap::default();
        let a = ChunkPos::ZERO;
        let b = ChunkPos::new(1, 0);
        allow(&mut chunks, a, 33);
        let first = chunks
            .schedule_generation_task(a, ChunkStatus::Full)
            .task
            .schedule
            .unwrap();
        chunks.ensure_holder(b);
        chunks
            .ensure_holder(a)
            .generation
            .increase_generation_ref_count();
        let cancelled = allow(&mut chunks, a, 45);
        assert_eq!(cancelled.cancel, Some(first));
        assert!(chunks.remove_unloaded_holder(a).is_none());
        chunks
            .ensure_holder(a)
            .generation
            .decrease_generation_ref_count();
        assert!(chunks.remove_unloaded_holder(a).is_some());
        assert_eq!(chunks.holder_positions().collect::<Vec<_>>(), vec![b]);
        allow(&mut chunks, a, 33);
        let second = chunks
            .schedule_generation_task(a, ChunkStatus::Full)
            .task
            .schedule
            .unwrap();
        assert!(second.id > first.id);
        assert_eq!(chunks.holder_positions().collect::<Vec<_>>(), vec![b, a]);
    }
}
