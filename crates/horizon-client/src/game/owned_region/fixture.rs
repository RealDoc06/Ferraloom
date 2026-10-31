//! Resource-backed checks of the concrete adapters, on a headless server Game.
use super::*;
use horizon_client::chunk::{Chunk, Section};
use horizon_core::random::{LegacyRandom, RandomSource};
use serde_json::{Value, json};
use std::{hint::black_box, time::Instant};

#[inline(never)]
fn block_reads<L: LevelAccess>(level: &mut L, positions: &[BlockPos], iterations: u64) -> u64 {
    let mut checksum = 0u64;
    for i in 0..iterations {
        let pos = black_box(positions[i as usize & (positions.len() - 1)]);
        checksum = checksum.wrapping_add(u64::from(black_box(&mut *level).block_state(pos).0));
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
            let mut level = OwnedLevel::new(
                region,
                frame,
                SerialServices {
                    game: RefCell::new(self),
                },
            );
            let started = Instant::now();
            let checksum = block_reads(&mut level, positions, iterations);
            let nanos = started.elapsed().as_nanos() as f64 / iterations as f64;
            let OwnedLevel {
                region, services, ..
            } = level;
            services
                .game
                .into_inner()
                .restore_block_region(*region.unwrap());
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
            Some("trace") => self.region_exact_trace(),
            Some("handoff") => self.region_handoff_checks(),
            _ => return Err("region fixture kind must be bench, trace or handoff".into()),
        };
        serde_json::to_writer_pretty(
            std::fs::File::create(input["output"].as_str().ok_or("missing output path")?)?,
            &output,
        )?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
struct BlockTrace {
    writes: Vec<((i32, i32, i32), u32, u32)>,
    events: Vec<LevelEvent>,
    random: (i64, u64),
    position_random: i32,
    queues: Vec<(
        (i32, i32),
        Vec<horizon_core::ticks::SavedTick>,
        Vec<horizon_core::ticks::SavedTick>,
    )>,
    comparators: Vec<((i32, i32, i32), i32)>,
    torches: Vec<(BlockPos, i64)>,
    block_events: Vec<horizon_world::level::BlockEventData>,
}
impl Game<'_> {
    fn region_trace_scene(&mut self, seed: i64) {
        self.fixture_reset(seed);
        self.spill = Default::default();
        self.pending = Default::default();
        self.next_entity = 0;
        self.server_level.block_ticks.random_tick_value = seed as i32;
        let state = |name| self.pack.assets.registry.default_state(name).unwrap();
        let (stone, grass, dirt, farmland, wheat, water, sapling) = (
            state("minecraft:stone"),
            state("minecraft:grass_block"),
            state("minecraft:dirt"),
            state("minecraft:farmland"),
            state("minecraft:wheat"),
            state("minecraft:water"),
            state("minecraft:oak_sapling"),
        );
        for column in self.world.chunks.values_mut() {
            Arc::make_mut(column).lit = true;
        }
        for x in -20i32..20 {
            for z in -20i32..20 {
                self.world.set(x, 63, z, stone).unwrap();
                let ground = if (x + z).rem_euclid(3) == 0 {
                    dirt
                } else {
                    grass
                };
                self.world.set(x, 64, z, ground).unwrap();
            }
        }
        for x in -3i32..=3 {
            for z in -3i32..=3 {
                if x == 0 && z == 0 {
                    self.world.set(x, 64, z, water).unwrap();
                } else {
                    self.world.set(x, 64, z, farmland).unwrap();
                    self.world.set(x, 65, z, wheat).unwrap();
                }
            }
        }
        for (x, z) in [(-12, -12), (-12, 12), (12, -12), (12, 12)] {
            self.world.set(x, 65, z, sapling).unwrap();
        }
        self.world.watched = vec![true; self.pack.assets.registry.len()];
        self.world.watched_writes.clear();
        self.ui.rules.set(
            "random_tick_speed",
            horizon_client::gamerules::Value::Int(64),
        );
    }
    fn region_trace_snapshot(&mut self, events: Vec<LevelEvent>) -> BlockTrace {
        let mut random = self.world.random.clone();
        let mut keys: Vec<_> = self.world.chunks.keys().copied().collect();
        keys.sort();
        let ticks = &self.server_level.block_ticks;
        let queues = keys
            .into_iter()
            .map(|(x, z)| {
                let pos = horizon_core::position::ChunkPos::new(x, z);
                let pack = |queue: &horizon_core::ticks::LevelTicks| {
                    queue
                        .chunk(pos)
                        .map_or_else(Vec::new, |chunk| chunk.pack(self.world.game_time))
                };
                ((x, z), pack(&ticks.blocks), pack(&ticks.fluids))
            })
            .collect();
        let mut comparators: Vec<_> = ticks
            .comparator_outputs
            .iter()
            .map(|(p, v)| ((p.x, p.y, p.z), *v))
            .collect();
        comparators.sort();
        BlockTrace {
            writes: std::mem::take(&mut self.world.watched_writes),
            events,
            random: (random.next_long(), random.next_gaussian().to_bits()),
            position_random: ticks.random_tick_value,
            queues,
            comparators,
            torches: ticks.torch_toggles.clone(),
            block_events: ticks.block_events.clone(),
        }
    }
    fn region_trace(&mut self, mode: Mode, seed: i64) -> Vec<BlockTrace> {
        self.options.regions = mode;
        self.region_trace_scene(seed);
        let mut trace = Vec::new();
        let mut unloaded = None;
        for step in 0..32 {
            // The caller supplies one dimension clock in both paths.
            self.world.game_time = step;
            self.world.day_time = if step < 16 { 6000 } else { 18000 };
            self.world.weather.rain_level = if step % 4 == 0 {
                (1.0, 1.0)
            } else {
                (0.0, 0.0)
            };
            match step {
                8 => {
                    let replacement = self.world.chunks[&(0, 0)].as_ref().clone();
                    self.world.chunks.insert((0, 0), Arc::new(replacement));
                }
                12 => {
                    unloaded = self.world.chunks.remove(&(-1, 0));
                }
                13 => {
                    self.world.chunks.insert((-1, 0), unloaded.take().unwrap());
                }
                _ => {}
            }
            self.sync_tick_chunks();
            self.tick_scheduled().unwrap();
            let ticking = self
                .world
                .chunks
                .keys()
                .copied()
                .filter(|&(x, z)| ticking_chunk(&self.world, x, z))
                .collect();
            self.tick_region_block_phase(ticking);
            let events = self.world.level_events.clone();
            self.process_level_events().unwrap();
            trace.push(self.region_trace_snapshot(events));
        }
        trace
    }
    fn region_exact_trace(&mut self) -> Value {
        self.install_live_features();
        let previous = self.options.regions;
        let mut writes = 0;
        for seed in [0, 1, 42, -1, 12345, i64::MAX, i64::MIN, 922337203] {
            let reference = self.region_trace(Mode::Off, seed);
            let owned = self.region_trace(Mode::Serial, seed);
            assert_eq!(reference, owned, "one-region live block phase, seed={seed}");
            writes += reference.iter().map(|s| s.writes.len()).sum::<usize>();
        }
        self.options.regions = previous;
        assert!(writes > 0, "the trace must exercise live block callbacks");
        json!({"seeds":8,"steps_per_seed":32,"ordered_writes":writes,"identical":true})
    }
}

