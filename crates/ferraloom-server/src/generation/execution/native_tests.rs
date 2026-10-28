//! The same legal native FEATURES order, with one and several Rust workers.
use super::*;
use crate::generation::proto_save_tests::{assert_exact, decompress, noise_fixture};
use ferraloom_nbt::{Compression, NbtAccounter, NbtIo};
use ferraloom_storage::RegionFileVersion;
use ferraloom_worldgen::region::Heightmap;
use std::{
    io::{Cursor, Read},
    sync::mpsc,
    time::Duration,
};

fn int(reader: &mut impl Read) -> i32 {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes).unwrap();
    i32::from_be_bytes(bytes)
}
fn compare_native(
    state: &GenerationState,
    registry: &RegistryAdapter,
    bytes: &[u8],
) -> Vec<Vec<u8>> {
    let mut input = Cursor::new(bytes);
    let mut snapshots = Vec::new();
    for x in 62..=66 {
        for z in 62..=66 {
            let pos = ChunkPos::new(x, z);
            let chunk = &state.holders[&pos.pack()].chunk;
            for y in -64..320 {
                for dz in 0..16 {
                    for dx in 0..16 {
                        assert_eq!(
                            i32::from(chunk.region.chunk.block(dx, y, dz).id()),
                            int(&mut input),
                            "native block at {pos:?} {dx},{y},{dz}"
                        );
                    }
                }
            }
            let maps = chunk.region.heightmap_snapshots().collect::<Vec<_>>();
            for dz in 0..16 {
                for dx in 0..16 {
                    for kind in [Heightmap::WorldSurfaceWg, Heightmap::OceanFloorWg] {
                        let heights = maps.iter().find(|(map, _)| *map == kind).unwrap().1;
                        assert_eq!(
                            // ChunkAccess#getHeight returns first-available Y minus one.
                            heights[dz * 16 + dx] - 1,
                            int(&mut input),
                            "native {kind:?} at {pos:?}"
                        );
                    }
                }
            }
            let start = input.position() as usize;
            NbtIo::read_compound(&mut input, Compression::None, &mut NbtAccounter::default())
                .unwrap();
            let end = input.position() as usize;
            let expected = &bytes[start..end];
            let disk = chunk.capture(&state.context, registry).unwrap();
            assert_exact(&disk, expected);
            let mut actual = Vec::new();
            NbtIo::write_compound(&mut actual, Compression::None, &disk.write().unwrap()).unwrap();
            snapshots.push(actual);
        }
    }
    assert_eq!(input.position() as usize, bytes.len());
    snapshots
}
fn advance(
    state: GenerationState,
    registry: &Arc<RegistryAdapter>,
    io: &Arc<IoWorker>,
    pool: &Arc<JobPool>,
    id: u64,
    pos: ChunkPos,
    status: ChunkStatus,
) -> GenerationState {
    let (reply, _) = oneshot::channel();
    let request = super::super::GenerationRequest {
        id,
        pos,
        status,
        reply,
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let retained = (51..=77)
        .flat_map(|x| (51..=77).map(move |z| ChunkPos::new(x, z).pack()))
        .collect();
    let pending = dispatch_generation(
        state,
        request,
        retained,
        registry.clone(),
        io.clone(),
        pool.clone(),
    )
    .unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(pending.blocking_recv());
    });
    let completion = receive
        .recv_timeout(Duration::from_secs(120))
        .expect("owned generation timed out")
        .unwrap();
    completion.cleanup.unwrap();
    completion.result.unwrap();
    completion.state
}
fn max_overlap(state: &GenerationState, status: ChunkStatus) -> usize {
    let mut events = state
        .center_intervals
        .iter()
        .filter(|(kind, _, _)| *kind == status)
        .flat_map(|(_, start, end)| [(*start, 1i32), (*end, -1)])
        .collect::<Vec<_>>();
    events.sort();
    let mut active = 0;
    let mut maximum = 0;
    for (_, delta) in events {
        active += delta;
        maximum = maximum.max(active);
    }
    maximum as usize
}

#[test]
fn serial_and_parallel_centers_match_complete_native_nbt_in_request_feature_order() {
    let (context, registry) = noise_fixture();
    let terrain = decompress(include_bytes!(
        "../../../tests/java/generation_schedule/fixtures/terrain.bin.gz"
    ));
    let features = decompress(include_bytes!(
        "../../../tests/java/generation_schedule/fixtures/features-request-order.bin.gz"
    ));
    let other_order = decompress(include_bytes!(
        "../../../tests/java/generation_schedule/fixtures/features-permuted-order.bin.gz"
    ));
    assert_ne!(
        features, other_order,
        "native legal FEATURES orders are observably different"
    );
    let directory = tempfile::tempdir().unwrap();
    let mut reference = None;
    for width in [1, 2, 3] {
        let mut io = Arc::new(
            IoWorker::new(
                directory.path().join(width.to_string()),
                RegionFileVersion::Deflate,
                false,
            )
            .unwrap(),
        );
        let pool = Arc::new(JobPool::new("center-parity", width).unwrap());
        let mut state = GenerationState::new(
            context.clone(),
            Arc::new(JobPool::new("center-parity-light", 1).unwrap()),
        );
        let mut request = 0;
        if width == 1 {
            for x in 62..=66 {
                for z in 62..=66 {
                    state = advance(
                        state,
                        &registry,
                        &io,
                        &pool,
                        request,
                        ChunkPos::new(x, z),
                        ChunkStatus::Terrain,
                    );
                    request += 1;
                }
            }
            compare_native(&state, &registry, &terrain);
            // The FEATURES replay starts fresh on every worker count, with the
            // same request trace; each first FEATURES request has a multi-center
            // TERRAIN layer rather than one isolated TERRAIN computation.
            state = GenerationState::new(
                context.clone(),
                Arc::new(JobPool::new("center-parity-light", 1).unwrap()),
            );
            request = 0;
        }
        for x in 63..=65 {
            for z in 63..=65 {
                state = advance(
                    state,
                    &registry,
                    &io,
                    &pool,
                    request,
                    ChunkPos::new(x, z),
                    ChunkStatus::Features,
                );
                request += 1;
            }
        }
        let features_bytes = compare_native(&state, &registry, &features);
        if width > 1 {
            assert!(
                max_overlap(&state, ChunkStatus::Biomes) >= 2,
                "BIOMES bodies did not overlap"
            );
            assert!(
                max_overlap(&state, ChunkStatus::Terrain) >= 2,
                "TERRAIN bodies did not overlap"
            );
        }
        if let Some(expected_features) = &reference {
            assert_eq!(&features_bytes, expected_features);
        } else {
            reference = Some(features_bytes);
        }
        Arc::get_mut(&mut io).unwrap().shutdown().wait().unwrap();
    }
}
