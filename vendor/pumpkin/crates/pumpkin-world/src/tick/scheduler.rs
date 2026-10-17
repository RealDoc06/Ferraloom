use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    hash::Hash,
    sync::{Mutex, MutexGuard},
};

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use super::{SavedTick, ScheduledTick};

pub struct ChunkTickScheduler<T> {
    inner: Mutex<ChunkTickQueue<T>>,
}

/// The heap compares scheduling keys only; equal keys still retain every record.
struct QueueEntry<T>(ScheduledTick<T>);

impl<T> PartialEq for QueueEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.drain_key() == other.0.drain_key()
    }
}
impl<T> Eq for QueueEntry<T> {}
impl<T> PartialOrd for QueueEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for QueueEntry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.drain_key().cmp(&self.0.drain_key())
    }
}

pub(crate) struct ChunkTickQueue<T> {
    queue: BinaryHeap<QueueEntry<T>>,
    pending_saved: Option<Vec<SavedTick<T>>>,
    keys: FxHashSet<(BlockPos, T)>,
}

impl<T> ChunkTickScheduler<T> {
    // A leaf lock: never acquire map, lifecycle, terrain or POI locks from here.
    pub(crate) fn lock(&self) -> MutexGuard<'_, ChunkTickQueue<T>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<T: Copy + Hash + Eq> ChunkTickScheduler<T> {
    /// Queued absolute records only. Pending saved records are deliberately hidden.
    #[must_use]
    pub fn get_all(&self) -> Vec<ScheduledTick<T>> {
        self.lock().get_all()
    }

    #[must_use]
    pub fn has_ticks(&self) -> bool {
        self.lock().peek().is_some()
    }

    /// Explicit-clock packing for standalone callers and fixtures.
    #[must_use]
    pub fn pack_at(&self, now: i64) -> Vec<SavedTick<T>> {
        self.lock().pack(now)
    }
}

impl<T: Hash + Eq> ChunkTickScheduler<&T> {
    #[must_use]
    pub fn is_scheduled(&self, pos: BlockPos, value: &T) -> bool {
        self.lock().keys.contains(&(pos, value))
    }
}

impl<T: Copy + Hash + Eq> ChunkTickQueue<T> {
    pub(crate) fn schedule(&mut self, tick: ScheduledTick<T>) -> bool {
        if !self.keys.insert((tick.position, tick.value)) {
            return false;
        }
        self.queue.push(QueueEntry(tick));
        true
    }

    pub(crate) fn peek(&self) -> Option<ScheduledTick<T>> {
        self.queue.peek().map(|entry| entry.0)
    }

    pub(crate) fn poll(&mut self) -> Option<ScheduledTick<T>> {
        let tick = self.queue.pop()?.0;
        self.keys.remove(&(tick.position, tick.value));
        Some(tick)
    }

    pub(crate) fn get_all(&self) -> Vec<ScheduledTick<T>> {
        self.queue.iter().map(|entry| entry.0).collect()
    }

    pub(crate) fn unpack(&mut self, now: i64) -> bool {
        let Some(saved) = self.pending_saved.take() else {
            return false;
        };
        let changed = !saved.is_empty();
        let mut order = -(saved.len() as i64);
        for tick in saved {
            // Keys were seeded by construction. Saved duplicates must survive.
            self.queue.push(QueueEntry(tick.unpack(now, order)));
            order = order.wrapping_add(1);
        }
        changed
    }

    pub(crate) fn pack(&self, now: i64) -> Vec<SavedTick<T>> {
        let mut result = self.pending_saved.clone().unwrap_or_default();
        let mut queued = self.get_all();
        queued.sort_by_key(|tick| tick.sub_tick_order);
        result.extend(queued.iter().map(|tick| tick.to_saved_tick(now)));
        result
    }

