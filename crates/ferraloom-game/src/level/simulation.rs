//! Port of SimulationChunkTracker, ChunkTracker and DynamicGraphMinFixedPoint.
//! The primitive map's complete contents determine random-chunk traversal order;
//! a sorted list or a map containing only eligible chunks cannot substitute.
use ferraloom_util::{
    ChunkPos,
    collections::{Long2ByteOpenHashMap, LongLinkedOpenHashSet},
};
const MAX_LEVEL: i32 = 33;
const INVALID_CHUNK: i64 = 1_875_066_i64 | (1_875_066_i64 << 32);

pub struct SimulationChunkTracker {
    chunks: Long2ByteOpenHashMap,
    sources: Long2ByteOpenHashMap,
    computed: Long2ByteOpenHashMap,
    queues: [LongLinkedOpenHashSet; 34],
}
impl Default for SimulationChunkTracker {
    fn default() -> Self {
        Self {
            chunks: Long2ByteOpenHashMap::new(),
            sources: Long2ByteOpenHashMap::new(),
            computed: Long2ByteOpenHashMap::with_capacity_and_load_factor(256, 0.5),
            queues: std::array::from_fn(|_| {
                LongLinkedOpenHashSet::with_capacity_and_load_factor(16, 0.5)
            }),
        }
    }
}
impl SimulationChunkTracker {
    pub fn level(&self, pos: ChunkPos) -> i32 {
        self.get_level(pos.pack())
    }
    /// Supply TicketStorage's minimum simulation ticket level at this position.
    /// Multiple players in one chunk share one PLAYER_SIMULATION source ticket.
    pub fn set_source_level(&mut self, pos: ChunkPos, level: Option<i32>) {
        let key = pos.pack();
        let old = self
            .sources
            .get(&key)
            .copied()
            .map(i32::from)
            .unwrap_or(MAX_LEVEL);
        let new = level.unwrap_or(MAX_LEVEL).clamp(0, MAX_LEVEL);
        if new == old {
            return;
        }
        if level.is_some() {
            self.sources.insert(key, new as i8);
        } else {
            self.sources.remove(&key);
        }
        self.check_edge(INVALID_CHUNK, key, new, new < old);
    }
    pub fn entity_ticking_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks
            .iter()
            .filter(|(_, level)| **level <= 31)
            .map(|(&pos, _)| ChunkPos::unpack(pos))
    }
    pub fn block_ticking_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks
            .iter()
            .filter(|(_, level)| **level <= 32)
            .map(|(&pos, _)| ChunkPos::unpack(pos))
    }
    pub fn run_all_updates(&mut self) {
        while let Some(priority) = self.queues.iter().position(|queue| !queue.is_empty()) {
            let node = self.queues[priority].pop_first().unwrap();
            let level = self.get_level(node).clamp(0, MAX_LEVEL);
            let computed = i32::from(
                self.computed
                    .remove(&node)
                    .expect("queued graph node has a computed level"),
            );
            if computed < level {
                self.set_level(node, computed);
                self.check_neighbors_after_update(node, computed, true);
            } else if computed > level {
                self.set_level(node, MAX_LEVEL);
                if computed != MAX_LEVEL {
                    self.enqueue(node, MAX_LEVEL.min(computed));
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
            .unwrap_or(MAX_LEVEL)
    }
    fn set_level(&mut self, node: i64, level: i32) {
        if level >= MAX_LEVEL {
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
                .unwrap_or(MAX_LEVEL)
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
        let new_from = new_from.clamp(0, MAX_LEVEL);
        let level_to = level_to.clamp(0, MAX_LEVEL);
        let old_computed = stored.unwrap_or(level_to);
        let new_computed = if only_decreased {
            old_computed.min(new_from)
        } else {
            self.computed_level(to, from, new_from).clamp(0, MAX_LEVEL)
        };
        let old_priority = level_to.min(old_computed).min(MAX_LEVEL);
        if level_to != new_computed {
            let priority = level_to.min(new_computed).min(MAX_LEVEL);
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
            .clamp(0, MAX_LEVEL);
        if only_decreased {
            self.check_edge_at_level(from, to, level_from, self.get_level(to), stored, true);
        } else {
            let old_computed = stored.unwrap_or_else(|| self.get_level(to).clamp(0, MAX_LEVEL));
            if level_from == old_computed {
                let level_to = if stored.is_none() {
                    old_computed
                } else {
                    self.get_level(to)
                };
                self.check_edge_at_level(from, to, MAX_LEVEL, level_to, stored, false);
            }
        }
    }
    fn check_neighbors_after_update(&mut self, node: i64, level: i32, only_decreased: bool) {
        if only_decreased && level >= 32 {
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
}
