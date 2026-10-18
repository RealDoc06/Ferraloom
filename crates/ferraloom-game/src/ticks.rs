//! Ports of ScheduledTick, SavedTick, LevelChunkTicks and LevelTicks.
use ferraloom_data::Block;
use ferraloom_util::{
    BlockPos, ChunkPos,
    collections::{JavaPriorityQueue, Long2LongOpenHashMap},
};
use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashSet, VecDeque},
    hash::Hash,
};

pub const MAX_TICKS_PER_TICK: usize = 65_536;

/// Port of TickPriority. Discriminants are the saved -3..3 values.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i8)]
pub enum TickPriority {
    ExtremelyHigh = -3,
    VeryHigh = -2,
    High = -1,
    #[default]
    Normal = 0,
    Low = 1,
    VeryLow = 2,
    ExtremelyLow = 3,
}
impl TickPriority {
    pub fn by_value(value: i32) -> Self {
        match value {
            ..=-3 => Self::ExtremelyHigh,
            -2 => Self::VeryHigh,
            -1 => Self::High,
            0 => Self::Normal,
            1 => Self::Low,
            2 => Self::VeryLow,
            3.. => Self::ExtremelyLow,
        }
    }
}

/// Port of ScheduledTick. Identity is (position, type), not the ordering key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledTick<T = Block> {
    pub pos: BlockPos,
    pub block: T,
    pub trigger_tick: i64,
    pub priority: TickPriority,
    pub sub_tick_order: i64,
}
impl<T: Copy> ScheduledTick<T> {
    fn drain_order(&self, other: &Self) -> Ordering {
        self.trigger_tick
            .cmp(&other.trigger_tick)
            .then(self.intra_tick_order(other))
    }
    fn intra_tick_order(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then(self.sub_tick_order.cmp(&other.sub_tick_order))
    }
    pub fn to_saved_tick(self, current_tick: i64) -> SavedTick<T> {
        SavedTick {
            block: self.block,
            pos: self.pos,
            delay: self.trigger_tick.wrapping_sub(current_tick) as i32,
            priority: self.priority,
        }
    }
}

/// Port of SavedTick. Unpacked saved ticks precede newly scheduled ticks.
#[derive(Clone, Copy, Debug)]
pub struct SavedTick<T> {
    pub block: T,
    pub pos: BlockPos,
    pub delay: i32,
    pub priority: TickPriority,
}
impl<T: Copy> SavedTick<T> {
    pub fn unpack(self, current_tick: i64, sub_tick_order: i64) -> ScheduledTick<T> {
        ScheduledTick {
            block: self.block,
            pos: self.pos,
            trigger_tick: current_tick.wrapping_add(i64::from(self.delay)),
            priority: self.priority,
            sub_tick_order,
        }
    }
    pub fn to_nbt(self, name: impl Into<ferraloom_nbt::NbtString>) -> ferraloom_storage::SavedTick {
        ferraloom_storage::SavedTick {
            id: Some(name.into()),
            x: Some(self.pos.x),
            y: Some(self.pos.y),
            z: Some(self.pos.z),
            delay: Some(self.delay),
            priority: Some(self.priority as i32),
            ..Default::default()
        }
    }
    pub fn from_nbt(tick: &ferraloom_storage::SavedTick, block: T) -> Option<Self> {
        Some(Self {
            block,
            pos: BlockPos::new(tick.x?, tick.y?, tick.z?),
            delay: tick.delay?,
            priority: TickPriority::by_value(tick.priority?),
        })
    }
}

