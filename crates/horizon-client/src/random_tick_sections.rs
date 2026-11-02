//! Which sections of the game thread's chunks random-tick
//! (`LevelChunkSection.isRandomlyTicking`), memoized per section.
use crate::{
    blocks::BlockFacts,
    chunk::{Chunk, MIN_Y, SECTIONS, Section, index},
    fast_hash::FxHashMap,
};
use horizon_core::position::BlockPos;
use horizon_world::{
    level::{Behaviours, RandomTickSource},
    registry::BlockStateId,
};
use std::sync::{Arc, Weak};

/// A value snapshot, in ascending section order, with no per-chunk allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sections(u32);
impl Sections {
    /// The minimum Y of the lowest section in the set from block `min_y` up.
    pub fn from(self, min_y: i32) -> Option<i32> {
        let index = ((min_y - MIN_Y) >> 4).max(0) as u32;
        Sections(self.0 & u32::MAX.checked_shl(index).unwrap_or(0)).next()
    }
}
impl Iterator for Sections {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(MIN_Y + index as i32 * 16)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}
impl ExactSizeIterator for Sections {}

struct Cached {
    chunk: Weak<Chunk>,
    /// The section each bit of `sections` was read from.
    read: [Weak<Section>; SECTIONS],
    sections: Sections,
    /// For the sections in `sections`: which of the 4,096 blocks (bit
    /// `chunk::index`) `Sim::random_tick_state` acts on.
    positions: [Option<Box<[u64; 64]>>; SECTIONS],
}
impl Cached {
    /// Reads the sections that are no longer the ones read. `random` is
    /// `isRandomlyTicking`'s test of a state, `acts` the random tick's own.
    fn refresh(
        &mut self,
        chunk: &Arc<Chunk>,
        mut random: impl FnMut(u32) -> bool,
        mut acts: impl FnMut(u32) -> bool,
    ) {
        const {
            assert!(SECTIONS <= u32::BITS as usize);
        }
        if self.chunk.as_ptr() == Arc::as_ptr(chunk) {
            return;
        }
        for (i, section) in chunk.sections.iter().enumerate() {
            if self.read[i].as_ptr() == Arc::as_ptr(section) {
                continue;
            }
            // `isRandomlyTicking`: the blocks themselves, not the palette,
            // which keeps the states no block uses any more.
            if section.blocks.has(&mut random) {
                self.sections.0 |= 1 << i;
                let positions = self.positions[i].get_or_insert_with(|| Box::new([0; 64]));
                section.blocks.fill_matching(&mut acts, &mut positions[..]);
            } else {
                self.sections.0 &= !(1 << i);
                self.positions[i] = None;
            }
            self.read[i] = Arc::downgrade(section);
        }
        self.chunk = Arc::downgrade(chunk);
    }
}
impl Default for Cached {
    fn default() -> Self {
        Self {
            chunk: Weak::new(),
            read: std::array::from_fn(|_| Weak::new()),
            sections: Sections::default(),
            positions: std::array::from_fn(|_| None),
        }
    }
}
/// Vanilla keeps a count of randomly ticking blocks and fluids per section,
/// updated by every `setBlockState`, and ticks the sections where it is above
/// zero. Here a section's answer is read from its blocks after any edit:
/// resident chunks and sections are immutable behind `Arc` and edited through
/// `Arc::make_mut`, so a weak reference tells an edited or replaced one from
/// the one read, without retaining block data or forcing a deep copy on the
/// next edit, and prevents address reuse. An unchanged chunk costs one
/// pointer comparison; a changed one, a comparison per section and a read of
/// the sections that changed. Clear this cache whenever the gameplay facts
/// change (including resource reload).
#[derive(Default)]
pub struct RandomTickSections {
    chunks: FxHashMap<(i32, i32), Cached>,
}
impl RandomTickSections {
    pub fn clear(&mut self) {
        self.chunks.clear();
    }
    pub fn remove(&mut self, key: &(i32, i32)) {
        self.chunks.remove(key);
    }
    /// The [`RandomTickSource`] for the chunk at `key`, for one chunk's turn.
    pub fn visit<'a>(
        &'a mut self,
        key: (i32, i32),
        facts: &'a BlockFacts,
        behaviours: &'a Behaviours,
    ) -> ChunkVisit<'a> {
        ChunkVisit {
            cached: self.chunks.entry(key).or_default(),
            key,
            revision: None,
            present: false,
            facts,
            behaviours,
        }
    }
}

