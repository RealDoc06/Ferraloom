use super::*;
use crate::generation_tests::{generation_context, resources};
use ferraloom_data::{Block, BlockState};
use ferraloom_game::level::generation::ChunkPyramid;
use ferraloom_util::BlockPos;
use ferraloom_worldgen::generator::{ChunkGenerator, GenerationData};

fn fixture() -> (GenerationState, RegistryAdapter) {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let generator = Arc::new(
        ChunkGenerator::decode(
            &serde_json::json!({
                "type": "minecraft:flat",
                "settings": {
                    "layers": [{"height": 1, "block": "minecraft:bedrock"}],
                    "biome": "minecraft:plains",
                    "features": false,
                    "lakes": false,
                    "structure_overrides": []
                }
            }),
            &resources,
            data,
            0,
        )
        .unwrap(),
    );
    let mut context = generation_context(&resources, generator, "overworld", 0);
    // A controlled build height keeps the propagation fixture small while
    // retaining the actual registries, block behavior and executor paths.
    context.dimension.min_y = 0;
    context.dimension.height = 32;
    let registry =
        RegistryAdapter::new(&crate::registries::RegistrySet::load(&resources).unwrap()).unwrap();
    (
        GenerationState::new(
            Arc::new(context),
            Arc::new(JobPool::new("proto-relight-test", 1).unwrap()),
        ),
        registry,
    )
}

fn light_air_chunks(state: &mut GenerationState) {
    for x in 0..=2 {
        let pos = ChunkPos::new(x, 0);
        let mut proto = ProtoChunk::empty(&state.context, pos).unwrap();
        proto.region.status = ChunkStatus::Features;
        let mut holder = GenerationChunkHolder::new(proto, ChunkStatus::Features);
        holder.complete(ChunkStatus::Features);
        state.holders.insert(pos.pack(), holder);
        for status in [ChunkStatus::InitializeLight, ChunkStatus::Light] {
            state.apply_light(pos, status).unwrap();
            state.holders.get_mut(&pos.pack()).unwrap().complete(status);
        }
    }
    state.flush_light().unwrap();
    state.light_changed.clear();
}

fn write(state: &mut GenerationState, pos: BlockPos, block: BlockState) {
    let chunk_pos = ChunkPos::containing(pos);
    let mut holder = state.holders.remove(&chunk_pos.pack()).unwrap();
    {
        let mut region = state
            .context
            .region(
                std::slice::from_mut(&mut holder.chunk.region),
                [chunk_pos.x, chunk_pos.z],
                ChunkPyramid::generation().step_to(ChunkStatus::Features),
            )
            .unwrap();
        region.set_light_engine(&mut state.light);
        assert!(region.set_block([pos.x, pos.y, pos.z], block, 16).unwrap());
    }
    // The executor must see the modified canonical region, including its
    // synchronous sky-source update, before draining the queued checkBlock.
    state.holders.insert(chunk_pos.pack(), holder);
}

fn block_light(disk: &SerializableChunkData, pos: BlockPos) -> u8 {
    let Some(layer) = disk
        .sections
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|section| section.y == Some((pos.y >> 4) as i8))
        .and_then(|section| section.block_light.as_ref())
    else {
        return 0;
    };
    let bytes = &layer.0;
    let index = ((pos.y & 15) * 256 + (pos.z & 15) * 16 + (pos.x & 15)) as usize;
    ((bytes[index >> 1] as u8) >> ((index & 1) * 4)) & 15
}

