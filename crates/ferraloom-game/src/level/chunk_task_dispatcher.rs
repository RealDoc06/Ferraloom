//! ChunkTaskDispatcher's four mailbox priorities and throttled chunk batches.
use std::collections::VecDeque;

use ferraloom_util::{
    ChunkPos,
    collections::{Long2IntOpenHashMap, Long2ObjectLinkedOpenHashMap, LongOpenHashSet},
};

const PRIORITY_LEVEL_COUNT: usize = super::distance_manager::MAX_LOADED_LEVEL as usize + 2;

enum Message<T> {
    Level {
        pos: ChunkPos,
        level: i32,
    },
    Release {
        pos: ChunkPos,
        clear: bool,
        after: Option<T>,
    },
    Submit {
        pos: ChunkPos,
        level: i32,
        task: T,
    },
    Poll,
}
/// Runtime executes these owned tasks on its main executor. After all tasks in
/// a batch finish, call complete_batch; its slot remains held until release.
#[derive(Debug, PartialEq, Eq)]
pub enum ChunkTaskDispatch<T> {
    Released(T),
    Batch { pos: ChunkPos, tasks: Vec<T> },
}

/// Port of ThrottlingChunkTaskDispatcher. T is a concrete, owned main-thread
/// operation; no callback holds a borrow of canonical level or holder state.
pub struct ThrottlingChunkTaskDispatcher<T> {
    queues: Vec<Long2ObjectLinkedOpenHashMap<Vec<T>>>,
    mailbox: [VecDeque<Message<T>>; 4],
    queue_levels: Long2IntOpenHashMap,
    in_execution: LongOpenHashSet,
    max_in_execution: usize,
    sleeping: bool,
}
impl<T> ThrottlingChunkTaskDispatcher<T> {
    pub fn new(max_in_execution: usize) -> Self {
        Self {
            queues: (0..PRIORITY_LEVEL_COUNT)
                .map(|_| Long2ObjectLinkedOpenHashMap::new())
                .collect(),
            mailbox: std::array::from_fn(|_| VecDeque::new()),
            queue_levels: Long2IntOpenHashMap::new(),
            in_execution: LongOpenHashSet::new(),
            max_in_execution,
            sleeping: true,
        }
    }
    /// PlayerTicketTracker's queueLevels default is maxDistance+2 = 34. It is
    /// read when the priority-zero message runs, rather than when enqueued.
    pub fn queue_level(&self, pos: ChunkPos) -> i32 {
        self.queue_levels.get(&pos.pack()).copied().unwrap_or(34)
    }
    pub fn on_level_change(&mut self, pos: ChunkPos, level: i32) {
        self.mailbox[0].push_back(Message::Level { pos, level });
    }
    pub fn release(&mut self, pos: ChunkPos, clear: bool, after: Option<T>) {
        self.mailbox[1].push_back(Message::Release { pos, clear, after });
    }
    pub fn submit(&mut self, pos: ChunkPos, level: i32, task: T) {
        assert!((0..PRIORITY_LEVEL_COUNT as i32).contains(&level));
        self.mailbox[2].push_back(Message::Submit { pos, level, task });
    }
    pub fn complete_batch(&mut self) {
        self.mailbox[3].push_back(Message::Poll);
    }
    fn wake(&mut self) {
        if self.sleeping {
            self.sleeping = false;
            self.mailbox[3].push_back(Message::Poll);
        }
    }
    fn append(&mut self, priority: usize, pos: i64, tasks: Vec<T>) {
        let queue = &mut self.queues[priority];
        if let Some(existing) = queue.get_mut(&pos) {
            existing.extend(tasks);
        } else {
            queue.insert(pos, tasks);
        }
    }
    pub fn next_dispatch(&mut self) -> Option<ChunkTaskDispatch<T>> {
        loop {
            let message = self.mailbox.iter_mut().find_map(VecDeque::pop_front)?;
            match message {
                Message::Level { pos, level } => {
                    let old = self.queue_level(pos);
                    if old < PRIORITY_LEVEL_COUNT as i32
                        && let Some(tasks) = self.queues[old as usize].remove(&pos.pack())
                        && !tasks.is_empty()
                    {
                        self.append(level as usize, pos.pack(), tasks);
                    }
                    if level >= 34 {
                        self.queue_levels.remove(&pos.pack());
                    } else {
                        self.queue_levels.insert(pos.pack(), level);
                    }
                }
                Message::Release { pos, clear, after } => {
                    for queue in &mut self.queues {
                        if let Some(tasks) = queue.get_mut(&pos.pack()) {
                            if clear {
                                tasks.clear();
                            }
                            if tasks.is_empty() {
                                queue.remove(&pos.pack());
                            }
                        }
                    }
                    self.in_execution.remove(&pos.pack());
                    self.wake();
                    if let Some(task) = after {
                        return Some(ChunkTaskDispatch::Released(task));
                    }
                }
                Message::Submit { pos, level, task } => {
                    self.append(level as usize, pos.pack(), vec![task]);
                    self.wake();
                }
                Message::Poll => {
                    if self.in_execution.len() < self.max_in_execution
                        && let Some((key, tasks)) =
                            self.queues.iter_mut().find_map(|queue| queue.pop_first())
                    {
                        self.in_execution.insert(key);
                        return Some(ChunkTaskDispatch::Batch {
                            pos: ChunkPos::unpack(key),
                            tasks,
                        });
                    }
                    self.sleeping = true;
                }
            }
        }
    }
    pub fn in_execution(&self) -> usize {
        self.in_execution.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{
        simulation::{PlayerLoadingTask, PlayerTicketTracker},
        tickets::{Ticket, TicketStorage, TicketTracker, TicketType},
    };

    fn pump(
        tracker: &PlayerTicketTracker,
        dispatcher: &mut ThrottlingChunkTaskDispatcher<PlayerLoadingTask>,
        tickets: &mut TicketStorage,
    ) -> String {
        while let Some(dispatch) = dispatcher.next_dispatch() {
            let (tasks, batch) = match dispatch {
                ChunkTaskDispatch::Released(task) => (vec![task], false),
                ChunkTaskDispatch::Batch { tasks, .. } => (tasks, true),
            };
            for task in tasks {
                match task {
                    PlayerLoadingTask::Add(pos) if tracker.has_ticket(pos) => tickets.add(
                        pos,
                        Ticket {
                            kind: TicketType::PlayerLoading,
                            level: 31,
                            expires: None,
                        },
                    ),
                    PlayerLoadingTask::Add(pos) => dispatcher.release(pos, false, None),
                    PlayerLoadingTask::Remove(pos) => {
                        tickets.remove(pos, TicketType::PlayerLoading, 31)
                    }
                }
            }
            if batch {
                dispatcher.complete_batch();
            }
        }
        tickets
            .take_source_updates()
            .iter()
            .filter(|u| u.tracker == TicketTracker::Loading)
            .map(|u| format!("{},{},{},{}", u.pos.x, u.pos.z, u.level, u.only_decreased))
            .collect::<Vec<_>>()
            .join(";")
    }
    fn vector(stage: &str) -> &'static str {
        include_str!("../../tests/java/level/player-loading.tsv")
            .lines()
            .find_map(|row| {
                row.split_once('\t')
                    .filter(|(name, _)| *name == stage)
                    .map(|(_, value)| value)
            })
            .unwrap()
    }
    #[test]
    fn native_player_loading_order_throttles_four_slots_and_cancels_moved_tickets() {
        let mut tracker = PlayerTicketTracker::default();
        let mut dispatcher = ThrottlingChunkTaskDispatcher::new(4);
        let mut tickets = TicketStorage::default();
        let origin = ChunkPos::new(0, 0);
        tracker.update_view_distance(2, &mut dispatcher);
        tracker.update_player_chunk(origin, true);
        tracker.run_all_updates(&mut dispatcher);
        assert_eq!(
            pump(&tracker, &mut dispatcher, &mut tickets),
            vector("origin")
        );
        assert_eq!(dispatcher.in_execution(), 4);
        for pos in [
            origin,
            ChunkPos::new(-1, -1),
            ChunkPos::new(1, 1),
            ChunkPos::new(-2, -2),
        ] {
            dispatcher.release(pos, false, None);
            assert_eq!(
                pump(&tracker, &mut dispatcher, &mut tickets),
                vector(&format!("release-{},{}", pos.x, pos.z))
            );
        }
        tracker.update_player_chunk(origin, false);
        let moved = ChunkPos::new(2, -1);
        tracker.update_player_chunk(moved, true);
        tracker.run_all_updates(&mut dispatcher);
        assert_eq!(
            pump(&tracker, &mut dispatcher, &mut tickets),
            vector("move")
        );
        tracker.update_view_distance(0, &mut dispatcher);
        assert_eq!(
            pump(&tracker, &mut dispatcher, &mut tickets),
            vector("shrink")
        );
        tracker.update_player_chunk(moved, false);
        tracker.run_all_updates(&mut dispatcher);
        assert_eq!(
            pump(&tracker, &mut dispatcher, &mut tickets),
            vector("empty")
        );
    }
}
