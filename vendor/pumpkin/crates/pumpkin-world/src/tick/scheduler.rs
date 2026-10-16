use std::sync::Mutex;

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use crate::tick::{MAX_TICK_DELAY, OrderedTick, ScheduledTick};

pub struct ChunkTickScheduler<T> {
    inner: Mutex<Option<Box<ChunkTickSchedulerInner<T>>>>,
}

struct ChunkTickSchedulerInner<T> {
    // The last completed tick phase. Queue access and clock advancement share
    // one lock so scheduling and snapshots use the same remaining-delay origin.
    offset: usize,
    tick_queue: [Vec<OrderedTick<T>>; MAX_TICK_DELAY],
    queued_ticks: FxHashSet<(BlockPos, T)>,
}

impl<'a, T: std::hash::Hash + Eq> ChunkTickScheduler<&'a T> {
    pub fn step_tick(&self) -> Vec<OrderedTick<&'a T>> {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return Vec::new();
        };

        let current_offset = inner.offset;
        let next_offset = (current_offset + 1) % MAX_TICK_DELAY;
        // Delay zero is already due; delay one becomes due in this phase.
        // Both must run here, while delay two waits for the second phase.
        let mut res = std::mem::take(&mut inner.tick_queue[current_offset]);
        res.append(&mut inner.tick_queue[next_offset]);
        inner.offset = next_offset;

        if !res.is_empty() {
            for next_tick in &res {
                inner
                    .queued_ticks
                    .remove(&(next_tick.position, next_tick.value));
            }
            if inner.queued_ticks.is_empty() {
                *inner_guard = None;
            }
        }
        res
    }

    pub fn schedule_tick(&self, tick: &ScheduledTick<&'a T>, sub_tick_order: u64) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = inner_guard.get_or_insert_with(|| {
            Box::new(ChunkTickSchedulerInner {
                offset: 0,
                tick_queue: std::array::from_fn(|_| Vec::new()),
                queued_ticks: FxHashSet::default(),
            })
        });

        if inner.queued_ticks.insert((tick.position, tick.value)) {
            let index = (inner.offset + tick.delay as usize) % MAX_TICK_DELAY;

            inner.tick_queue[index].push(OrderedTick {
                priority: tick.priority,
                sub_tick_order,
                position: tick.position,
                value: tick.value,
            });
        }
    }

    pub fn is_scheduled(&self, pos: BlockPos, value: &T) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| inner.queued_ticks.contains(&(pos, value)))
    }

    pub fn clear_area(&self, min: &BlockPos, max: &BlockPos) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return;
        };

        let contains = |position: &BlockPos| {
            position.0.x >= min.0.x
                && position.0.x < max.0.x
                && position.0.y >= min.0.y
                && position.0.y < max.0.y
                && position.0.z >= min.0.z
                && position.0.z < max.0.z
        };

        for queue in &mut inner.tick_queue {
            queue.retain(|tick| !contains(&tick.position));
        }
        inner
            .queued_ticks
            .retain(|(position, _)| !contains(position));
        let became_empty = inner.queued_ticks.is_empty();

        if became_empty {
            *inner_guard = None;
        }
    }

    pub fn has_ticks(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| !inner.queued_ticks.is_empty())
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<ScheduledTick<&'a T>> {
        let inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_ref() else {
            return Vec::new();
        };

        let mut res = Vec::new();

        for i in 0..MAX_TICK_DELAY {
            let index = (inner.offset + i) % MAX_TICK_DELAY;
            res.extend(inner.tick_queue[index].iter().map(|x| ScheduledTick {
                delay: i as u8,
                priority: x.priority,
                position: x.position,
                value: x.value,
            }));
        }
        res
    }
}

impl<'a, T: std::hash::Hash + Eq + 'static> FromIterator<ScheduledTick<&'a T>>
    for ChunkTickScheduler<&'a T>
{
    fn from_iter<I: IntoIterator<Item = ScheduledTick<&'a T>>>(iter: I) -> Self {
        let scheduler = Self::default();
        let iter = iter.into_iter();

        let (lower, _) = iter.size_hint();
        if lower > 0 {
            let mut inner_guard = scheduler
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let inner = inner_guard.get_or_insert_with(|| {
                Box::new(ChunkTickSchedulerInner {
                    offset: 0,
                    tick_queue: std::array::from_fn(|_| Vec::new()),
                    queued_ticks: FxHashSet::default(),
                })
            });
            inner.queued_ticks.reserve(lower);
        }

        for tick in iter {
            scheduler.schedule_tick(&tick, 0);
        }
        scheduler
    }
}

impl<T> Default for ChunkTickScheduler<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
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
            delay,
            priority: TickPriority::High,
            position: BlockPos::new(x, 64, 0),
            value: &Block::STONE,
        }
    }

    #[test]
    fn scheduled_delays_run_when_due_and_deduplicate_without_postponing() {
        let scheduler = ChunkTickScheduler::default();
        for delay in 0..=u8::MAX {
            scheduler.schedule_tick(&tick(i32::from(delay), delay), u64::from(delay));
        }
        // Scheduling the same position/type again must keep the original due
        // time, priority and insertion order, rather than postponing it.
        let mut duplicate = tick(2, 255);
        duplicate.priority = TickPriority::Low;
        scheduler.schedule_tick(&duplicate, 999);

        for elapsed in 1..=u8::MAX {
            let due = scheduler.step_tick();
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
                assert_eq!(entry.sub_tick_order, entry.position.0.x as u64);
                assert!(!scheduler.is_scheduled(entry.position, &Block::STONE));
            }
        }
        assert!(!scheduler.has_ticks());
        assert!(scheduler.to_vec().is_empty());
        // An empty queue may release its allocation and restart its clock.
        scheduler.schedule_tick(&tick(256, 2), 256);
        assert!(scheduler.step_tick().is_empty());
        assert_eq!(scheduler.step_tick()[0].position.0.x, 256);
    }

    #[test]
    fn remaining_delays_round_trip_through_nbt_across_ring_wrap() {
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(255, 255), 0);
        for _ in 0..254 {
            assert!(scheduler.step_tick().is_empty());
        }
        for delay in 0..=2 {
            scheduler.schedule_tick(&tick(i32::from(delay), delay), u64::from(delay));
        }
        let snapshot = scheduler.to_vec();
        assert_eq!(
            snapshot.iter().map(|entry| entry.delay).collect::<Vec<_>>(),
            vec![0, 1, 1, 2]
        );
        let restored: ChunkTickScheduler<_> = snapshot
            .iter()
            .map(|entry| {
                let nbt = entry.to_nbt_compound();
                assert_eq!(nbt.get_int("t"), Some(i32::from(entry.delay)));
                ScheduledTick::<&'static Block>::from_nbt_compound(&nbt).unwrap()
            })
            .collect();

        for expected in [vec![0, 1, 255], vec![2]] {
            for queue in [&scheduler, &restored] {
                let mut due: Vec<_> = queue
                    .step_tick()
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
}