/// Port of LevelChunkTicks. Pending ticks reserve their uniqueness keys before
/// unpacking; pack sorts active ticks by sub-tick order, not due time.
pub struct LevelChunkTicks<T = Block> {
    queue: JavaPriorityQueue<ScheduledTick<T>>,
    pending: Option<Vec<SavedTick<T>>>,
    positions: HashSet<(BlockPos, T)>,
}
impl<T: Copy> Default for LevelChunkTicks<T> {
    fn default() -> Self {
        Self {
            queue: JavaPriorityQueue::new(|a, b| a.drain_order(b)),
            pending: None,
            positions: HashSet::new(),
        }
    }
}
impl<T: Copy + Eq + Hash> LevelChunkTicks<T> {
    pub fn from_saved(pending: Vec<SavedTick<T>>, chunk: ChunkPos) -> Self {
        let pending: Vec<_> = pending
            .into_iter()
            .filter(|t| ChunkPos::containing(t.pos) == chunk)
            .collect();
        Self {
            positions: pending.iter().map(|t| (t.pos, t.block)).collect(),
            pending: Some(pending),
            ..Self::default()
        }
    }
    pub fn unpack(&mut self, current_tick: i64) {
        if let Some(pending) = self.pending.take() {
            let base = -(pending.len() as i64);
            for (i, tick) in pending.into_iter().enumerate() {
                self.queue.offer(tick.unpack(current_tick, base + i as i64));
            }
        }
    }
    pub fn pack(&self, current_tick: i64) -> Vec<SavedTick<T>> {
        let mut saved = self.pending.clone().unwrap_or_default();
        let mut active = self.queue.iter().copied().collect::<Vec<_>>();
        active.sort_by_key(|t| t.sub_tick_order);
        saved.extend(active.into_iter().map(|t| t.to_saved_tick(current_tick)));
        saved
    }
    pub fn peek(&self) -> Option<ScheduledTick<T>> {
        self.queue.peek().copied()
    }
    pub fn poll(&mut self) -> Option<ScheduledTick<T>> {
        let tick = self.queue.poll()?;
        self.positions.remove(&(tick.pos, tick.block));
        Some(tick)
    }
    pub fn schedule(&mut self, tick: ScheduledTick<T>) -> bool {
        if !self.positions.insert((tick.pos, tick.block)) {
            return false;
        }
        self.queue.offer(tick);
        true
    }
    pub fn has_scheduled_tick(&self, pos: BlockPos, block: T) -> bool {
        self.positions.contains(&(pos, block))
    }
    pub fn count(&self) -> usize {
        self.queue.len() + self.pending.as_ref().map_or(0, Vec::len)
    }
}

