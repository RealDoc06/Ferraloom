use super::*;
use crate::{
    generator::{ChunkGenerator, GenerationData},
    settings::DimensionType,
};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_light::{LevelLightEngine, LightLayer, SectionSnapshots};
use ferraloom_util::{BlockPos, ChunkPos, SectionPos};
use serde_json::Value;
use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

struct Behavior;
impl WorldgenBlockBehavior for Behavior {
    fn post_process_pos(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        position: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Ok(state
            .post_process_y_offset()
            .map(|offset| [position[0], position[1] + offset, position[2]]))
    }
}
static BEHAVIOR: Behavior = Behavior;

fn fixtures() -> &'static Value {
    static FIXTURES: LazyLock<Value> =
        LazyLock::new(|| serde_json::from_str(include_str!("proto-light.json")).unwrap());
    &FIXTURES
}
fn generated(position: [i32; 2]) -> GeneratedChunk {
    static GENERATOR: LazyLock<(ChunkGenerator, DimensionType)> = LazyLock::new(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla resources");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        let data = Arc::new(GenerationData::load(&resources).unwrap());
        let generator = ChunkGenerator::decode(
            &serde_json::json!({"type":"minecraft:debug"}),
            &resources,
            data,
            0,
        )
        .unwrap();
        let mut dimension = DimensionType::decode(
            &resources
                .require(&"minecraft:dimension_type/overworld.json".parse().unwrap())
                .unwrap()
                .json()
                .unwrap(),
        )
        .unwrap();
        dimension.min_y = -16;
        dimension.height = 48;
        (generator, dimension)
    });
    GENERATOR
        .0
        .build_terrain(position, GENERATOR.1, None)
        .unwrap()
}
fn point(value: &Value) -> [i32; 3] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as i32)
}
fn position(pos: BlockPos) -> String {
    format!("{},{},{}", pos.x, pos.y, pos.z)
}
#[derive(Default)]
struct Calls(Vec<String>);
impl ProtoChunkLightEngine for Calls {
    fn update_section_status(&mut self, pos: SectionPos, empty: bool) {
        self.0
            .push(format!("section:{},{},{}:{empty}", pos.x, pos.y, pos.z));
    }
    fn check_block(&mut self, pos: BlockPos) {
        self.0.push(format!("check:{}", position(pos)));
    }
}
fn view<'a>(chunks: &'a mut [RegionChunk], position: [i32; 2]) -> WorldGenRegion<'a> {
    WorldGenRegion::new(
        chunks,
        position,
        &[ChunkStatus::Light],
        0,
        None,
        &BEHAVIOR,
        0,
        63,
    )
    .unwrap()
}

