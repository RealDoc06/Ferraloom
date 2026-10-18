use super::*;
use crate::structure::processor::tests::generator;
use serde_json::Value;
use std::io::Read;
fn golden(bytes: &[u8]) -> Value {
    let mut s = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}
#[test]
fn jigsaw_structure_compounds_round_trip_through_storage_chunk_model() {
    let (resources, generator) = generator();
    let registry = StructureRegistry::load(&resources, &generator.data).unwrap();
    let pools = crate::structure::pool::StructurePools::load(&resources, &mut |_| {
        Err(Error("no inline feature".into()))
    })
    .unwrap();
    let templates = StructureTemplateManager::load(&resources, pools.templates()).unwrap();
    let reference_cases = golden(include_bytes!("testdata/reference_sets.json.gz"));
    let refs = reference_cases.as_array().unwrap().last().unwrap();
    let input_refs = refs["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect::<Vec<_>>();
    let expected_refs = refs["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect::<Vec<_>>();
    for case in golden(include_bytes!("testdata/jigsaw.json.gz"))
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.get("start_nbt").is_some())
    {
        let id = case["structure"].as_str().unwrap();
        let start = ferraloom_nbt::parse_compound(case["start_nbt"].as_str().unwrap()).unwrap();
        let starts: CompoundTag = [(id, Tag::Compound(start))].into_iter().collect();
        let references: CompoundTag = [
            (id, Tag::LongArray(input_refs.clone())),
            ("unknown:discarded", Tag::LongArray(input_refs.clone())),
            ("minecraft:stronghold", Tag::LongArray(Vec::new())),
        ]
        .into_iter()
        .collect();
        let data: CompoundTag = [
            ("starts", Tag::Compound(starts)),
            ("References", Tag::Compound(references)),
        ]
        .into_iter()
        .collect();
        let chunk = ChunkStructures::from_nbt(
            [0, 0],
            &data,
            &registry,
            &templates,
            &resources,
            &mut |_| Err(Error("no inline feature".into())),
        )
        .unwrap();
        let packed = chunk.to_nbt().unwrap();
        let mut expected =
            ferraloom_nbt::parse_compound(case["loaded_nbt"].as_str().unwrap()).unwrap();
        expected.insert("ChunkX", Tag::Int(0));
        expected.insert("ChunkZ", Tag::Int(0));
        assert_eq!(
            packed
                .get("starts")
                .unwrap()
                .as_compound()
                .unwrap()
                .get(id)
                .unwrap(),
            &Tag::Compound(expected)
        );
        assert_eq!(
            packed
                .get("References")
                .unwrap()
                .as_compound()
                .unwrap()
                .get(id)
                .unwrap(),
            &Tag::LongArray(expected_refs.clone())
        );
        assert_eq!(
            packed
                .get("References")
                .unwrap()
                .as_compound()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(chunk.known_valid_starts()[&id.parse().unwrap()], 3);
        let storage = ferraloom_storage::schema::StructureData::from_nbt(packed.clone()).unwrap();
        assert_eq!(storage.to_nbt(), packed);
        let root: CompoundTag = [
            ("DataVersion", Tag::Int(5023)),
            (
                "Status",
                Tag::String("minecraft:structure_references".into()),
            ),
            ("xPos", Tag::Int(0)),
            ("zPos", Tag::Int(0)),
            ("structures", Tag::Compound(packed)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            ferraloom_storage::schema::SerializableChunkData::parse(root.clone())
                .unwrap()
                .write()
                .unwrap(),
            root
        );
    }
}

fn probe_start(id: Identifier, seed: i64, chunk: [i32; 2], references: i32) -> StructureStart {
    use crate::structure::{
        BoundingBox, jigsaw::PoolElementStructurePiece, pool::StructurePoolElement,
        start::StructurePiece, template::LiquidSettings,
    };
    use ferraloom_util::{
        Rotation,
        random::{LegacyRandomSource, RandomSource, WorldgenRandom},
    };
    let index = i32::from(id.path().as_bytes()[0] - b'a');
    let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
    random.set_large_feature_seed(seed, chunk[0], chunk[1]);
    if random.next_int_bound(5) == index {
        return StructureStart::invalid();
    }
    let position = [
        chunk[0] * 16 + random.next_int_bound(16),
        12 + random.next_int_bound(17),
        chunk[1] * 16 + random.next_int_bound(16),
    ];
    let width = 8 + random.next_int_bound(40);
    let depth = 8 + random.next_int_bound(40);
    let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
    StructureStart::new(
        id,
        chunk,
        references,
        [
            TerrainAdjustment::None,
            TerrainAdjustment::Bury,
            TerrainAdjustment::BeardThin,
            TerrainAdjustment::BeardBox,
            TerrainAdjustment::Encapsulate,
        ][index as usize],
        vec![StructurePiece::Jigsaw(PoolElementStructurePiece {
            element: Arc::new(StructurePoolElement::Empty),
            position,
            ground_level_delta: index - 1,
            rotation,
            bounds: BoundingBox::new(
                position,
                [
                    position[0] + width - 1,
                    position[1] + 7,
                    position[2] + depth - 1,
                ],
            ),
            liquid: if index % 2 == 0 {
                LiquidSettings::ApplyWaterlogging
            } else {
                LiquidSettings::IgnoreWaterlogging
            },
            gen_depth: 0,
            orientation: None,
            junctions: Vec::new(),
        })],
    )
}
#[test]
fn structure_start_and_reference_statuses_match_vanilla_weighted_retries_and_bounds() {
    use super::super::{
        BoundingBox, jigsaw::PoolElementStructurePiece, placement::StructureSet,
        pool::StructurePoolElement, start::StructurePiece, template::LiquidSettings,
    };
    use ferraloom_util::Rotation;
    let cases = golden(include_bytes!("testdata/structure_statuses.json.gz"));
    let registry = StructureRegistry {
        structures: (0..5)
            .map(|index| {
                let id = format!("test:{}", char::from(b'a' + index));
                (
                    id.parse().unwrap(),
                    StructureDefinition {
                        kind: worldgen_structure_type::Id::from_name("minecraft:jigsaw").unwrap(),
                        biomes: vec![
                            if index == 4 {
                                "minecraft:ocean"
                            } else {
                                "minecraft:plains"
                            }
                            .parse()
                            .unwrap(),
                        ],
                        step: GenerationStep::SurfaceStructures,
                        terrain: [
                            TerrainAdjustment::None,
                            TerrainAdjustment::Bury,
                            TerrainAdjustment::BeardThin,
                            TerrainAdjustment::BeardBox,
                            TerrainAdjustment::Encapsulate,
                        ][index as usize],
                    },
                )
            })
            .collect(),
    };
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let mut state = ChunkGeneratorStructureState {
            seed,
            origin: [-2, 3],
            sets: BTreeMap::new(),
            rings: Vec::new(),
        };
        for set in case["sets"].as_array().unwrap() {
            state.sets.insert(
                set["id"].as_str().unwrap().parse().unwrap(),
                StructureSet::decode(set).unwrap(),
            );
        }
        let possible = registry
            .possible_structure_sets(&state, &["minecraft:plains".parse().unwrap()])
            .unwrap();
        assert_eq!(
            possible.iter().map(ToString::to_string).collect::<Vec<_>>(),
            case["possible_sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        let mut chunks = BTreeMap::new();
        for expected in case["chunks"].as_array().unwrap() {
            let p = [
                expected["position"][0].as_i64().unwrap() as i32,
                expected["position"][1].as_i64().unwrap() as i32,
            ];
            let mut chunk = ChunkStructures::new(p);
            if (p[0] - p[1]).rem_euclid(7) == 0 {
                let position = [p[0] * 16 + 1, 20, p[1] * 16 + 2];
                chunk.starts.insert(
                    "test:c".parse().unwrap(),
                    StructureStart::new(
                        "test:c".parse().unwrap(),
                        p,
                        2,
                        TerrainAdjustment::BeardThin,
                        vec![StructurePiece::Jigsaw(PoolElementStructurePiece {
                            element: Arc::new(StructurePoolElement::Empty),
                            position,
                            ground_level_delta: 1,
                            rotation: Rotation::None,
                            bounds: BoundingBox::new(
                                position,
                                [position[0] + 5, 27, position[2] + 5],
                            ),
                            liquid: LiquidSettings::ApplyWaterlogging,
                            gen_depth: 0,
                            orientation: None,
                            junctions: Vec::new(),
                        })],
                    ),
                );
            } else if (p[0] + p[1]).rem_euclid(5) == 0 {
                chunk.starts.insert(
                    "test:a".parse().unwrap(),
                    StructureStart::new(
                        "test:a".parse().unwrap(),
                        p,
                        9,
                        TerrainAdjustment::None,
                        Vec::new(),
                    ),
                );
            }
            let mut calls = Vec::new();
            create_structures(&state, &possible, &mut chunk, &mut |id, refs| {
                calls.push(id.to_string());
                Ok(probe_start(id.clone(), seed, p, refs))
            })
            .unwrap();
            assert_eq!(
                calls,
                expected["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "seed={seed} chunk={p:?}"
            );
            assert_eq!(
                chunk.to_nbt().unwrap().get("starts").unwrap(),
                &Tag::Compound(
                    ferraloom_nbt::parse_compound(expected["starts"].as_str().unwrap()).unwrap()
                ),
                "seed={seed} chunk={p:?}"
            );
            chunks.insert(p, chunk);
        }
        for expected in case["references"].as_array().unwrap() {
            let p = [
                expected["position"][0].as_i64().unwrap() as i32,
                expected["position"][1].as_i64().unwrap() as i32,
            ];
            let mut refs = std::mem::take(&mut chunks.get_mut(&p).unwrap().references);
            for _ in 0..2 {
                create_references(p, &mut refs, &mut |p| {
                    chunks
                        .get(&p)
                        .map(|c| &c.starts)
                        .ok_or_else(|| Error("missing structure dependency".into()))
                })
                .unwrap();
            }
            chunks.get_mut(&p).unwrap().references = refs;
            assert_eq!(
                chunks[&p].to_nbt().unwrap().get("References").unwrap(),
                &Tag::Compound(
                    ferraloom_nbt::parse_compound(expected["references"].as_str().unwrap())
                        .unwrap()
                ),
                "seed={seed} chunk={p:?}"
            );
        }
    }
}