/// Port of LevelTicks. Collection and callback execution are distinct phases;
/// callbacks can inspect/remove/copy the live remaining collection reentrantly.
pub struct LevelTicks<T = Block> {
    containers: crate::level::store::InsertOnlyMap<ChunkPos, LevelChunkTicks<T>>,
    next_tick_for_container: RefCell<Long2LongOpenHashMap>,
    to_run: VecDeque<ScheduledTick<T>>,
    already_run: Vec<ScheduledTick<T>>,
    next_order: i64,
    to_run_set: HashSet<(BlockPos, T)>,
}
impl<T> Default for LevelTicks<T> {
    fn default() -> Self {
        Self {
            containers: crate::level::store::InsertOnlyMap::default(),
            next_tick_for_container: RefCell::new(Long2LongOpenHashMap::new()),
            to_run: VecDeque::new(),
            already_run: Vec::new(),
            next_order: 0,
            to_run_set: HashSet::new(),
        }
    }
}
impl<T: Copy + Eq + Hash> LevelTicks<T> {
    /// Register a newly published FULL container without mutating any existing
    /// queue. Only this cold insertion borrows the dimension index dynamically;
    /// normal scheduling/collection uses exclusive get_mut access.
    pub fn publish_container(
        &self,
        chunk: ChunkPos,
        ticks: LevelChunkTicks<T>,
    ) -> Result<(), LevelChunkTicks<T>> {
        let head = ticks.peek();
        self.containers.publish(chunk, ticks)?;
        if let Some(head) = head {
            self.next_tick_for_container
                .borrow_mut()
                .insert(chunk.pack(), head.trigger_tick);
        }
        Ok(())
    }
    pub fn add_container(&mut self, chunk: ChunkPos, ticks: LevelChunkTicks<T>) {
        if let Some(head) = ticks.peek() {
            self.next_tick_for_container
                .get_mut()
                .insert(chunk.pack(), head.trigger_tick);
        }
        self.containers.insert(chunk, ticks);
    }
    pub fn remove_container(&mut self, chunk: ChunkPos) -> Option<LevelChunkTicks<T>> {
        self.next_tick_for_container.get_mut().remove(&chunk.pack());
        self.containers.remove(&chunk)
    }
    pub fn container(&self, chunk: ChunkPos) -> Option<&LevelChunkTicks<T>> {
        self.containers.get(&chunk)
    }
    /// Port of the registered LevelChunkTicks#unpack schedule callback. Saved
    /// ticks enter the dimension index only when they become the queue's head.
    pub fn unpack_container(&mut self, chunk: ChunkPos, current_tick: i64) {
        let Some(container) = self.containers.get_mut(&chunk) else {
            return;
        };
        let previous = container.peek();
        container.unpack(current_tick);
        if let Some(head) = container.peek()
            && Some(head) != previous
        {
            self.next_tick_for_container
                .get_mut()
                .insert(chunk.pack(), head.trigger_tick);
        }
    }
    pub fn schedule_tick(&mut self, tick: ScheduledTick<T>) -> bool {
        let chunk = ChunkPos::containing(tick.pos);
        let Some(container) = self.containers.get_mut(&chunk) else {
            return false;
        };
        if !container.schedule(tick) {
            return false;
        }
        if container.peek().is_some_and(|head| {
            head.pos == tick.pos
                && head.block == tick.block
                && head.trigger_tick == tick.trigger_tick
                && head.priority == tick.priority
                && head.sub_tick_order == tick.sub_tick_order
        }) {
            self.next_tick_for_container
                .get_mut()
                .insert(chunk.pack(), tick.trigger_tick);
        }
        true
    }
    /// Convenience for isolated scheduler callers. Live Level uses one shared
    /// sub-tick counter across both block and fluid schedulers.
    pub fn schedule(&mut self, pos: BlockPos, block: T, trigger_tick: i64, priority: TickPriority) {
        let chunk = ChunkPos::containing(pos);
        if !self.containers.contains_key(&chunk) {
            self.containers.insert(chunk, LevelChunkTicks::default());
        }
        let sub_tick_order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        self.schedule_tick(ScheduledTick {
            pos,
            block,
            trigger_tick,
            priority,
            sub_tick_order,
        });
    }
    pub fn collect(&mut self, game_time: i64, budget: usize, eligible: impl Fn(ChunkPos) -> bool) {
        assert!(
            self.to_run.is_empty() && self.already_run.is_empty(),
            "finish previous tick first"
        );
        let mut ready: JavaPriorityQueue<(ChunkPos, ScheduledTick<T>)> =
            JavaPriorityQueue::new(|a, b| a.1.intra_tick_order(&b.1));
        let mut cursor = self.next_tick_for_container.get_mut().cursor();
        let mut stale = Vec::new();
        while let Some((&packed, &next_time)) = cursor.next_entry() {
            if next_time > game_time {
                continue;
            }
            let pos = ChunkPos::unpack(packed);
            let Some(head) = self.containers.get(&pos).and_then(LevelChunkTicks::peek) else {
                cursor.remove();
                continue;
            };
            if head.trigger_tick > game_time {
                stale.push((packed, head.trigger_tick));
            } else if eligible(pos) {
                cursor.remove();
                ready.offer((pos, head));
            }
        }
        for (pos, time) in stale {
            self.next_tick_for_container.get_mut().insert(pos, time);
        }
        let budget = budget.min(MAX_TICKS_PER_TICK);
        while self.to_run.len() < budget {
            let Some((pos, _)) = ready.poll() else {
                break;
            };
            let container = self.containers.get_mut(&pos).unwrap();
            self.to_run.push_back(container.poll().unwrap());
            let next_best = ready.peek().map(|(_, tick)| *tick);
            while self.to_run.len() < budget {
                let Some(next) = container.peek().filter(|t| t.trigger_tick <= game_time) else {
                    break;
                };
                if next_best.is_some_and(|head| next.intra_tick_order(&head) == Ordering::Greater) {
                    break;
                }
                self.to_run.push_back(container.poll().unwrap());
            }
            if let Some(next) = container.peek() {
                if next.trigger_tick <= game_time && self.to_run.len() < budget {
                    ready.offer((pos, next));
                } else {
                    self.next_tick_for_container
                        .get_mut()
                        .insert(pos.pack(), next.trigger_tick);
                }
            }
        }
        for (pos, head) in ready.iter() {
            self.next_tick_for_container
                .get_mut()
                .insert(pos.pack(), head.trigger_tick);
        }
    }

