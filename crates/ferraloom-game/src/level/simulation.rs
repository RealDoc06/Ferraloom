//! Port of SimulationChunkTracker, ChunkTracker and DynamicGraphMinFixedPoint.
//! The primitive map's complete contents determine random-chunk traversal order;
//! a sorted list or a map containing only eligible chunks cannot substitute.
use ferraloom_util::{
    ChunkPos,
    collections::{Long2ByteOpenHashMap, LongLinkedOpenHashSet},
};
const INVALID_CHUNK: i64 = 1_875_066_i64 | (1_875_066_i64 << 32);

pub struct SimulationChunkTracker {
    graph: ChunkTracker,
}
impl Default for SimulationChunkTracker {
    fn default() -> Self {
        Self {
            graph: ChunkTracker::new(34, 33, 32, 33),
        }
    }
}
impl SimulationChunkTracker {
    pub fn level(&self, pos: ChunkPos) -> i32 {
        self.graph.get_level(pos.pack())
    }
    /// Supply TicketStorage's minimum simulation level. Players sharing one
    /// chunk share one PLAYER_SIMULATION source ticket.
    pub fn set_source_level(&mut self, pos: ChunkPos, level: Option<i32>) {
        self.graph.set_source_level(pos, level);
    }
    /// Apply each TicketStorage listener notification while graph work is
    /// queued, including non-minimal ticket removals.
    pub fn update_source_level(&mut self, pos: ChunkPos, level: i32, only_decreased: bool) {
        self.graph.update_source_level(pos, level, only_decreased);
    }
    pub fn entity_ticking_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.graph
            .chunks
            .iter()
            .filter(|(_, level)| **level <= 31)
            .map(|(&pos, _)| ChunkPos::unpack(pos))
    }
    pub fn block_ticking_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.graph
            .chunks
            .iter()
            .filter(|(_, level)| **level <= 32)
            .map(|(&pos, _)| ChunkPos::unpack(pos))
    }
    pub fn run_all_updates(&mut self) {
        self.graph.run_all_updates();
    }
    /// Construct explicit simulation membership for manually owned test chunks.
    /// Production changes always enter through TicketStorage's source updates.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub(super) fn set_fixture_level(&mut self, pos: ChunkPos, level: i32) {
        self.graph.set_level(pos.pack(), level);
    }
}

/// One LoadingChunkTracker#setLevel call in fixed-point queue order. Holder
/// scheduling consumes these changes before its full-status future pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkLevelChange {
    pub pos: ChunkPos,
    pub old_level: i32,
    pub level: i32,
}

/// Port of LoadingChunkTracker. The generation pipeline supplies ChunkLevel's
/// maximum loaded level; its dependency radius is independent of simulation.
pub struct LoadingChunkTracker {
    graph: ChunkTracker,
}
impl LoadingChunkTracker {
    pub fn new(max_loaded_level: i32) -> Self {
        assert!((33..=125).contains(&max_loaded_level));
        let missing = max_loaded_level + 1;
        let mut graph = ChunkTracker::new(missing + 1, missing, max_loaded_level, missing);
        graph.changes = Some(Vec::new());
        Self { graph }
    }
    pub fn level(&self, pos: ChunkPos) -> i32 {
        self.graph.get_level(pos.pack())
    }
    pub fn update_source_level(&mut self, pos: ChunkPos, level: i32, only_decreased: bool) {
        self.graph.update_source_level(pos, level, only_decreased);
    }
    pub fn run_all_updates(&mut self) {
        self.graph.run_all_updates();
    }
    pub fn take_level_changes(&mut self) -> Vec<ChunkLevelChange> {
        std::mem::take(self.graph.changes.as_mut().unwrap())
    }
}