/// A level's resident chunks and its block revision, which must advance with
/// every block write.
pub trait RandomTickChunks {
    fn block_revision(&self) -> u64;
    fn chunk_at(&self, key: (i32, i32)) -> Option<&Arc<Chunk>>;
}

/// One chunk's turn of random ticks. The sections are read again only after a
/// block changed, and a position that won't tick (nearly all of them) is
/// answered from the bitmap without touching the chunk.
pub struct ChunkVisit<'a> {
    cached: &'a mut Cached,
    key: (i32, i32),
    revision: Option<u64>,
    present: bool,
    facts: &'a BlockFacts,
    behaviours: &'a Behaviours,
}
impl ChunkVisit<'_> {
    #[inline]
    fn sync<L: RandomTickChunks + ?Sized>(&mut self, level: &L) {
        let revision = level.block_revision();
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            self.present = level.chunk_at(self.key).is_some_and(|chunk| {
                let (facts, behaviours) = (self.facts, self.behaviours);
                self.cached.refresh(
                    chunk,
                    |id| facts.get(BlockStateId(id)).random_ticks,
                    |id| behaviours.acts_on_random_tick(BlockStateId(id)),
                );
                true
            });
        }
    }
}
impl<L: RandomTickChunks + ?Sized> RandomTickSource<L> for ChunkVisit<'_> {
    fn next_section(&mut self, level: &mut L, from_y: i32) -> Option<i32> {
        self.sync(&*level);
        self.present
            .then(|| self.cached.sections.from(from_y))
            .flatten()
    }
    #[inline]
    fn ticking_state(&mut self, level: &mut L, pos: BlockPos) -> Option<BlockStateId> {
        self.sync(&*level);
        let positions = self.cached.positions[((pos.y - MIN_Y) >> 4) as usize].as_deref()?;
        let i = index(pos.x, pos.y, pos.z);
        if !self.present || positions[i / 64] >> (i % 64) & 1 == 0 {
            return None;
        }
        Some(
            level
                .chunk_at(self.key)?
                .block(pos.x & 15, pos.y, pos.z & 15),
        )
    }
}

