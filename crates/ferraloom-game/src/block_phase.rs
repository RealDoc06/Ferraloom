//! Live block phases shared by the serial caller and the regional driver.
use crate::{
    chunk::ChunkBlocks,
    random_tick_sections::{SectionOwner, Sections},
};
use horizon_core::position::ChunkPos;
use horizon_world::level::{
    Behaviours, LevelAccess, RandomTickChunk, Ticks,
    tick_access::{BorrowedLevelTicks, TickContext},
};
use std::sync::Arc;

/// Resident storage and section eligibility used by the block phase kernel.
/// All answers concern the live world, including changes made by earlier
/// callbacks in the same phase. The caller retains its dimension's tick budget.
pub trait BlockPhaseLevel: LevelAccess {
    type Chunk: SectionOwner;

    /// Invalidate cached reads after a storage change or ownership transfer.
    fn block_revision(&self) -> u64;
    /// A fast path for a locally resident column. A miss is resolved through
    /// `LevelAccess::block_state`; it must never be interpreted as unloaded air.
    fn resident_chunk(&self, chunk: ChunkPos) -> Option<Arc<Self::Chunk>>;
    /// Resolve ownership before answering; a local miss must not skip a section
    /// that the live loaded owner will tick.
    fn random_tick_sections(&mut self, chunk: ChunkPos) -> Sections;
}

/// The random-block portion of `ServerChunkCache#tickChunks`, retaining the
/// caller's captured chunk order and live section checks. Precipitation draws
/// and block/fluid callbacks keep their original interleaving. Calls go straight
/// to the live adapter; the caller retains its existing effect-flush boundaries.
/// This adds no phase boundary, independent clock or effect replay.
pub fn tick_random_chunks<L: BlockPhaseLevel + ?Sized>(
    behaviours: &Behaviours,
    level: &mut L,
    ticks: &mut Ticks,
    chunks: impl IntoIterator<Item = ChunkPos>,
) {
    tick_random_chunks_with_context(behaviours, BorrowedLevelTicks::new(level, ticks), chunks);
}