    pub(crate) fn pack_for_retirement(&mut self, now: i64) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        self.pending_saved = Some(self.pack(now));
        self.queue.clear();
        // Preserve keys verbatim: polling one saved duplicate can remove its
        // key while another same-key record remains in the heap.
        true
    }

    pub(crate) fn pack_unbound(&self) -> Option<Vec<SavedTick<T>>> {
        self.queue
            .is_empty()
            .then(|| self.pending_saved.clone().unwrap_or_default())
    }

    pub(crate) fn clear_area(&mut self, min: &BlockPos, max: &BlockPos) -> bool {
        let contains = |position: &BlockPos| {
            position.0.x >= min.0.x
                && position.0.x < max.0.x
                && position.0.y >= min.0.y
                && position.0.y < max.0.y
                && position.0.z >= min.0.z
                && position.0.z < max.0.z
        };
        let before = self.queue.len();
        self.queue.retain(|entry| !contains(&entry.0.position));
        let mut removed = self.queue.len() != before;
        if let Some(saved) = self.pending_saved.as_mut() {
            let before = saved.len();
            saved.retain(|tick| !contains(&tick.position));
            removed |= saved.len() != before;
        }
        if removed {
            self.keys.retain(|(pos, _)| !contains(pos));
        }
        removed
    }
}

impl<T: Copy + Hash + Eq> FromIterator<SavedTick<T>> for ChunkTickScheduler<T> {
    fn from_iter<I: IntoIterator<Item = SavedTick<T>>>(iter: I) -> Self {
        let saved: Vec<_> = iter.into_iter().collect();
        let keys = saved
            .iter()
            .map(|tick| (tick.position, tick.value))
            .collect();
        Self {
            inner: Mutex::new(ChunkTickQueue {
                queue: BinaryHeap::new(),
                pending_saved: Some(saved),
                keys,
            }),
        }
    }
}