#[cfg(test)]
impl RandomTickSections {
    /// The sections of `chunk` holding a state `random` accepts.
    fn get_with(
        &mut self,
        key: (i32, i32),
        chunk: &Arc<Chunk>,
        random: impl FnMut(u32) -> bool,
    ) -> Sections {
        let cached = self.chunks.entry(key).or_default();
        cached.refresh(chunk, random, |_| false);
        cached.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Section;

    #[test]
    fn edits_replacements_compaction_and_reload() {
        let mut cache = RandomTickSections::default();
        let mut chunk = Arc::new(Chunk::empty(0, 0));
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 0);
        cache.get_with((0, 0), &chunk, |_| panic!("unchanged chunk rescanned"));
        let worker = chunk.clone();
        Arc::make_mut(&mut chunk).set(0, MIN_Y, 0, 1);
        assert_eq!(
            cache
                .get_with((0, 0), &chunk, |id| id == 1)
                .collect::<Vec<_>>(),
            [MIN_Y]
        );
        assert_eq!(worker.get(0, MIN_Y, 0), 0);
        // Once the last ticking block is gone the section stops ticking, as
        // vanilla's count drops to zero, though its palette still lists it.
        Arc::make_mut(&mut chunk).set(0, MIN_Y, 0, 0);
        assert!(chunk.sections[0].blocks.maybe_has(|id| id == 1));
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 0);
        // Only the edited section is read again (its palette: air, then 1).
        Arc::make_mut(&mut chunk).set(0, MIN_Y + 16, 0, 1);
        let mut asked = Vec::new();
        let sections = cache.get_with((0, 0), &chunk, |id| {
            asked.push(id);
            id == 1
        });
        assert_eq!(
            (sections.collect::<Vec<_>>(), asked),
            (vec![MIN_Y + 16], vec![0, 1])
        );
        Arc::make_mut(&mut chunk).set(0, MIN_Y + 16, 0, 0);
        Arc::make_mut(&mut Arc::make_mut(&mut chunk).sections[1]).compact();
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 1).len(), 0);
        Arc::make_mut(&mut chunk).sections[SECTIONS - 1] = Arc::new(Section::default());
        cache.clear(); // resource reload, with changed facts for the same state IDs
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 0).len(), SECTIONS);
        cache.remove(&(0, 0));
        assert!(cache.chunks.is_empty());
    }

    #[test]
    fn replacement_at_same_coordinates_and_early_snapshot() {
        let mut cache = RandomTickSections::default();
        let mut chunk = Arc::new(Chunk::empty(-2, 3));
        let before_scheduled = cache.get_with((-2, 3), &chunk, |id| id == 1);
        Arc::make_mut(&mut chunk).set(0, MIN_Y + 16, 0, 1);
        assert_eq!(before_scheduled.len(), 0);
        assert_eq!(
            cache
                .get_with((-2, 3), &chunk, |id| id == 1)
                .collect::<Vec<_>>(),
            [MIN_Y + 16]
        );
        let weak = Arc::downgrade(&chunk);
        drop(chunk);
        assert!(weak.upgrade().is_none()); // cache does not retain unloaded block data
        let replacement = Arc::new(Chunk::empty(-2, 3));
        assert_eq!(cache.get_with((-2, 3), &replacement, |id| id == 1).len(), 0);
    }

    #[test]
    fn global_palette_and_ascending_order() {
        let mut chunk = Chunk::empty(0, 0);
        for i in [0, 9, SECTIONS - 1] {
            Arc::make_mut(&mut chunk.sections[i]).blocks =
                Section::from_blocks(&std::array::from_fn(|j| (j % 300) as u32));
        }
        let chunk = Arc::new(chunk);
        let mut cache = RandomTickSections::default();
        // Global palettes: the states the blocks hold, not the whole ID range.
        assert_eq!(cache.get_with((0, 0), &chunk, |id| id == 40000).len(), 0);
        cache.clear();
        assert_eq!(
            cache
                .get_with((0, 0), &chunk, |id| id == 299)
                .collect::<Vec<_>>(),
            [MIN_Y, MIN_Y + 9 * 16, MIN_Y + (SECTIONS as i32 - 1) * 16]
        );
        let sections = cache.get_with((0, 0), &chunk, |id| id == 299);
        let top = MIN_Y + (SECTIONS as i32 - 1) * 16;
        let from = |y| sections.from(y);
        assert_eq!(
            [
                from(MIN_Y - 16),
                from(MIN_Y),
                from(MIN_Y + 16),
                from(MIN_Y + 9 * 16)
            ],
            [
                Some(MIN_Y),
                Some(MIN_Y),
                Some(MIN_Y + 9 * 16),
                Some(MIN_Y + 9 * 16)
            ]
        );
        assert_eq!(
            [from(top), from(top + 16), from(top + 1000)],
            [Some(top), None, None]
        );
    }
}

#[cfg(test)]
mod tick_parity {
    use super::*;
    use horizon_core::{
        position::ChunkPos,
        random::{LegacyRandom, RandomSource},
    };
    use horizon_world::level::{LevelAccess, LevelEvent, Ticks, data};
    use std::{cell::RefCell, path::PathBuf};

