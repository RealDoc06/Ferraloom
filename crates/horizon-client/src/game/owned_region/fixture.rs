//! Resource-backed checks of the concrete adapters, on a headless server Game.
use super::*;
use horizon_client::chunk::{Chunk, Section};
use horizon_core::random::{LegacyRandom, RandomSource};
use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

#[inline(never)]
fn block_reads<L: LevelAccess>(level: &L, positions: &[BlockPos], iterations: u64) -> u64 {
    let mut checksum = 0u64;
    for i in 0..iterations {
        let pos = black_box(positions[i as usize & (positions.len() - 1)]);
        checksum = checksum.wrapping_add(u64::from(black_box(level).block_state(pos).0));
    }
    black_box(checksum)
}

impl Game<'_> {
    fn region_bench_scene(&mut self) -> Vec<BlockPos> {
        self.fixture_reset(42);
        self.world.chunks.clear();
        let states = ["minecraft:stone", "minecraft:dirt", "minecraft:grass_block"]
            .map(|name| self.pack.assets.registry.default_state(name).unwrap().0);
        for x in -4..4 {
            for z in -4..4 {
                let mut chunk = Chunk::empty(x, z);
                for sy in [4, 7, 11, 15] {
                    let blocks =
                        std::array::from_fn::<_, 4096, _>(|i| states[(i + sy) % states.len()]);
                    Arc::make_mut(&mut chunk.sections[sy]).blocks = Section::from_blocks(&blocks);
                }
                self.world.chunks.insert((x, z), Arc::new(chunk));
            }
        }
        let mut random = LegacyRandom::new(42);
        (0..8192)
            .map(|_| {
                BlockPos::new(
                    random.next_int_bound(144).unwrap() - 72,
                    random.next_int_bound(HEIGHT + 16).unwrap() + MIN_Y - 8,
                    random.next_int_bound(144).unwrap() - 72,
                )
            })
            .collect()
    }
    fn region_read_measure(
        &mut self,
        owned: bool,
        positions: &[BlockPos],
        iterations: u64,
    ) -> (f64, u64) {
        // Each branch calls the same generic work on its actual concrete
        // adapter. Frame construction and ownership moves are outside timing.
        if !owned {
            self.with_level_unflushed(|_, level, _| {
                let started = Instant::now();
                let checksum = block_reads(level, positions, iterations);
                (
                    started.elapsed().as_nanos() as f64 / iterations as f64,
                    checksum,
                )
            })
        } else {
            let frame = self.block_region_frame();
            let region = self.take_block_region(ferraloom_game::region::RegionId::REFERENCE);
            let level = OwnedLevel::new(
                region,
                frame,
                SerialServices {
                    game: RefCell::new(self),
                },
            );
            let started = Instant::now();
            let checksum = block_reads(&level, positions, iterations);
            let nanos = started.elapsed().as_nanos() as f64 / iterations as f64;
            let OwnedLevel {
                region, services, ..
            } = level;
            services
                .game
                .into_inner()
                .restore_block_region(region.into_inner().unwrap());
            (nanos, checksum)
        }
    }
    fn region_read_bench(&mut self, iterations: u64) -> Value {
        assert!(iterations > 0);
        let positions = self.region_bench_scene();
        let expected = self.region_read_measure(false, &positions, iterations).1;
        for owned in [true, false, true] {
            assert_eq!(
                self.region_read_measure(owned, &positions, iterations).1,
                expected
            );
        }
        let mut rows = Vec::new();
        for pair in 0..12 {
            let (first, second) = if pair % 2 == 0 {
                (false, true)
            } else {
                (true, false)
            };
            let a = self.region_read_measure(first, &positions, iterations);
            let b = self.region_read_measure(second, &positions, iterations);
            assert_eq!((a.1, b.1), (expected, expected));
            let (before, after) = if first { (b.0, a.0) } else { (a.0, b.0) };
            rows.push(json!({"pair":pair,"owned_first":first,"before_ns":before,"after_ns":after,"iterations":iterations,"checksum":expected}));
        }
        json!({"case":"block-read","pairs":rows})
    }
    pub(in crate::game) fn region_fixture(&mut self, spec: &str) -> Result<()> {
        assert!(!self.options.execution_role.is_client());
        let input: Value = serde_json::from_reader(std::fs::File::open(spec)?)?;
        let output = match input["kind"].as_str() {
            Some("bench") => {
                self.region_read_bench(input["iterations"].as_u64().unwrap_or(5_000_000))
            }
            _ => return Err("region fixture kind must be bench".into()),
        };
        serde_json::to_writer_pretty(
            std::fs::File::create(input["output"].as_str().ok_or("missing output path")?)?,
            &output,
        )?;
        Ok(())
    }
}