/// The live context form releases tick-state borrows before each block callback.
/// The compatibility entry point above and the owned-region driver share this
/// body, so changing ownership does not select a different block algorithm.
pub fn tick_random_chunks_with_context<C: TickContext>(
    behaviours: &Behaviours,
    mut level: C,
    chunks: impl IntoIterator<Item = ChunkPos>,
) where
    C::Level: BlockPhaseLevel,
{
    for chunk in chunks {
        let mut read = None;
        let mut sections = Sections::default();
        let mut current_chunk = RandomTickChunk::default();
        behaviours.tick_chunk_random_with_context(
            level.reborrow(),
            chunk,
            |level, from| {
                if read != Some(level.block_revision()) {
                    read = Some(level.block_revision());
                    sections = level.random_tick_sections(chunk);
                }
                sections.from(from)
            },
            |level, at| {
                let current =
                    current_chunk.get_owned(level.block_revision(), || level.resident_chunk(chunk));
                current.map_or_else(
                    || level.block_state(at),
                    |column| ChunkBlocks::new(column.sections()).block(at.x & 15, at.y, at.z & 15),
                )
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chunk::{MIN_Y, SECTIONS, Section, index},
        fast_hash::FxHashMap,
        random_tick_sections::RandomTickSections,
    };
    use horizon_core::{
        position::BlockPos,
        random::{LegacyRandom, RandomSource},
    };
    use horizon_world::{
        level::{GameRules, LevelEvent, data},
        registry::BlockStateId,
    };
    use std::path::PathBuf;

    #[derive(Clone)]
    struct Column(Vec<Arc<Section>>);
    impl SectionOwner for Column {
        fn sections(&self) -> &[Arc<Section>] {
            &self.0
        }
    }
    struct Level<'a> {
        chunks: FxHashMap<(i32, i32), Arc<Column>>,
        sections: RandomTickSections<Column>,
        behaviours: &'a Behaviours,
        random: LegacyRandom,
        revision: u64,
        time: i64,
        speed: i32,
        fast_path: bool,
        writes: Vec<(BlockPos, BlockStateId, BlockStateId)>,
        events: Vec<LevelEvent>,
    }
    impl LevelAccess for Level<'_> {
        fn block_state(&self, pos: BlockPos) -> BlockStateId {
            self.chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .map_or(BlockStateId(0), |c| {
                    ChunkBlocks::new(c.sections()).block(pos.x, pos.y, pos.z)
                })
        }
        fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
            let section = (pos.y - MIN_Y) >> 4;
            if !(0..SECTIONS as i32).contains(&section) {
                return None;
            }
            let chunk = Arc::make_mut(self.chunks.get_mut(&(pos.x >> 4, pos.z >> 4))?);
            let old = BlockStateId(
                Arc::make_mut(&mut chunk.0[section as usize])
                    .set(index(pos.x, pos.y, pos.z), state.0),
            );
            if old != state {
                self.revision += 1;
                self.writes.push((pos, old, state));
            }
            Some(old)
        }
        fn is_loaded(&self, pos: BlockPos) -> bool {
            self.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
        }
        #[inline]
        fn with_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
            draw(&mut self.random)
        }
        fn game_time(&self) -> i64 {
            self.time
        }
        fn game_rules(&self) -> GameRules {
            GameRules {
                random_tick_speed: self.speed,
                ..Default::default()
            }
        }
        fn emit(&mut self, event: LevelEvent) {
            self.events.push(event);
        }
    }
    impl BlockPhaseLevel for Level<'_> {
        type Chunk = Column;
        fn block_revision(&self) -> u64 {
            self.revision
        }
        fn resident_chunk(&self, chunk: ChunkPos) -> Option<Arc<Column>> {
            self.fast_path
                .then(|| self.chunks.get(&(chunk.x, chunk.z)).cloned())
                .flatten()
        }
        fn random_tick_sections(&mut self, chunk: ChunkPos) -> Sections {
            self.chunks
                .get(&(chunk.x, chunk.z))
                .map_or_else(Sections::default, |column| {
                    self.sections.get((chunk.x, chunk.z), column, |id| {
                        self.behaviours.is_randomly_ticking(BlockStateId(id))
                    })
                })
        }
    }

    #[test]
    fn live_random_phase_matches_full_section_scan_and_rng() {
        let root = std::env::var_os("HORIZON_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../.local/horizon-client")
                    .join(horizon_core::VANILLA_VERSION)
            });
        if !root.join("gameplay-facts.json").exists() {
            eprintln!(
                "skipping resource-backed block kernel parity: {}",
                root.display()
            );
            return;
        }
        let registry = data::load_registry(&root).unwrap();
        let behaviours = Behaviours::load(&registry, &root).unwrap();
        let grass = registry.default_state("minecraft:grass_block").unwrap();
        let column = |ticking: bool| {
            let mut sections: Vec<_> = (0..SECTIONS)
                .map(|_| Arc::new(Section::default()))
                .collect();
            if ticking {
                for s in [4, 7] {
                    Arc::make_mut(&mut sections[s]).blocks = Section::from_blocks(&[grass.0; 4096]);
                }
            }
            Arc::new(Column(sections))
        };
        let world = || Level {
            chunks: [
                ((0, 0), column(true)),
                ((-1, 0), column(true)),
                ((1, 0), column(false)),
            ]
            .into_iter()
            .collect(),
            sections: RandomTickSections::default(),
            behaviours: &behaviours,
            random: LegacyRandom::new(42),
            revision: 0,
            time: 0,
            speed: 3,
            fast_path: true,
            writes: Vec::new(),
            events: Vec::new(),
        };
        let (mut reference, mut kernel) = (world(), world());
        let (mut reference_ticks, mut kernel_ticks) = (Ticks::new(12345), Ticks::new(12345));
        for x in [-1, 0, 1] {
            reference_ticks
                .add_empty_chunk(ChunkPos::new(x, 0))
                .unwrap();
            kernel_ticks.add_empty_chunk(ChunkPos::new(x, 0)).unwrap();
        }
        for step in 0..32 {
            for level in [&mut reference, &mut kernel] {
                level.time = step;
                level.speed = [0, 1, 3, 9][step as usize % 4];
                match step {
                    8 => {
                        Arc::make_mut(level.chunks.get_mut(&(0, 0)).unwrap()).0[7] =
                            Arc::new(Section::default());
                        level.revision += 1;
                    }
                    12 => {
                        level.set_raw(BlockPos::new(0, MIN_Y + 16, 0), grass);
                    }
                    16 => {
                        let column = level.chunks[&(0, 0)].as_ref().clone();
                        level.chunks.insert((0, 0), Arc::new(column));
                        level.revision += 1;
                    }
                    20 => {
                        level.chunks.remove(&(-1, 0));
                        level.sections.remove(&(-1, 0));
                        level.revision += 1;
                    }
                    21 => {
                        level.chunks.insert((-1, 0), column(true));
                        level.revision += 1;
                    }
                    _ => {}
                }
                level.writes.clear();
                level.events.clear();
            }
            // A chunk fast-path miss still reads the live world.
            kernel.fast_path = step != 6;
            let chunks: Vec<_> = [0, -1, 1]
                .into_iter()
                .filter(|x| reference.chunks.contains_key(&(*x, 0)))
                .map(|x| ChunkPos::new(x, 0))
                .collect();
            for &chunk in &chunks {
                behaviours.tick_chunk_random(
                    &mut reference,
                    &mut reference_ticks,
                    chunk,
                    |level, from| {
                        (from..level.max_y()).step_by(16).find(|&y| {
                            (0..4096).any(|i| {
                                let pos = BlockPos::new(
                                    chunk.x * 16 + (i & 15),
                                    y + (i >> 8),
                                    chunk.z * 16 + ((i >> 4) & 15),
                                );
                                behaviours.is_randomly_ticking(level.block_state(pos))
                            })
                        })
                    },
                );
            }
            tick_random_chunks(&behaviours, &mut kernel, &mut kernel_ticks, chunks);
            assert_eq!(
                reference.writes, kernel.writes,
                "ordered writes at tick {step}"
            );
            assert_eq!(
                reference.events, kernel.events,
                "ordered effects at tick {step}"
            );
            assert_eq!(
                reference_ticks.random_tick_value, kernel_ticks.random_tick_value,
                "position RNG at tick {step}"
            );
            assert_eq!(
                reference.random.next_long(),
                kernel.random.next_long(),
                "world RNG at tick {step}"
            );
        }
        assert!(
            !kernel.writes.is_empty(),
            "real block callbacks must have run"
        );
    }
}