    pub fn poll_this_tick(&mut self) -> Option<ScheduledTick<T>> {
        let tick = self.to_run.pop_front()?;
        if !self.to_run_set.is_empty() {
            self.to_run_set.remove(&(tick.pos, tick.block));
        }
        self.already_run.push(tick);
        Some(tick)
    }
    pub fn cleanup_after_tick(&mut self) {
        self.to_run.clear();
        self.already_run.clear();
        self.to_run_set.clear();
    }
    pub fn collect_due(&mut self, game_time: i64, budget: usize) -> Vec<ScheduledTick<T>> {
        self.collect(game_time, budget, |_| true);
        let mut due = Vec::new();
        while let Some(tick) = self.poll_this_tick() {
            due.push(tick);
        }
        self.cleanup_after_tick();
        due
    }
    pub fn has_scheduled_tick(&self, pos: BlockPos, block: T) -> bool {
        self.containers
            .get(&ChunkPos::containing(pos))
            .is_some_and(|c| c.has_scheduled_tick(pos, block))
    }
    pub fn will_tick_this_tick(&mut self, pos: BlockPos, block: T) -> bool {
        if self.to_run_set.is_empty() && !self.to_run.is_empty() {
            self.to_run_set
                .extend(self.to_run.iter().map(|t| (t.pos, t.block)));
        }
        self.to_run_set.contains(&(pos, block))
    }
    pub fn clear_area(&mut self, area: TickArea) {
        for x in (area.min.x >> 4)..=(area.max.x >> 4) {
            for z in (area.min.z >> 4)..=(area.max.z >> 4) {
                let pos = ChunkPos::new(x, z);
                let Some(container) = self.containers.get_mut(&pos) else {
                    continue;
                };
                let old = container.peek();
                let mut cursor = container.queue.cursor();
                while let Some(tick) = cursor.next_entry() {
                    if area.contains(tick.pos) {
                        let tick = cursor.remove().unwrap();
                        container.positions.remove(&(tick.pos, tick.block));
                    }
                }
                let head = container.peek();
                if old.map(|t| (t.pos, t.block, t.sub_tick_order))
                    != head.map(|t| (t.pos, t.block, t.sub_tick_order))
                {
                    if let Some(head) = head {
                        self.next_tick_for_container
                            .get_mut()
                            .insert(pos.pack(), head.trigger_tick);
                    } else {
                        self.next_tick_for_container.get_mut().remove(&pos.pack());
                    }
                }
            }
        }
        self.already_run.retain(|t| !area.contains(t.pos));
        self.to_run.retain(|t| !area.contains(t.pos));
        // Vanilla deliberately does not rebuild toRunThisTickSet here.
    }
    pub fn ticks_in_area(&self, area: TickArea) -> Vec<ScheduledTick<T>> {
        let mut ticks: Vec<_> = self
            .already_run
            .iter()
            .chain(self.to_run.iter())
            .filter(|t| area.contains(t.pos))
            .copied()
            .collect();
        for x in (area.min.x >> 4)..=(area.max.x >> 4) {
            for z in (area.min.z >> 4)..=(area.max.z >> 4) {
                if let Some(container) = self.containers.get(&ChunkPos::new(x, z)) {
                    ticks.extend(
                        container
                            .queue
                            .iter()
                            .filter(|t| area.contains(t.pos))
                            .copied(),
                    );
                }
            }
        }
        ticks
    }
    pub fn copy_ticks(&mut self, ticks: &[ScheduledTick<T>], offset: BlockPos) {
        let Some(min) = ticks.iter().map(|t| t.sub_tick_order).min() else {
            return;
        };
        let max = ticks.iter().map(|t| t.sub_tick_order).max().unwrap();
        for &tick in ticks {
            self.schedule_tick(ScheduledTick {
                pos: tick.pos.offset(offset.x, offset.y, offset.z),
                sub_tick_order: tick
                    .sub_tick_order
                    .wrapping_sub(min)
                    .wrapping_add(max)
                    .wrapping_add(1),
                ..tick
            });
        }
    }
    pub fn copy_area_from(&mut self, source: &Self, area: TickArea, offset: BlockPos) {
        self.copy_ticks(&source.ticks_in_area(area), offset);
    }
    pub fn copy_area(&mut self, area: TickArea, offset: BlockPos) {
        let ticks = self.ticks_in_area(area);
        self.copy_ticks(&ticks, offset);
    }
    pub fn count(&self) -> usize {
        self.containers.values().map(LevelChunkTicks::count).sum()
    }
}