#[test]
fn lit_proto_writes_match_native_status_section_source_and_heightmap_order() {
    let mut count = 0;
    for case in fixtures()["cases"].as_array().unwrap() {
        let coordinates = point(&case["chunk"]);
        let chunk_pos = [coordinates[0], coordinates[2]];
        let status = match case["status"].as_str().unwrap() {
            "minecraft:features" => ChunkStatus::Features,
            "minecraft:initialize_light" => ChunkStatus::InitializeLight,
            "minecraft:light" => ChunkStatus::Light,
            other => panic!("status {other}"),
        };
        let mut generated = generated(chunk_pos);
        generated.set_block(15, -1, 0, Block::STONE.default_state());
        let mut chunks = [RegionChunk::new(generated, status)];
        chunks[0].initialize_light_sources();
        {
            let region = view(&mut chunks, chunk_pos);
            for map in Heightmap::ALL {
                assert_eq!(
                    region
                        .height_at(map, chunk_pos[0] * 16 + 15, chunk_pos[1] * 16)
                        .unwrap(),
                    0
                );
            }
        }
        for op in case["writes"].as_array().unwrap() {
            let p = point(&op["pos"]);
            let state = BlockState::from_id(op["state"].as_u64().unwrap() as u16).unwrap();
            let before = chunks[0]
                .sky_light_sources()
                .lowest_source_y((p[0] & 15) as u8, (p[2] & 15) as u8);
            let mut calls = Calls::default();
            {
                let mut region = view(&mut chunks, chunk_pos);
                region.set_light_engine(&mut calls);
                if op["bulk"].as_bool().unwrap() {
                    region.set_block_in_section(p, state).unwrap();
                } else {
                    assert!(region.set_block(p, state, 16).unwrap());
                }
                let chunk = region.chunk(chunk_pos, ChunkStatus::Empty).unwrap();
                assert_eq!(
                    u64::from(
                        chunk
                            .chunk
                            .block((p[0] & 15) as u8, p[1], (p[2] & 15) as u8)
                            .id()
                    ),
                    op["stored"].as_u64().unwrap()
                );
                let sources = chunk.sky_light_sources();
                assert_eq!(
                    sources.lowest_source_y((p[0] & 15) as u8, (p[2] & 15) as u8),
                    op["sky"].as_i64().unwrap() as i32,
                    "{case:?} {op:?}"
                );
                assert_eq!(
                    sources.highest_lowest_source_y(),
                    op["highest"].as_i64().unwrap() as i32
                );
                let heights = Heightmap::ALL.map(|map| region.height_at(map, p[0], p[2]).unwrap());
                assert_eq!(heights, point_heights(&op["heights"]), "{op:?}");
            }
            let expected = op["calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| {
                    let (call, sky) = value.as_str().unwrap().rsplit_once(':').unwrap();
                    // Native observes the old buffer at updateSectionStatus, and
                    // the synchronously updated buffer at checkBlock.
                    assert_eq!(
                        sky.parse::<i32>().unwrap(),
                        if call.starts_with("section:") {
                            before
                        } else {
                            op["sky"].as_i64().unwrap() as i32
                        }
                    );
                    call.to_owned()
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.0, expected, "{op:?}");
            count += 1;
        }
    }
    assert_eq!(count, 306);
}
fn point_heights(value: &Value) -> [i32; 6] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as i32)
}
fn snapshot(chunk: &RegionChunk) -> SectionSnapshots<'static> {
    let mut snapshots = SectionSnapshots::new(
        chunk.chunk.min_y >> 4,
        (chunk.chunk.min_y + chunk.chunk.height) >> 4,
    );
    snapshots.insert_chunk_with_sky_light_sources(
        ChunkPos::new(chunk.chunk.position[0], chunk.chunk.position[1]),
        chunk
            .chunk
            .sections
            .iter()
            .cloned()
            .map(|section| Some(Cow::Owned(section)))
            .collect(),
        Some(chunk.sky_light_sources().clone()),
    );
    snapshots
}

#[test]
fn raw_bulk_then_checked_write_keeps_canonical_sources_when_light_is_drained() {
    let mut generated = generated([0, 0]);
    for x in 0..16 {
        for z in 0..16 {
            generated.set_block(x, -1, z, Block::STONE.default_state());
        }
    }
    let mut chunks = [RegionChunk::new(generated, ChunkStatus::InitializeLight)];
    chunks[0].initialize_light_sources();
    let mut engine = LevelLightEngine::new(true, true);
    engine.initialize_light(&snapshot(&chunks[0]), ChunkPos::ZERO, false);
    let checked = &fixtures()["stale_source"]["checked"];
    {
        let mut region = view(&mut chunks, [0, 0]);
        region.set_light_engine(&mut engine);
        for x in 0..16 {
            for z in 0..16 {
                region
                    .set_block_in_section([x, 15, z], Block::STONE.default_state())
                    .unwrap();
            }
        }
        assert!(
            region
                .set_block(
                    point(&checked["pos"]),
                    BlockState::from_id(checked["state"].as_u64().unwrap() as u16).unwrap(),
                    16
                )
                .unwrap()
        );
        let sources = region
            .chunk([0, 0], ChunkStatus::Empty)
            .unwrap()
            .sky_light_sources();
        assert_eq!(
            sources.lowest_source_y(8, 8),
            checked["sky"].as_i64().unwrap() as i32
        );
        assert_eq!(sources.highest_lowest_source_y(), 0);
        // A required later read can fail after a successful write. Its queued
        // light checks and owned mutation must remain available to the caller.
        assert!(region.block([16, 1, 8]).is_err());
    }
    assert!(engine.has_light_work());
    engine.light_chunk(&snapshot(&chunks[0]), ChunkPos::ZERO, false);
    for sample in fixtures()["stale_source"]["light"].as_array().unwrap() {
        let p = point(sample);
        let p = BlockPos::new(p[0], p[1], p[2]);
        assert_eq!(
            engine.light_value(LightLayer::Sky, p),
            sample[3].as_u64().unwrap() as u8,
            "{sample:?}"
        );
        assert_eq!(
            engine.light_value(LightLayer::Block, p),
            sample[4].as_u64().unwrap() as u8,
            "{sample:?}"
        );
    }
    assert_eq!(
        chunks[0].chunk.block(8, 15, 8),
        Block::STONE.default_state()
    );
}

