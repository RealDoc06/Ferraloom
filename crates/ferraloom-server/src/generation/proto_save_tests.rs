//! Complete native proto save captures, including materialization boundaries.
use super::*;
use ferraloom_data::datapack::ResourceManager;
use ferraloom_nbt::{CompoundTag, Compression, NbtAccounter, NbtIo};
use ferraloom_worldgen::{
    generator::{ChunkGenerator, GenerationData},
    region::Heightmap,
};
use std::{io::Read, sync::OnceLock};

pub(super) fn noise_fixture() -> (Arc<WorldGenContext>, Arc<RegistryAdapter>) {
    static FIXTURE: OnceLock<(Arc<WorldGenContext>, Arc<RegistryAdapter>)> = OnceLock::new();
    let (context, registry) = FIXTURE.get_or_init(|| {
        let resources: ResourceManager = crate::generation_tests::resources();
        let preset = resources
            .require(
                &"minecraft:worldgen/world_preset/normal.json"
                    .parse()
                    .unwrap(),
            )
            .unwrap()
            .json()
            .unwrap();
        let data = Arc::new(GenerationData::load(&resources).unwrap());
        let generator = Arc::new(
            ChunkGenerator::decode(
                &preset["dimensions"]["minecraft:overworld"]["generator"],
                &resources,
                data,
                12345,
            )
            .unwrap(),
        );
        let mut context =
            crate::generation_tests::generation_context(&resources, generator, "overworld", 12345);
        context.generate_structures = false;
        context.possible_sets.clear();
        let registry =
            RegistryAdapter::new(&crate::registries::RegistrySet::load(&resources).unwrap())
                .unwrap();
        (Arc::new(context), Arc::new(registry))
    });
    (context.clone(), registry.clone())
}

pub(super) fn decompress(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut output)
        .unwrap();
    output
}
fn tag(bytes: &[u8]) -> CompoundTag {
    NbtIo::read_compound(bytes, Compression::None, &mut NbtAccounter::default()).unwrap()
}
pub(super) fn assert_exact(disk: &SerializableChunkData, expected: &[u8]) {
    let mut actual = Vec::new();
    NbtIo::write_compound(&mut actual, Compression::None, &disk.write().unwrap()).unwrap();
    let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(
        mismatch.is_none() && actual.len() == expected.len(),
        "native complete NBT differs at {mismatch:?}; actual length {}, expected {}",
        actual.len(),
        expected.len()
    );
}

#[test]
fn fresh_proto_defaults_and_light_presence_match_complete_native_nbt() {
    let (context, registry) = noise_fixture();
    let mut proto = ProtoChunk::empty(&context, ChunkPos::new(64, 64)).unwrap();
    assert_exact(
        &proto.capture(&context, registry.as_ref()).unwrap(),
        &decompress(include_bytes!(
            "../../tests/java/generation_schedule/fixtures/bare-empty.nbt.gz"
        )),
    );
    proto.light_correct = true;
    assert_exact(
        &proto.capture(&context, registry.as_ref()).unwrap(),
        &decompress(include_bytes!(
            "../../tests/java/generation_schedule/fixtures/bare-light-correct.nbt.gz"
        )),
    );
}

#[test]
fn generator_terrain_and_features_keep_complete_native_saved_maps() {
    let (context, registry) = noise_fixture();
    for fixture in [
        &include_bytes!("../../tests/java/generation_schedule/fixtures/generator-body.nbt.gz")[..],
        &include_bytes!("../../tests/java/generation_schedule/fixtures/terrain.nbt.gz")[..],
        &include_bytes!("../../tests/java/generation_schedule/fixtures/features.nbt.gz")[..],
    ] {
        let expected = decompress(fixture);
        let proto = ProtoChunk::load(
            &context,
            registry.as_ref(),
            ChunkPos::new(64, 64),
            SerializableChunkData::parse(tag(&expected)).unwrap(),
        )
        .unwrap();
        assert_exact(
            &proto.capture(&context, registry.as_ref()).unwrap(),
            &expected,
        );
    }
}

#[test]
fn features_update_final_heightmaps_and_keep_wg_frozen_like_native() {
    let (context, registry) = noise_fixture();
    let expected = decompress(include_bytes!(
        "../../tests/java/generation_schedule/fixtures/features.nbt.gz"
    ));
    let mut proto = ProtoChunk::load(
        &context,
        registry.as_ref(),
        ChunkPos::new(64, 64),
        SerializableChunkData::parse(tag(&expected)).unwrap(),
    )
    .unwrap();
    let before = proto
        .region
        .heightmap_snapshots()
        .filter(|(map, _)| matches!(map, Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg))
        .map(|(map, heights)| (map, *heights))
        .collect::<Vec<_>>();
    {
        let step = ferraloom_game::level::generation::ChunkPyramid::generation()
            .step_to(ChunkStatus::Features);
        let mut region = context
            .region(std::slice::from_mut(&mut proto.region), [64, 64], step)
            .unwrap();
        region
            .set_block(
                [1024, 319, 1024],
                ferraloom_data::Block::STONE.default_state(),
                16,
            )
            .unwrap();
    }
    let after = proto
        .region
        .heightmap_snapshots()
        .filter(|(map, _)| matches!(map, Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg))
        .map(|(map, heights)| (map, *heights))
        .collect::<Vec<_>>();
    assert_eq!(before, after);
    assert_exact(
        &proto.capture(&context, registry.as_ref()).unwrap(),
        &decompress(include_bytes!(
            "../../tests/java/generation_schedule/fixtures/features-edited.nbt.gz"
        )),
    );
}