/// Inclusive block bounds used by LevelTicks#clearArea/copyAreaFrom.
#[derive(Clone, Copy, Debug)]
pub struct TickArea {
    pub min: BlockPos,
    pub max: BlockPos,
}
impl TickArea {
    pub fn contains(self, pos: BlockPos) -> bool {
        (self.min.x..=self.max.x).contains(&pos.x)
            && (self.min.y..=self.max.y).contains(&pos.y)
            && (self.min.z..=self.max.z).contains(&pos.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: BlockPos = BlockPos::new(0, 0, 0);
    const B: BlockPos = BlockPos::new(1, 0, 0);
    const C: BlockPos = BlockPos::new(16, 0, 0);
    fn vector(name: &str) -> &'static str {
        include_str!("../tests/java/level/ticks.tsv")
            .lines()
            .find_map(|line| line.strip_prefix(name).and_then(|s| s.strip_prefix('\t')))
            .unwrap()
    }
    fn label(pos: BlockPos) -> &'static str {
        if pos == A {
            "A"
        } else if pos == B {
            "B"
        } else {
            "C"
        }
    }
    #[test]
    fn vanilla_container_drain_and_deduplication() {
        let mut ticks = LevelTicks::default();
        ticks.schedule(A, Block::STONE, 1, TickPriority::Low);
        ticks.schedule(B, Block::STONE, 2, TickPriority::ExtremelyHigh);
        ticks.schedule(C, Block::STONE, 3, TickPriority::Normal);
        ticks.schedule(A, Block::STONE, 0, TickPriority::ExtremelyHigh);
        let mut due = ticks.collect_due(3, 1);
        due.extend(ticks.collect_due(3, 65536));
        assert_eq!(
            due.into_iter()
                .map(|t| label(t.pos))
                .collect::<Vec<_>>()
                .join(","),
            vector("container-order")
        );
    }
    #[test]
    fn vanilla_reentrant_collection_and_will_tick() {
        let mut ticks = LevelTicks::default();
        ticks.schedule(A, Block::STONE, 4, TickPriority::Normal);
        ticks.schedule(B, Block::STONE, 4, TickPriority::Normal);
        ticks.collect(4, 65536, |_| true);
        let mut output = Vec::new();
        while let Some(tick) = ticks.poll_this_tick() {
            output.push(format!(
                "{}:{}",
                label(tick.pos),
                ticks.will_tick_this_tick(B, Block::STONE)
            ));
            if tick.pos == A {
                ticks.schedule(C, Block::STONE, 4, TickPriority::Normal);
            }
        }
        ticks.cleanup_after_tick();
        for tick in ticks.collect_due(4, 65536) {
            output.push(label(tick.pos).into());
        }
        assert_eq!(output.join(","), vector("reentrant-collection"));
    }
    #[test]
    fn vanilla_saved_ticks_pack_and_negative_unpack_order() {
        let mut ticks = LevelChunkTicks::from_saved(
            vec![
                SavedTick {
                    block: Block::STONE,
                    pos: A,
                    delay: 5,
                    priority: TickPriority::Normal,
                },
                SavedTick {
                    block: Block::STONE,
                    pos: B,
                    delay: 2,
                    priority: TickPriority::High,
                },
            ],
            ChunkPos::new(0, 0),
        );
        assert!(!ticks.schedule(ScheduledTick {
            block: Block::STONE,
            pos: A,
            trigger_tick: 0,
            priority: TickPriority::ExtremelyHigh,
            sub_tick_order: 0
        }));
        ticks.unpack(10);
        assert_eq!(
            ticks
                .pack(10)
                .into_iter()
                .map(|t| format!("{}:{}:{}", label(t.pos), t.delay, t.priority as i32))
                .collect::<Vec<_>>()
                .join(","),
            vector("saved-pack")
        );
        let mut output = Vec::new();
        while let Some(tick) = ticks.poll() {
            output.push(format!(
                "{}:{}:{}",
                label(tick.pos),
                tick.trigger_tick,
                tick.sub_tick_order
            ));
        }
        assert_eq!(output.join(","), vector("saved-unpack"));
    }
    #[test]
    fn registered_saved_ticks_wait_for_start_ticking_and_keep_existing_heads() {
        let chunk = ChunkPos::new(0, 0);
        let mut ticks = LevelTicks::default();
        ticks.add_container(
            chunk,
            LevelChunkTicks::from_saved(
                vec![SavedTick {
                    block: Block::STONE,
                    pos: A,
                    delay: 7,
                    priority: TickPriority::Normal,
                }],
                chunk,
            ),
        );
        let pending = ticks.collect_due(100, 65536).len();
        let delay = ticks.container(chunk).unwrap().pack(200)[0].delay;
        assert_eq!(format!("{delay}:{pending}"), vector("registered-pending"));
        ticks.schedule(B, Block::STONE, 105, TickPriority::Normal);
        ticks.unpack_container(chunk, 200);
        ticks.unpack_container(chunk, 500);
        let mut output = Vec::new();
        for time in [200, 206, 207] {
            for tick in ticks.collect_due(time, 65536) {
                output.push(format!("{}@{time}", label(tick.pos)));
            }
        }
        assert_eq!(output.join(","), vector("late-start-ticking"));
    }
    #[test]
    fn ineligible_containers_keep_ticks_and_unregistered_chunks_reject_them() {
        let mut ticks = LevelTicks::default();
        ticks.schedule(A, Block::STONE, 0, TickPriority::Normal);
        ticks.collect(1, 65536, |_| false);
        assert!(ticks.poll_this_tick().is_none());
        ticks.cleanup_after_tick();
        let container = ticks.remove_container(ChunkPos::new(0, 0)).unwrap();
        assert!(!ticks.schedule_tick(ScheduledTick {
            block: Block::STONE,
            pos: A,
            trigger_tick: 1,
            priority: TickPriority::Normal,
            sub_tick_order: 1
        }));
        ticks.add_container(ChunkPos::new(0, 0), container);
        assert_eq!(ticks.collect_due(1, 65536).len(), 1);
    }
    #[test]
    fn saved_tick_nbt_retains_signed_delay_priority_and_chunk_filter() {
        let saved = SavedTick {
            block: Block::STONE,
            pos: A,
            delay: -17,
            priority: TickPriority::ExtremelyLow,
        };
        let nbt = saved.to_nbt("minecraft:stone");
        let decoded = SavedTick::from_nbt(&nbt, Block::STONE).unwrap();
        assert_eq!(decoded.delay, -17);
        assert_eq!(decoded.priority, TickPriority::ExtremelyLow);
        assert_eq!(
            LevelChunkTicks::from_saved(vec![decoded], ChunkPos::new(1, 0)).count(),
            0
        );
    }
    #[test]
    fn equal_loaded_container_heads_match_fastutil_and_jdk_tie_order() {
        let mut ticks = LevelTicks::default();
        for x in -10..=10 {
            let pos = BlockPos::new(x * 16, 0, 0);
            let chunk = ChunkPos::new(x, 0);
            let mut container = LevelChunkTicks::from_saved(
                vec![SavedTick {
                    block: Block::STONE,
                    pos,
                    delay: 0,
                    priority: TickPriority::Normal,
                }],
                chunk,
            );
            container.unpack(0);
            ticks.add_container(chunk, container);
        }
        let mut output = Vec::new();
        for _ in 0..5 {
            output.extend(
                ticks
                    .collect_due(0, 5)
                    .into_iter()
                    .map(|t| t.pos.x.to_string()),
            );
        }
        assert_eq!(output.join(","), vector("equal-loaded-heads"));
    }
    #[test]
    fn vanilla_area_copy_clear_and_lazy_will_tick_cache() {
        let mut ticks = LevelTicks::default();
        for x in 0..=3 {
            ticks.add_container(ChunkPos::new(x, 0), LevelChunkTicks::default());
        }
        ticks.schedule(A, Block::STONE, 0, TickPriority::Normal);
        ticks.schedule(B, Block::STONE, 0, TickPriority::Normal);
        ticks.schedule(C, Block::STONE, 0, TickPriority::Normal);
        ticks.schedule(
            BlockPos::new(2, 0, 0),
            Block::STONE,
            9,
            TickPriority::Normal,
        );
        ticks.collect(0, 65536, |_| true);
        let mut output = Vec::new();
        while let Some(tick) = ticks.poll_this_tick() {
            output.push(tick.pos.x.to_string());
            if tick.pos == A {
                output.push(format!(
                    "before:{}",
                    ticks.will_tick_this_tick(B, Block::STONE)
                ));
                ticks.copy_area(TickArea { min: A, max: C }, BlockPos::new(32, 0, 0));
                ticks.clear_area(TickArea { min: A, max: B });
                output.push(format!(
                    "after:{}",
                    ticks.will_tick_this_tick(B, Block::STONE)
                ));
            }
        }
        ticks.cleanup_after_tick();
        output.extend(
            ticks
                .collect_due(0, 65536)
                .into_iter()
                .map(|t| t.pos.x.to_string()),
        );
        output.extend(
            ticks
                .collect_due(9, 65536)
                .into_iter()
                .map(|t| t.pos.x.to_string()),
        );
        assert_eq!(output.join(","), vector("area-copy-clear"));
    }
}