#[test]
fn full_snapshot_refresh_preserves_canonical_sources_and_pending_payload() {
    let mut original = generated([0, 0]);
    original.set_block(8, -1, 8, Block::STONE.default_state());
    original.post_processing.push([8, -1, 8]);
    let mut chunks = [RegionChunk::new(original, ChunkStatus::Full)];
    chunks[0].initialize_light_sources();
    let sources = chunks[0].sky_light_sources().clone();
    assert_eq!(sources.lowest_source_y(8, 8), 0);
    let payload: CompoundTag = [("marker", Tag::Int(1))].into_iter().collect();
    chunks[0].block_entities.insert([8, 0, 8], payload.clone());
    chunks[0].entities.push(payload.clone());
    chunks[0].block_ticks.schedule(SavedTick {
        position: [8, -1, 8],
        kind: Block::STONE,
        priority: 1,
    });
    chunks[0].fluid_ticks.schedule(SavedTick {
        position: [8, 0, 8],
        kind: ferraloom_data::registries::fluid::Id::WATER,
        priority: 2,
    });
    chunks[0].poi_changes.push((
        [8, 0, 8],
        Block::AIR.default_state(),
        Block::STONE.default_state(),
    ));
    chunks[0].structures.references.insert(
        "minecraft:mineshaft".parse().unwrap(),
        crate::structure::references::StructureReferences::from_array(&[
            ChunkPos::new(1, 0).to_long()
        ]),
    );
    let structures = chunks[0].structures.to_nbt().unwrap();
    {
        let region = view(&mut chunks, [0, 0]);
        for map in Heightmap::ALL {
            assert_eq!(region.height_at(map, 8, 8).unwrap(), 0);
        }
    }
    let mut updated = chunks[0].chunk.clone();
    updated.set_block(8, 15, 8, Block::STONE.default_state());
    updated.post_processing.clear();
    chunks[0].refresh_full_snapshot(updated);
    assert_eq!(chunks[0].sky_light_sources(), &sources);
    assert_eq!(chunks[0].chunk.post_processing, [[8, -1, 8]]);
    assert_eq!(chunks[0].status, ChunkStatus::Full);
    assert_eq!(chunks[0].block_entities[&[8, 0, 8]], payload);
    assert_eq!(chunks[0].entities, [payload]);
    assert_eq!(chunks[0].block_ticks.ticks()[0].priority, 1);
    assert_eq!(chunks[0].fluid_ticks.ticks()[0].priority, 2);
    assert_eq!(chunks[0].poi_changes.len(), 1);
    assert_eq!(chunks[0].structures.to_nbt().unwrap(), structures);
    for map in Heightmap::ALL {
        if matches!(map, Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg) {
            assert_eq!(
                chunks[0].heightmaps[map as usize].get().unwrap()[8 * 16 + 8],
                16
            );
        } else {
            assert!(chunks[0].heightmaps[map as usize].get().is_none());
        }
    }
    let region = view(&mut chunks, [0, 0]);
    for map in Heightmap::ALL {
        assert_eq!(region.height_at(map, 8, 8).unwrap(), 16);
    }
}