#[test]
fn partial_write_error_flushes_propagated_neighbors_before_transport_and_save() {
    let (mut state, registry) = fixture();
    light_air_chunks(&mut state);
    let source = BlockPos::new(31, 8, 8);
    let probe = BlockPos::new(32, 8, 8);
    write(&mut state, source, Block::GLOWSTONE.default_state());
    assert!(state.light.has_light_work());
    let failure = state
        .finish_light::<()>(Err(ferraloom_worldgen::Error(
            "partial feature error".into(),
        )
        .into()))
        .unwrap_err();
    assert_eq!(
        failure
            .downcast_ref::<ferraloom_worldgen::Error>()
            .unwrap()
            .0,
        "partial feature error"
    );
    assert!(!state.light.has_light_work());
    assert_eq!(state.light.light_value(LightLayer::Block, probe), 14);
    let changed_neighbor = ChunkPos::containing(probe);
    assert!(state.light_changed.contains(&changed_neighbor.pack()));

    let directory = tempfile::tempdir().unwrap();
    let mut io =
        IoWorker::new(directory.path(), storage::RegionFileVersion::Deflate, false).unwrap();
    // This center is two chunks away from the propagation-only neighbor, so
    // the old radius-one light transport would lose its updated layer.
    let output = state
        .generate(
            0,
            ChunkPos::new(0, 0),
            ChunkStatus::Empty,
            &registry,
            &io,
            None,
        )
        .unwrap();
    let neighbor = output
        .terrain
        .light_neighbors
        .iter()
        .find(|neighbor| neighbor.pos == changed_neighbor)
        .unwrap();
    assert_eq!(neighbor.revision, output.terrain.revision);
    assert_eq!(block_light(&neighbor.disk, probe), 14);
    let saved = &output
        .neighbors
        .iter()
        .find(|(pos, _)| *pos == changed_neighbor)
        .unwrap()
        .1;
    assert_eq!(block_light(saved, probe), 14);

    write(&mut state, source, Block::AIR.default_state());
    assert!(state.light.has_light_work());
    state.save_proto_chunks(&registry, &io).unwrap();
    assert!(!state.light.has_light_work());
    let saved =
        SerializableChunkData::parse(io.load(changed_neighbor).wait().unwrap().unwrap()).unwrap();
    assert_eq!(block_light(&saved, probe), 0);
    assert!(state.light_changed.contains(&changed_neighbor.pack()));
    io.close().unwrap();
}

#[test]
fn executor_error_retains_engine_and_cancellation_flushes_before_proto_eviction() {
    let (mut state, registry) = fixture();
    light_air_chunks(&mut state);
    let source = BlockPos::new(31, 8, 8);
    let probe = BlockPos::new(32, 8, 8);
    write(&mut state, source, Block::GLOWSTONE.default_state());
    let failure = state
        .run_light_work(|_, _| anyhow::bail!("executor operation failed"))
        .unwrap_err();
    assert_eq!(failure.to_string(), "executor operation failed");
    assert!(state.light.has_light_work());

    let directory = tempfile::tempdir().unwrap();
    let mut io =
        IoWorker::new(directory.path(), storage::RegionFileVersion::Deflate, false).unwrap();
    let cancelled = AtomicBool::new(true);
    let failure = state
        .generate(
            0,
            ChunkPos::new(0, 0),
            ChunkStatus::Full,
            &registry,
            &io,
            Some(&cancelled),
        )
        .err()
        .unwrap();
    assert!(failure.to_string().contains("Cancelled"));
    assert!(!state.light.has_light_work());
    assert_eq!(state.light.light_value(LightLayer::Block, probe), 14);
    state
        .save_unclaimed_protos(&BTreeSet::new(), &registry, &io)
        .unwrap();
    assert!(state.holders.is_empty());
    let saved = SerializableChunkData::parse(
        io.load(ChunkPos::containing(probe))
            .wait()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(block_light(&saved, probe), 14);
    io.close().unwrap();
}

#[test]
fn loaded_lit_proto_fills_canonical_sources_at_initialize_light() {
    let (mut state, registry) = fixture();
    let pos = ChunkPos::new(0, 0);
    let mut proto = ProtoChunk::empty(&state.context, pos).unwrap();
    proto
        .region
        .chunk
        .set_block(7, 15, 7, Block::STONE.default_state());
    proto.region.status = ChunkStatus::Light;
    proto.light_correct = true;
    let disk = proto.capture(&state.context, &registry).unwrap();
    let loaded = ProtoChunk::load(&state.context, &registry, pos, disk).unwrap();
    assert_eq!(
        loaded.region.sky_light_sources().lowest_source_y(7, 7),
        i32::MIN
    );
    state.holders.insert(
        pos.pack(),
        GenerationChunkHolder::new(loaded, ChunkStatus::Light),
    );
    state
        .apply_light(pos, ChunkStatus::InitializeLight)
        .unwrap();
    assert_eq!(
        state.holders[&pos.pack()]
            .chunk
            .region
            .sky_light_sources()
            .lowest_source_y(7, 7),
        16
    );
    assert_eq!(
        ferraloom_light::LightWorld::sky_light_sources(&state.snapshots, pos)
            .unwrap()
            .lowest_source_y(7, 7),
        16
    );
}