    #[derive(Clone)]
    struct Level {
        chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
        random: LegacyRandom,
        reads: RefCell<Vec<BlockPos>>,
        events: Vec<LevelEvent>,
        time: i64,
        speed: i32,
        revision: u64,
    }
    impl RandomTickChunks for Level {
        fn block_revision(&self) -> u64 {
            self.revision
        }
        fn chunk_at(&self, key: (i32, i32)) -> Option<&Arc<Chunk>> {
            self.chunks.get(&key)
        }
    }
    /// The whole-section scan's draws: every position vanilla reads, with the
    /// state there, outside the reads the random ticks themselves make.
    struct Scan<'a, F> {
        next: F,
        draws: &'a mut Vec<(BlockPos, BlockStateId)>,
    }
    impl<F: FnMut(&mut Level, i32) -> Option<i32>> RandomTickSource<Level> for Scan<'_, F> {
        fn next_section(&mut self, level: &mut Level, from_y: i32) -> Option<i32> {
            (self.next)(level, from_y)
        }
        fn ticking_state(&mut self, level: &mut Level, pos: BlockPos) -> Option<BlockStateId> {
            let state = level
                .chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .map_or(BlockStateId(0), |c| c.block(pos.x, pos.y, pos.z));
            self.draws.push((pos, state));
            Some(state)
        }
    }
    /// The bitmap's draws, each checked against the blocks as they are then.
    struct Checked<'a> {
        visit: ChunkVisit<'a>,
        draws: &'a mut Vec<(BlockPos, BlockStateId)>,
        behaviours: &'a Behaviours,
    }
    impl RandomTickSource<Level> for Checked<'_> {
        fn next_section(&mut self, level: &mut Level, from_y: i32) -> Option<i32> {
            self.visit.next_section(level, from_y)
        }
        fn ticking_state(&mut self, level: &mut Level, pos: BlockPos) -> Option<BlockStateId> {
            let answer = self.visit.ticking_state(level, pos);
            let state = level.chunks[&(pos.x >> 4, pos.z >> 4)].block(pos.x, pos.y, pos.z);
            assert_eq!(
                answer,
                self.behaviours.acts_on_random_tick(state).then_some(state),
                "at {pos:?}"
            );
            if answer.is_some() {
                self.draws.push((pos, state));
            }
            answer
        }
    }
    impl LevelAccess for Level {
        fn block_state(&self, pos: BlockPos) -> BlockStateId {
            self.reads.borrow_mut().push(pos);
            self.chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .map_or(BlockStateId(0), |c| c.block(pos.x, pos.y, pos.z))
        }
        fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
            let chunk = self.chunks.get_mut(&(pos.x >> 4, pos.z >> 4))?;
            let previous = Arc::make_mut(chunk).set(pos.x, pos.y, pos.z, state.0);
            self.revision += u64::from(previous != state.0);
            Some(BlockStateId(previous))
        }
        fn is_loaded(&self, pos: BlockPos) -> bool {
            self.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
        }
        fn random(&mut self) -> &mut LegacyRandom {
            &mut self.random
        }
        fn game_time(&self) -> i64 {
            self.time
        }
        fn game_rules(&self) -> horizon_world::level::GameRules {
            horizon_world::level::GameRules {
                random_tick_speed: self.speed,
                ..Default::default()
            }
        }
        fn emit(&mut self, event: LevelEvent) {
            self.events.push(event);
        }
    }

    #[test]
    fn actual_random_ticks_match_a_block_scan() {
        let root = std::env::var_os("HORIZON_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let local = manifest
                    .join("../../.local/horizon-client")
                    .join(horizon_core::VANILLA_VERSION);
                if local.join("gameplay-facts.json").exists() {
                    local
                } else {
                    // The branch worktree lives below the main checkout.
                    manifest
                        .join("../../../../.local/horizon-client")
                        .join(horizon_core::VANILLA_VERSION)
                }
            });
        if !root.join("gameplay-facts.json").exists() {
            eprintln!(
                "skipping resource-backed random-tick parity: {}",
                root.display()
            );
            return;
        }
        let registry = data::load_registry(&root).unwrap();
        let (mut facts, behaviours) = BlockFacts::load_pair(&root, &registry).unwrap();
        let grass = registry.default_state("minecraft:grass_block").unwrap();
        let stone = registry.default_state("minecraft:stone").unwrap();
        let chunk = |x| {
            let mut c = Chunk::empty(x, 0);
            c.lit = true;
            // Every sampled block can run real behaviour and change to dirt.
            for i in [4, 7] {
                Arc::make_mut(&mut c.sections[i]).blocks =
                    crate::chunk::Section::from_blocks(&[grass.0; 4096]);
            }
            Arc::new(c)
        };
        let mut original = Level {
            chunks: [
                ((-1, 0), chunk(-1)),
                ((0, 0), chunk(0)),
                ((1, 0), Arc::new(Chunk::empty(1, 0))),
            ]
            .into_iter()
            .collect(),
            random: LegacyRandom::new(42),
            reads: RefCell::default(),
            events: Vec::new(),
            time: 0,
            speed: 3,
            revision: 0,
        };
        Arc::make_mut(original.chunks.get_mut(&(1, 0)).unwrap()).lit = true; // precipitation-only chunk
        let mut cached = original.clone();
        let mut cache = RandomTickSections::default();
        let (mut a, mut b) = (Ticks::new(12345), Ticks::new(12345));
        for x in -1..=1 {
            a.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
            b.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
        }
        for step in 0..128 {
            for level in [&mut original, &mut cached] {
                level.time = step;
                level.speed = [0, 1, 3, 9][step as usize % 4];
                level.reads.borrow_mut().clear();
                level.events.clear();
                match step {
                    12 => {
                        level.chunks.remove(&(-1, 0));
                    }
                    13 => {
                        level.chunks.insert((-1, 0), chunk(-1));
                    }
                    30 => {
                        Arc::make_mut(level.chunks.get_mut(&(0, 0)).unwrap()).sections[7] =
                            Arc::new(crate::chunk::Section::default());
                    }
                    40 | 41 => {
                        Arc::make_mut(level.chunks.get_mut(&(0, 0)).unwrap()).lit = step == 41;
                    }
                    70 => {
                        // Every block of a ticking section replaced: its
                        // palette still lists grass, but nothing ticks there.
                        let c = Arc::make_mut(level.chunks.get_mut(&(-1, 0)).unwrap());
                        for i in 0..4096 {
                            c.set(i & 15, MIN_Y + 4 * 16 + (i >> 8), i >> 4 & 15, stone.0);
                        }
                        assert!(c.sections[4].blocks.maybe_has(|id| id == grass.0));
                    }
                    _ => {}
                }
            }
            if step == 12 {
                cache.remove(&(-1, 0));
            }
            if step == 50 || step == 60 {
                facts.states[grass.0 as usize].random_ticks = step == 60;
                cache.clear();
            }
            // A write between ticks.
            if step == 20 {
                for level in [&mut original, &mut cached] {
                    level.set_raw(BlockPos::new(0, MIN_Y + 16, 0), grass);
                }
            }
            let lit = |level: &Level| {
                let mut keys: Vec<_> = level
                    .chunks
                    .iter()
                    .filter(|(_, c)| c.lit)
                    .map(|(&key, _)| key)
                    .collect();
                keys.sort();
                keys
            };
            let random = |id| facts.get(BlockStateId(id)).random_ticks;
            let (mut scanned, mut hits) = (Vec::new(), Vec::new());
            for (x, z) in lit(&original) {
                // Vanilla's rule, asked as each section is reached: any of its
                // 4,096 blocks ticks randomly.
                let reference = |level: &mut Level, from: i32| {
                    let chunk = &level.chunks[&(x, z)];
                    (from..MIN_Y + SECTIONS as i32 * 16)
                        .step_by(16)
                        .find(|&min_y| {
                            (0..16).any(|y| {
                                (0..16).any(|z| (0..16).any(|x| random(chunk.get(x, min_y + y, z))))
                            })
                        })
                };
                let mut source = Scan {
                    next: reference,
                    draws: &mut scanned,
                };
                behaviours.tick_chunk_random_from(
                    &mut original,
                    &mut a,
                    ChunkPos::new(x, z),
                    &mut source,
                );
            }
            for (x, z) in lit(&cached) {
                let mut source = Checked {
                    visit: cache.visit((x, z), &facts, &behaviours),
                    draws: &mut hits,
                    behaviours: &behaviours,
                };
                behaviours.tick_chunk_random_from(
                    &mut cached,
                    &mut b,
                    ChunkPos::new(x, z),
                    &mut source,
                );
            }
            scanned.retain(|&(_, state)| behaviours.acts_on_random_tick(state));
            assert_eq!(scanned, hits, "ticking draws at tick {step}");
            assert_eq!(original.reads, cached.reads, "ordered reads at tick {step}");
            assert_eq!(original.events, cached.events, "events at tick {step}");
            assert_eq!(
                a.random_tick_value, b.random_tick_value,
                "position RNG at tick {step}"
            );
            assert_eq!(
                original.random.next_long(),
                cached.random.next_long(),
                "level RNG at tick {step}"
            );
        }
        let dirt = registry.default_state("minecraft:dirt").unwrap();
        assert!(
            original
                .chunks
                .values()
                .any(|c| c.sections[4].blocks.maybe_has(|id| id == dirt.0))
        );
        for (key, c) in &original.chunks {
            for (s, t) in c.sections.iter().zip(&cached.chunks[key].sections) {
                assert_eq!(s.blocks, t.blocks);
            }
        }
    }
}