/// Port of DistanceManager.FixedPlayerDistanceChunkTracker. Its missing level
/// is maxDistance+2, while the graph saturates at maxDistance+1. Preserving
/// that distinction also preserves the primitive map's insertion/removal order.
pub struct FixedPlayerDistanceChunkTracker {
    graph: ChunkTracker,
}
impl Default for FixedPlayerDistanceChunkTracker {
    fn default() -> Self {
        Self::new(8)
    }
}
impl FixedPlayerDistanceChunkTracker {
    pub fn new(max_distance: i32) -> Self {
        assert!((0..=125).contains(&max_distance));
        Self {
            graph: ChunkTracker::new(max_distance + 2, max_distance + 2, max_distance, i32::MAX),
        }
    }
    /// Call on first-player entry and last-player exit, in vanilla transition
    /// order. Duplicate presence changes do not enqueue redundant graph work.
    pub fn update_player_chunk(&mut self, pos: ChunkPos, present: bool) {
        self.graph.set_source_level(pos, present.then_some(0));
    }
    pub fn update_source_level(&mut self, pos: ChunkPos, level: i32, only_decreased: bool) {
        self.graph.update_source_level(pos, level, only_decreased);
    }
    pub fn run_all_updates(&mut self) {
        self.graph.run_all_updates();
    }
    pub fn level(&self, pos: ChunkPos) -> i32 {
        self.graph.get_level(pos.pack())
    }
    pub fn chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.graph
            .chunks
            .iter()
            .map(|(&pos, _)| ChunkPos::unpack(pos))
    }
    pub fn count(&self) -> usize {
        self.graph.chunks.len()
    }
}

