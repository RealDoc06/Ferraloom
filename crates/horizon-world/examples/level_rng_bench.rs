//! Paired timings of real level-RNG consumers, without rendering or network IO.
use horizon_core::{
    geometry::Direction,
    position::{BlockPos, ChunkPos},
    random::LegacyRandom,
};
use horizon_world::{
    level::{Behaviours, LevelAccess, LevelEvent, Ticks, data},
    registry::BlockStateId,
};
use std::{hint::black_box, path::Path, time::Instant};

struct Level {
    air: BlockStateId,
    ground: BlockStateId,
    crop: BlockStateId,
    random: LegacyRandom,
    events: u64,
    writes: u64,
}
const CROP: BlockPos = BlockPos::new(0, 65, 0);

impl LevelAccess for Level {
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        if pos == CROP {
            self.crop
        } else if pos.y == 64 {
            self.ground
        } else {
            self.air
        }
    }
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        assert_eq!(pos, CROP, "only the bonemeal crop changes in this scene");
        self.writes += 1;
        Some(std::mem::replace(&mut self.crop, state))
    }
    fn is_loaded(&self, _pos: BlockPos) -> bool {
        true
    }
    #[inline]
    fn with_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
        draw(&mut self.random)
    }
    fn game_time(&self) -> i64 {
        6000
    }
    fn emit(&mut self, _event: LevelEvent) {
        self.events = self.events.wrapping_add(1);
    }
}

fn run(
    case: &str,
    behaviours: &Behaviours,
    level: &mut Level,
    ticks: &mut Ticks,
    initial_crop: BlockStateId,
    iterations: usize,
) {
    match case {
        "random-chunk" => {
            for _ in 0..iterations {
                behaviours.tick_chunk_random(
                    black_box(&mut *level),
                    ticks,
                    black_box(ChunkPos::new(0, 0)),
                    |_, from| (from <= 64).then_some(64),
                );
            }
        }
        "crop-bonemeal" => {
            for _ in 0..iterations {
                level.crop = initial_crop;
                assert!(behaviours.use_bone_meal(
                    black_box(&mut *level),
                    ticks,
                    black_box(CROP),
                    Direction::Up,
                ));
            }
        }
        _ => panic!("expected random-chunk or crop-bonemeal"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("prepared resources path");
    let case = args.next().expect("random-chunk or crop-bonemeal");
    let iterations: usize = args.next().unwrap_or_else(|| "2000000".into()).parse()?;
    let registry = data::load_registry(Path::new(&root))?;
    let behaviours = Behaviours::load(&registry, Path::new(&root))?;
    let air = registry.default_state("minecraft:air").unwrap();
    let initial_crop = if case == "random-chunk" {
        air
    } else {
        registry.default_state("minecraft:wheat").unwrap()
    };
    let ground = registry
        .default_state(if case == "random-chunk" {
            "minecraft:grass_block"
        } else {
            "minecraft:farmland"
        })
        .unwrap();
    let new_level = || Level {
        air,
        ground,
        crop: initial_crop,
        random: LegacyRandom::new(42),
        events: 0,
        writes: 0,
    };
    let new_ticks = || {
        let mut ticks = Ticks::new(42);
        ticks.add_empty_chunk(ChunkPos::new(0, 0)).unwrap();
        ticks
    };
    run(
        &case,
        &behaviours,
        &mut new_level(),
        &mut new_ticks(),
        initial_crop,
        10000,
    );
    let mut level = new_level();
    let mut ticks = new_ticks();
    let started = Instant::now();
    run(
        &case,
        &behaviours,
        &mut level,
        &mut ticks,
        initial_crop,
        iterations,
    );
    let elapsed = started.elapsed();
    println!(
        "{}",
        serde_json::json!({
            "case": case,
            "iterations": iterations,
            "ns_per_operation": elapsed.as_nanos() as f64 / iterations as f64,
            "rng_state": black_box(level.random.state()),
            "position_lcg": ticks.random_tick_value,
            "crop": level.crop.0,
            "events": level.events,
            "writes": level.writes,
        }),
    );
    Ok(())
}