struct CallbackServices<'g, 'r> {
    game: RefCell<&'g mut Game<'r>>,
    callback: fn(&mut Game<'_>) -> Reply,
}
impl Services for CallbackServices<'_, '_> {
    fn call(&self, region: Box<Region<World>>, _request: Request) -> Returned {
        let mut game = self.game.borrow_mut();
        let id = region.id;
        game.restore_block_region(*region);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.callback)(&mut game)));
        Returned {
            region: Box::new(game.take_block_region(id)),
            result,
        }
    }
}
fn replace_resident_column(game: &mut Game<'_>) -> Reply {
    let grass = game
        .pack
        .assets
        .registry
        .default_state("minecraft:grass_block")
        .unwrap();
    let mut column = game.world.chunks[&(0, 0)].as_ref().clone();
    column.set(1, 65, 1, grass.0);
    // This is the real same-key storage replacement path: neither the world's
    // write revision nor its loaded-key token changes. The loan invalidates it.
    game.world.chunks.insert((0, 0), Arc::new(column));
    game.server_level
        .block_ticks
        .comparator_outputs
        .insert(BlockPos::new(1, 65, 1), 99);
    game.server_level.block_ticks.next_sub_tick();
    Reply::Int(99)
}
fn reentrant_callback(game: &mut Game<'_>) -> Reply {
    let stone = game
        .pack
        .assets
        .registry
        .default_state("minecraft:stone")
        .unwrap();
    game.with_level_unflushed(|b, level, ticks| {
        // Neighbour/shape callbacks read the actual returned world and queues.
        b.set_block(
            level,
            ticks,
            BlockPos::new(1, 65, 1),
            stone,
            level_flags::UPDATE_ALL,
        );
    });
    game.server_level
        .block_ticks
        .comparator_outputs
        .insert(BlockPos::new(1, 65, 1), 99);
    game.server_level.block_ticks.next_sub_tick();
    game.world.random.next_float();
    Reply::Int(99)
}
fn unwinding_callback(game: &mut Game<'_>) -> Reply {
    reentrant_callback(game);
    std::panic::resume_unwind(Box::new("focused live callback unwind"))
}
impl Game<'_> {
    fn region_handoff_checks(&mut self) -> Value {
        use ferraloom_game::block_phase::BlockPhaseLevel;
        let pos = BlockPos::new(1, 65, 1);
        let chunk = horizon_core::position::ChunkPos::new(0, 0);
        for case in 0..3 {
            self.fixture_reset(42);
            let stone = self
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            let grass = self
                .pack
                .assets
                .registry
                .default_state("minecraft:grass_block")
                .unwrap();
            if case == 0 {
                self.world.set(1, 65, 1, stone).unwrap();
            }
            let frame = self.block_region_frame();
            let region = self.take_block_region(ferraloom_game::region::RegionId::REFERENCE);
            let callback: fn(&mut Game<'_>) -> Reply = match case {
                0 => replace_resident_column,
                1 => reentrant_callback,
                _ => unwinding_callback,
            };
            let mut level = OwnedLevel::new(
                region,
                frame,
                CallbackServices {
                    game: RefCell::new(self),
                    callback,
                },
            );
            assert!(level.services.game.borrow().world.as_ref().is_err());
            assert!(
                level
                    .services
                    .game
                    .borrow()
                    .server_level
                    .block_ticks
                    .as_ref()
                    .is_err()
            );
            let mut cache = horizon_world::level::RandomTickChunk::default();
            let before = level.block_revision();
            let old = cache
                .get_owned(before, || level.resident_chunk(chunk))
                .unwrap();
            assert_eq!(
                old.get(1, 65, 1),
                if case == 0 {
                    stone
                } else {
                    level.read(|r| r.world.air)
                }
            );
            let versions = level.read(|r| (r.world.revision, r.world.chunks.membership_revision()));
            level.random().next_gaussian();
            let mut expected = level.read(|r| r.world.random.clone());
            if case != 0 {
                expected.next_float();
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                level.exclusive(Request::Analog(pos, stone, Direction::Up))
            }));
            assert_eq!(result.is_err(), case == 2);
            if let Ok(reply) = result {
                assert!(matches!(reply, Reply::Int(99)));
            }
            assert_eq!(
                level.block_state(pos),
                if case == 0 { grass } else { stone }
            );
            assert_eq!(
                level.read_ticks(TickScope::Current, |t| t.comparator_outputs[&pos]),
                99
            );
            assert_eq!(
                level.write_ticks(TickScope::Current, |t| t.next_sub_tick()),
                1
            );
            assert_eq!(
                level.random().next_gaussian().to_bits(),
                expected.next_gaussian().to_bits()
            );
            assert_eq!(level.random().next_long(), expected.next_long());
            if case == 0 {
                assert_eq!(
                    versions,
                    level.read(|r| (r.world.revision, r.world.chunks.membership_revision()))
                );
                assert_ne!(before, level.block_revision());
                let current = cache
                    .get_owned(level.block_revision(), || level.resident_chunk(chunk))
                    .unwrap();
                assert_eq!(current.get(1, 65, 1), grass);
            }
            let OwnedLevel {
                region, services, ..
            } = level;
            services
                .game
                .into_inner()
                .restore_block_region(*region.unwrap());
        }
        json!({"same_key_replacement":true,"reentrant_callback":true,"unwind_keeps_live_owner":true})
    }
}