/// DistanceManager.PlayerTicketTracker on the shared fixed-point graph.
/// Ticket changes are submitted to the native four-priority throttler; the
/// runtime rechecks have_ticket_for before applying each Add operation.
pub struct PlayerTicketTracker {
    graph: ChunkTracker,
    view_distance: i32,
    to_update: ferraloom_util::collections::LongOpenHashSet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerLoadingTask {
    Add(ChunkPos),
    Remove(ChunkPos),
}
impl Default for PlayerTicketTracker {
    fn default() -> Self {
        let mut graph = ChunkTracker::new(34, 34, 32, i32::MAX);
        graph.changes = Some(Vec::new());
        Self {
            graph,
            view_distance: 0,
            to_update: ferraloom_util::collections::LongOpenHashSet::new(),
        }
    }
}
impl PlayerTicketTracker {
    pub fn update_player_chunk(&mut self, pos: ChunkPos, present: bool) {
        self.graph
            .update_source_level(pos, if present { 0 } else { i32::MAX }, present);
    }
    pub fn has_ticket(&self, pos: ChunkPos) -> bool {
        self.graph.get_level(pos.pack()) <= self.view_distance
    }
    fn change_ticket(
        dispatcher: &mut super::chunk_task_dispatcher::ThrottlingChunkTaskDispatcher<
            PlayerLoadingTask,
        >,
        pos: ChunkPos,
        level: i32,
        saw: bool,
        sees: bool,
    ) {
        if saw == sees {
            return;
        }
        if sees {
            dispatcher.submit(pos, level, PlayerLoadingTask::Add(pos));
        } else {
            dispatcher.release(pos, true, Some(PlayerLoadingTask::Remove(pos)));
        }
    }
    pub fn update_view_distance(
        &mut self,
        distance: i32,
        dispatcher: &mut super::chunk_task_dispatcher::ThrottlingChunkTaskDispatcher<
            PlayerLoadingTask,
        >,
    ) {
        for (&key, &level) in self.graph.chunks.iter() {
            let level = i32::from(level);
            Self::change_ticket(
                dispatcher,
                ChunkPos::unpack(key),
                level,
                level <= self.view_distance,
                level <= distance,
            );
        }
        self.view_distance = distance;
    }
    pub fn run_all_updates(
        &mut self,
        dispatcher: &mut super::chunk_task_dispatcher::ThrottlingChunkTaskDispatcher<
            PlayerLoadingTask,
        >,
    ) {
        self.graph.run_all_updates();
        for change in std::mem::take(self.graph.changes.as_mut().unwrap()) {
            self.to_update.insert(change.pos.pack());
        }
        for &key in self.to_update.iter() {
            let pos = ChunkPos::unpack(key);
            let old = dispatcher.queue_level(pos);
            let level = self.graph.get_level(key);
            if old != level {
                dispatcher.on_level_change(pos, level);
                Self::change_ticket(
                    dispatcher,
                    pos,
                    level,
                    old <= self.view_distance,
                    level <= self.view_distance,
                );
            }
        }
        self.to_update.clear();
    }
}

/// Concrete shared ChunkTracker/DynamicGraphMinFixedPoint queue. Maps are
/// lookups plus the exact fastutil chunk iteration; queues retain FIFO order.
struct ChunkTracker {
    max_level: i32,
    default_level: i32,
    retained_level: i32,
    source_default: i32,
    chunks: Long2ByteOpenHashMap,
    sources: Long2ByteOpenHashMap,
    computed: Long2ByteOpenHashMap,
    queues: Vec<LongLinkedOpenHashSet>,
    changes: Option<Vec<ChunkLevelChange>>,
}
impl ChunkTracker {
    fn new(level_count: i32, default_level: i32, retained_level: i32, source_default: i32) -> Self {
        Self {
            max_level: level_count - 1,
            default_level,
            retained_level,
            source_default,
            changes: None,
            chunks: Long2ByteOpenHashMap::new(),
            sources: Long2ByteOpenHashMap::new(),
            computed: Long2ByteOpenHashMap::with_capacity_and_load_factor(256, 0.5),
            queues: (0..level_count)
                .map(|_| LongLinkedOpenHashSet::with_capacity_and_load_factor(16, 0.5))
                .collect(),
        }
    }
    fn set_source_level(&mut self, pos: ChunkPos, level: Option<i32>) {
        let key = pos.pack();
        let old = self
            .sources
            .get(&key)
            .copied()
            .map(i32::from)
            .unwrap_or(self.max_level);
        let new = level.unwrap_or(self.max_level).clamp(0, self.max_level);
        if new == old {
            return;
        }
        self.update_source_level(pos, new, new < old);
    }
    fn update_source_level(&mut self, pos: ChunkPos, level: i32, only_decreased: bool) {
        let key = pos.pack();
        let level = level.clamp(0, self.max_level);
        if level < self.max_level {
            self.sources.insert(key, level as i8);
        } else {
            self.sources.remove(&key);
        }
        self.check_edge(INVALID_CHUNK, key, level, only_decreased);
    }
    fn run_all_updates(&mut self) {
        while let Some(priority) = self.queues.iter().position(|queue| !queue.is_empty()) {
            let node = self.queues[priority].pop_first().unwrap();
            let level = self.get_level(node).clamp(0, self.max_level);
            let computed = i32::from(
                self.computed
                    .remove(&node)
                    .expect("queued graph node has a computed level"),
            );
            if computed < level {
                self.set_level(node, computed);
                self.check_neighbors_after_update(node, computed, true);
            } else if computed > level {
                self.set_level(node, self.max_level);
                if computed != self.max_level {
                    self.enqueue(node, self.max_level.min(computed));
                    self.computed.insert(node, computed as i8);
                }
                self.check_neighbors_after_update(node, level, false);
            }
        }
    }
    fn get_level(&self, node: i64) -> i32 {
        self.chunks
            .get(&node)
            .copied()
            .map(i32::from)
            .unwrap_or(self.default_level)
    }
    fn set_level(&mut self, node: i64, level: i32) {
        let old_level = self.get_level(node);
        if let Some(changes) = &mut self.changes
            && old_level != level
        {
            changes.push(ChunkLevelChange {
                pos: ChunkPos::unpack(node),
                old_level,
                level,
            });
        }
        if level > self.retained_level {
            self.chunks.remove(&node);
        } else {
            self.chunks.insert(node, level as i8);
        }
    }
    fn level_from_neighbor(&self, from: i64, to: i64, level: i32) -> i32 {
        if from == INVALID_CHUNK {
            self.sources
                .get(&to)
                .copied()
                .map(i32::from)
                .unwrap_or(self.source_default)
        } else {
            level + 1
        }
    }
    fn computed_level(&self, node: i64, known_parent: i64, known_level: i32) -> i32 {
        let mut computed = known_level;
        let pos = ChunkPos::unpack(node);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let mut neighbor =
                    ChunkPos::new(pos.x.wrapping_add(dx), pos.z.wrapping_add(dz)).pack();
                if neighbor == node {
                    neighbor = INVALID_CHUNK;
                }
                if neighbor != known_parent {
                    computed = computed.min(self.level_from_neighbor(
                        neighbor,
                        node,
                        self.get_level(neighbor),
                    ));
                    if computed == 0 {
                        return 0;
                    }
                }
            }
        }
        computed
    }
    fn enqueue(&mut self, node: i64, priority: i32) {
        self.queues[priority as usize].insert(node);
    }
    fn dequeue(&mut self, node: i64, priority: i32) {
        self.queues[priority as usize].remove(&node);
    }
    fn check_edge(&mut self, from: i64, to: i64, new_from: i32, only_decreased: bool) {
        let level_to = self.get_level(to);
        let stored = self.computed.get(&to).copied().map(i32::from);
        self.check_edge_at_level(from, to, new_from, level_to, stored, only_decreased);
    }
    fn check_edge_at_level(
        &mut self,
        from: i64,
        to: i64,
        new_from: i32,
        level_to: i32,
        stored: Option<i32>,
        only_decreased: bool,
    ) {
        if to == INVALID_CHUNK {
            return;
        }
        let new_from = new_from.clamp(0, self.max_level);
        let level_to = level_to.clamp(0, self.max_level);
        let old_computed = stored.unwrap_or(level_to);
        let new_computed = if only_decreased {
            old_computed.min(new_from)
        } else {
            self.computed_level(to, from, new_from)
                .clamp(0, self.max_level)
        };
        let old_priority = level_to.min(old_computed).min(self.max_level);
        if level_to != new_computed {
            let priority = level_to.min(new_computed).min(self.max_level);
            if old_priority != priority && stored.is_some() {
                self.dequeue(to, old_priority);
            }
            self.enqueue(to, priority);
            self.computed.insert(to, new_computed as i8);
        } else if stored.is_some() {
            self.dequeue(to, old_priority);
            self.computed.remove(&to);
        }
    }
    fn check_neighbor(&mut self, from: i64, to: i64, level: i32, only_decreased: bool) {
        let stored = self.computed.get(&to).copied().map(i32::from);
        let level_from = self
            .level_from_neighbor(from, to, level)
            .clamp(0, self.max_level);
        if only_decreased {
            self.check_edge_at_level(from, to, level_from, self.get_level(to), stored, true);
        } else {
            let old_computed =
                stored.unwrap_or_else(|| self.get_level(to).clamp(0, self.max_level));
            if level_from == old_computed {
                let level_to = if stored.is_none() {
                    old_computed
                } else {
                    self.get_level(to)
                };
                self.check_edge_at_level(from, to, self.max_level, level_to, stored, false);
            }
        }
    }
    fn check_neighbors_after_update(&mut self, node: i64, level: i32, only_decreased: bool) {
        if only_decreased && level >= self.max_level - 1 {
            return;
        }
        let pos = ChunkPos::unpack(node);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let neighbor = ChunkPos::new(pos.x.wrapping_add(dx), pos.z.wrapping_add(dz)).pack();
                if neighbor != node {
                    self.check_neighbor(node, neighbor, level, only_decreased);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loading_tracker_queue_changes_match_real_holder_scheduling() {
        use crate::level::distance_manager::MAX_LOADED_LEVEL;
        let vector = |name: &str| {
            include_str!("../../tests/java/level/loading.tsv")
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once('\t')?;
                    (key == name).then_some(value)
                })
                .unwrap()
        };
        assert_eq!(MAX_LOADED_LEVEL.to_string(), vector("max-loaded-level"));
        let mut tracker = LoadingChunkTracker::new(MAX_LOADED_LEVEL);
        for (stage, changes) in [
            ("origin", vec![(ChunkPos::new(0, 0), 31, true)]),
            ("overlap", vec![(ChunkPos::new(2, -1), 32, true)]),
            (
                "remove-origin",
                vec![(ChunkPos::new(0, 0), MAX_LOADED_LEVEL + 1, false)],
            ),
            (
                "empty",
                vec![(ChunkPos::new(2, -1), MAX_LOADED_LEVEL + 1, false)],
            ),
        ] {
            for (pos, level, down) in changes {
                tracker.update_source_level(pos, level, down);
            }
            tracker.run_all_updates();
            let actual = tracker
                .take_level_changes()
                .iter()
                .map(|change| format!("{},{},{}", change.pos.x, change.pos.z, change.level))
                .collect::<Vec<_>>()
                .join(";");
            assert_eq!(actual, vector(stage), "{stage}");
        }
        assert_eq!(tracker.level(ChunkPos::new(0, 0)), MAX_LOADED_LEVEL + 1);
    }
    #[test]
    fn simulation_tracker_map_order_and_levels_match_vanilla_ticket_changes() {
        let mut tracker = SimulationChunkTracker::default();
        for (stage, changes) in [
            ("origin", vec![(ChunkPos::new(0, 0), Some(30))]),
            ("overlap", vec![(ChunkPos::new(2, -1), Some(30))]),
            ("remove-origin", vec![(ChunkPos::new(0, 0), None)]),
            (
                "move",
                vec![
                    (ChunkPos::new(2, -1), None),
                    (ChunkPos::new(-2, 1), Some(29)),
                ],
            ),
            ("empty", vec![(ChunkPos::new(-2, 1), None)]),
        ] {
            for (pos, level) in changes {
                tracker.set_source_level(pos, level);
            }
            tracker.run_all_updates();
            let actual = tracker
                .graph
                .chunks
                .iter()
                .map(|(&key, &level)| {
                    let p = ChunkPos::unpack(key);
                    format!("{},{},{level}", p.x, p.z)
                })
                .collect::<Vec<_>>()
                .join(";");
            let expected = include_str!("../../tests/java/level/simulation.tsv")
                .lines()
                .find_map(|line| line.strip_prefix(stage).and_then(|s| s.strip_prefix('\t')))
                .unwrap();
            assert_eq!(actual, expected, "{stage}");
        }
    }

    #[test]
    fn natural_spawn_counter_order_and_shuffle_match_real_distance_manager() {
        use crate::level::{Level, Region};
        use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let vector = |name: &str| {
            include_str!("../../tests/java/level/spawn-chunks.tsv")
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once('\t')?;
                    (key == name).then_some(value)
                })
                .unwrap()
        };
        let mut tracker = FixedPlayerDistanceChunkTracker::default();
        for (stage, changes) in [
            ("origin", vec![(ChunkPos::new(0, 0), true)]),
            ("overlap", vec![(ChunkPos::new(2, -1), true)]),
            ("remove-origin", vec![(ChunkPos::new(0, 0), false)]),
            (
                "move",
                vec![(ChunkPos::new(2, -1), false), (ChunkPos::new(-2, 1), true)],
            ),
            ("empty", vec![(ChunkPos::new(-2, 1), false)]),
        ] {
            for (pos, present) in changes {
                tracker.update_player_chunk(pos, present);
            }
            tracker.run_all_updates();
            let actual = tracker
                .chunks()
                .map(|pos| format!("{},{},{}", pos.x, pos.z, tracker.level(pos)))
                .collect::<Vec<_>>()
                .join(";");
            assert_eq!(actual, vector(stage), "{stage}");
            if stage == "origin" {
                assert_eq!(
                    tracker.level(ChunkPos::new(100, 100)).to_string(),
                    vector("missing-level")
                );
                assert_eq!(tracker.count(), 17 * 17);
                let mut spawning = tracker.chunks().collect::<Vec<_>>();
                let mut level = Level::new(
                    Region::new(&BEHAVIORS),
                    crate::level::LevelContext::fixture(0, 63),
                );
                level.set_random_streams(Random::Legacy(LegacyRandomSource::new(0)), 0);
                level.shuffle_spawning_chunks(&mut spawning);
                let actual = spawning
                    .iter()
                    .map(|pos| format!("{},{}", pos.x, pos.z))
                    .collect::<Vec<_>>()
                    .join(";");
                assert_eq!(
                    format!("{actual}|{}", level.random().next_int()),
                    vector("shuffled-origin")
                );
            }
        }
    }
}
