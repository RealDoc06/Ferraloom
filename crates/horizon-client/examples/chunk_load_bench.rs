//! What a column costs to generate, on real terrain: generates and lights the
//! columns around a point with the vanilla generator, as a world being explored
//! does, so a change to generation shows as time per column.
//!
//!     cargo run --release -p horizon-client --example chunk_load_bench -- \
//!         .local/horizon-client/26.3 [--seed 1234] [--center CX CZ] [--radius 8]
//!         [--threads N]
//!
//! The default centre is the village of `tools/perf_bench.py`. The figure is
//! wall time with N workers (the game's own pool).
use horizon_client::{
    assets::Assets,
    biome::Biomes,
    blocks::BlockFacts,
    light::LightTables,
    mesher::MeshTables,
    models,
    streaming::{Streaming, WorkerContext},
    vanillagen::VanillaTerrain,
    world::World,
};
use horizon_world::registry::BlockStateId;
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

type Error = Box<dyn std::error::Error>;

fn flag<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(default)
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).ok_or("usage: chunk_load_bench <resources>")?);
    let seed: i64 = flag(&args, "--seed", 1234);
    let radius: i32 = flag(&args, "--radius", 8);
    let threads: usize = flag(
        &args,
        "--threads",
        thread::available_parallelism().map_or(2, |n| n.get()),
    );
    let center = args
        .iter()
        .position(|a| a == "--center")
        .map(|i| -> Result<(i32, i32), Error> {
            Ok((
                args.get(i + 1).ok_or("--center CX CZ")?.parse()?,
                args.get(i + 2).ok_or("--center CX CZ")?.parse()?,
            ))
        })
        .transpose()?
        .unwrap_or((-60, 10));

    let assets = Arc::new(Assets::load(&root)?);
    let render = Arc::new(assets.reload(&root)?);
    let facts = BlockFacts::load(&root, assets.registry)?;
    let visibility = models::load_visibility(&root, assets.registry)?;
    let biomes = Arc::new(Biomes::load(&root)?);
    let context = Arc::new(WorkerContext {
        tables: Arc::new(MeshTables::new(&assets, &render, &facts, visibility)?),
        light: Arc::new(LightTables::load(&root, assets.registry, &facts)?),
        biomes: biomes.clone(),
        assets: assets.clone(),
        render,
    });
    let terrain = VanillaTerrain::for_world(
        &root.join("worldgen"),
        assets.registry,
        &biomes,
        seed,
        "minecraft:overworld",
        true,
    )?;

    // Generation and light, on the game's worker pool.
    let mut streaming = Streaming::new(Arc::new(terrain), context, threads);
    streaming.center = center;
    streaming.radius = radius;
    let mut world = World::new(BlockStateId(0), 0);
    let started = Instant::now();
    let mut quiet = 0;
    while quiet < 100 {
        let before = world.chunks.len();
        let _ = streaming.update(&mut world);
        thread::sleep(Duration::from_millis(2));
        quiet = if streaming.pending() == 0 && before == world.chunks.len() {
            quiet + 1
        } else {
            0
        };
    }
    let generated = started.elapsed() - Duration::from_millis(2 * 100);
    let columns = world.chunks.len();
    println!(
        "generated and lit {columns} columns in {:.0} ms with {threads} workers ({:.2} ms per column)",
        generated.as_secs_f64() * 1e3,
        generated.as_secs_f64() * 1e3 / columns.max(1) as f64
    );
    Ok(())
}