impl<T> Default for ChunkTickScheduler<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(ChunkTickQueue {
                queue: BinaryHeap::new(),
                pending_saved: None,
                keys: FxHashSet::default(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::Block;

    use super::*;
    use crate::tick::TickPriority;

    fn tick(x: i32, delay: u8) -> ScheduledTick<&'static Block> {
        ScheduledTick {
            trigger_tick: i64::from(delay),
            sub_tick_order: 0,
            priority: TickPriority::High,
            position: BlockPos::new(x, 64, 0),
            value: &Block::STONE,
        }
    }

    fn schedule_at(
        scheduler: &ChunkTickScheduler<&'static Block>,
        mut tick: ScheduledTick<&'static Block>,
        order: i64,
        now: i64,
    ) {
        tick.trigger_tick = now.wrapping_add(tick.trigger_tick);
        tick.sub_tick_order = order;
        scheduler.lock().schedule(tick);
    }

    fn drain_at(
        scheduler: &ChunkTickScheduler<&'static Block>,
        now: i64,
    ) -> Vec<ScheduledTick<&'static Block>> {
        let mut queue = scheduler.lock();
        let mut ticks = Vec::new();
        while queue.peek().is_some_and(|tick| tick.trigger_tick <= now) {
            ticks.push(queue.poll().unwrap());
        }
        ticks
    }

    #[test]
    fn scheduled_delays_run_when_due_and_deduplicate_without_postponing() {
        let scheduler = ChunkTickScheduler::default();
        for delay in 0..=u8::MAX {
            schedule_at(
                &scheduler,
                tick(i32::from(delay), delay),
                i64::from(delay),
                0,
            );
        }
        // Scheduling the same position/type again must keep the original due
        // time, priority and insertion order, rather than postponing it.
        let mut duplicate = tick(2, 255);
        duplicate.priority = TickPriority::Low;
        schedule_at(&scheduler, duplicate, 999, 0);

        for elapsed in 1..=u8::MAX {
            let due = drain_at(&scheduler, i64::from(elapsed));
            let expected: Vec<_> = if elapsed == 1 {
                vec![0, 1]
            } else {
                vec![i32::from(elapsed)]
            };
            assert_eq!(
                due.iter()
                    .map(|entry| entry.position.0.x)
                    .collect::<Vec<_>>(),
                expected
            );
            for entry in due {
                assert_eq!(entry.priority, TickPriority::High);
                assert_eq!(entry.sub_tick_order, i64::from(entry.position.0.x));
                assert!(!scheduler.is_scheduled(entry.position, &Block::STONE));
            }
        }
        assert!(!scheduler.has_ticks());
        assert!(scheduler.get_all().is_empty());
        // An empty queue must retain the explicit absolute trigger origin.
        schedule_at(&scheduler, tick(256, 2), 256, 255);
        assert!(drain_at(&scheduler, 256).is_empty());
        assert_eq!(drain_at(&scheduler, 257)[0].position.0.x, 256);
    }

    #[test]
    fn remaining_delays_round_trip_through_nbt_across_ring_wrap() {
        let scheduler = ChunkTickScheduler::default();
        schedule_at(&scheduler, tick(255, 255), 1, 0);
        for now in 1..=254 {
            assert!(drain_at(&scheduler, now).is_empty());
        }
        for delay in 0..=2 {
            schedule_at(
                &scheduler,
                tick(i32::from(delay), delay),
                i64::from(delay) * 2,
                254,
            );
        }
        let snapshot = scheduler.pack_at(254);
        assert_eq!(
            snapshot.iter().map(|entry| entry.delay).collect::<Vec<_>>(),
            vec![0, 1, 1, 2]
        );
        let restored: ChunkTickScheduler<_> = snapshot
            .iter()
            .map(|entry| {
                let nbt = entry.to_nbt_compound();
                assert_eq!(nbt.get_int("t"), Some(entry.delay));
                SavedTick::<&'static Block>::from_nbt_compound(&nbt).unwrap()
            })
            .collect();

        restored.lock().unpack(254);
        for (now, expected) in [(255, vec![0, 1, 255]), (256, vec![2])] {
            for queue in [&scheduler, &restored] {
                let mut due: Vec<_> = drain_at(queue, now)
                    .into_iter()
                    .map(|entry| {
                        assert_eq!(entry.priority, TickPriority::High);
                        assert_eq!(entry.value.id, Block::STONE.id);
                        entry.position.0.x
                    })
                    .collect();
                due.sort_unstable();
                assert_eq!(due, expected);
            }
        }
        assert!(!scheduler.has_ticks());
        assert!(!restored.has_ticks());
    }

    #[tokio::test]
    async fn empty_disk_pending_is_consumed_once_without_dirtying_ready_chunk() {
        use crate::{
            chunk::{ChunkData, format::anvil::SingleChunkDataSerializer},
            level::Level,
        };
        use pumpkin_config::world::LevelConfig;
        use pumpkin_data::dimension::Dimension;
        use pumpkin_util::math::vector2::Vector2;
        use std::sync::{Arc, atomic::Ordering};
        let folder = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            folder.path().to_owned(),
            0,
            Dimension::OVERWORLD,
        );
        level.shutdown().await.unwrap();
        level.set_tick_game_time(400);
        let bytes = ChunkData::empty(0, 0).to_bytes().unwrap();
        let chunk = Arc::new(ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap());
        assert!(chunk.block_ticks.lock().pending_saved.is_some());
        assert!(chunk.fluid_ticks.lock().pending_saved.is_some());
        chunk.post_processing_ready.store(true, Ordering::Relaxed);
        level.bind_chunk_poi(&chunk).unwrap();
        level
            .loaded_chunks
            .insert(Vector2::new(0, 0), chunk.clone());
        assert!(!chunk.dirty.load(Ordering::Acquire));
        level.unpack_chunk_ticks(Vector2::new(0, 0), 400);
        assert!(chunk.block_ticks.lock().pending_saved.is_none());
        assert!(chunk.fluid_ticks.lock().pending_saved.is_none());
        assert!(!chunk.dirty.load(Ordering::Acquire));
        level.unpack_chunk_ticks(Vector2::new(0, 0), 500);
        assert!(!chunk.dirty.load(Ordering::Acquire));
        assert!(
            !level
                .chunks_with_scheduled_ticks
                .contains(&Vector2::new(0, 0))
        );
    }

    #[test]
    fn retirement_pack_preserves_missing_key_after_polling_one_saved_duplicate() {
        let scheduler: ChunkTickScheduler<_> = [
            SavedTick {
                position: BlockPos::new(1, 64, 0),
                value: &Block::STONE,
                delay: 0,
                priority: TickPriority::Normal,
            },
            SavedTick {
                position: BlockPos::new(1, 64, 0),
                value: &Block::STONE,
                delay: 10,
                priority: TickPriority::Normal,
            },
        ]
        .into_iter()
        .collect();
        {
            let mut queue = scheduler.lock();
            assert!(queue.unpack(100));
            assert_eq!(queue.poll().unwrap().trigger_tick, 100);
        };
        assert!(!scheduler.is_scheduled(BlockPos::new(1, 64, 0), &Block::STONE));
        assert_eq!(scheduler.get_all().len(), 1);
        assert!(scheduler.lock().pack_for_retirement(105));
        assert!(scheduler.get_all().is_empty());
        assert!(!scheduler.is_scheduled(BlockPos::new(1, 64, 0), &Block::STONE));
        assert_eq!(scheduler.pack_at(10_000)[0].delay, 5);
    }
}
