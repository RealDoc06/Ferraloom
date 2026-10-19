//! Tree fixtures run through the real game bridge, preserving native assertions.
use crate::block::GameWorldgenBlockBehavior;
use ferraloom_data::{Block, BlockState, properties};
use ferraloom_util::random::{
    LegacyRandomSource, Random, RandomSource, WorldgenRandom, XoroshiroRandomSource,
};
use ferraloom_worldgen::{
    feature::{FeatureContext, FeatureLoader, FeatureRandom, FeatureRandomSelector, WorldGenLevel},
    region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};

struct OriginalTreeBehavior;
impl WorldgenBlockBehavior for OriginalTreeBehavior {
    fn has_block_entity(&self, state: BlockState) -> bool {
        state.has_block_entity()
    }
    fn can_survive(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        _pos: [i32; 3],
    ) -> Result<bool, ferraloom_worldgen::Error> {
        Err(ferraloom_worldgen::Error(format!(
            "Unsupported: can_survive ({})",
            state.block().name()
        )))
    }
    fn post_process_pos(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _pos: [i32; 3],
    ) -> Result<Option<[i32; 3]>, ferraloom_worldgen::Error> {
        Err(ferraloom_worldgen::Error(
            "Unsupported: post_process_pos".into(),
        ))
    }
}
fn initial(p: [i32; 3], origin: [i32; 3], terrain: &str, ground: BlockState) -> BlockState {
    if !(-64..320).contains(&p[1]) {
        return Block::VOID_AIR.default_state();
    }
    let [x, y, z] = std::array::from_fn::<_, 3, _>(|i| p[i] - origin[i]);
    let slope = if terrain == "slope" {
        x.div_euclid(3)
    } else {
        0
    };
    if y < slope {
        return if y == slope - 1 {
            ground
        } else {
            Block::STONE.default_state()
        };
    }
    if terrain == "cave" && (4..=7).contains(&y) {
        return if y == 7 {
            ground
        } else {
            Block::STONE.default_state()
        };
    }
    if terrain == "existing_leaves" {
        if [x, y, z] == [2, 4, 0] {
            return Block::OAK_LOG.default_state();
        }
        if (3..=6).contains(&y) && (1..=3).contains(&x) && (-2..=2).contains(&z) {
            return Block::OAK_LEAVES
                .default_state()
                .with(properties::DISTANCE, 2)
                .unwrap();
        }
    }
    if terrain == "water" && y < 3 {
        return Block::WATER.default_state();
    }
    if terrain == "blocked" && (1..=30).contains(&y) && x.abs() <= 12 && z.abs() <= 12
        || terrain == "partial"
            && (3..=8).contains(&y)
            && (1..=3).contains(&x)
            && (-2..=1).contains(&z)
    {
        return Block::STONE.default_state();
    }
    Block::AIR.default_state()
}
fn placed(chunks: &[RegionChunk], origin: [i32; 3], terrain: &str, ground: BlockState) -> Value {
    let mut actual = std::collections::BTreeMap::new();
    for chunk in chunks {
        for &(p, _, _) in &chunk.poi_changes {
            let s = chunk
                .chunk
                .block((p[0] & 15) as u8, p[1], (p[2] & 15) as u8);
            if s != initial(p, origin, terrain, ground) {
                actual.insert(p, s.id());
            } else {
                actual.remove(&p);
            }
        }
    }
    serde_json::to_value(
        actual
            .into_iter()
            .map(|(p, s)| serde_json::json!([p[0], p[1], p[2], s]))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

#[test]
fn tree_fixtures_use_real_leaf_and_plant_bridge() {
    let (resources, generator) = super::worldgen_tests::setup();
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../ferraloom-worldgen/src/feature/tree/cases.json.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    let mut loader = FeatureLoader::new(&resources).unwrap();
    let mut features = BTreeMap::new();
    for case in &cases {
        let name = case["feature"].as_str().unwrap();
        if !features.contains_key(name) {
            features.insert(
                name.to_owned(),
                loader.feature(&Value::String(name.into())).unwrap(),
            );
        }
    }
    loader
        .feature(&Value::String("minecraft:pale_moss_patch".into()))
        .unwrap();
    let registry = loader.finish();
    let mut baselines = BTreeMap::new();
    let mut matches = 0;
    let mut unpended = 0;
    let mut block_entity_prefixes = 0;
    let mut pending = BTreeMap::<String, usize>::new();
    for case in &cases {
        let name = case["feature"].as_str().unwrap();
        let feature = &features[name];
        let origin: [i32; 3] = std::array::from_fn(|i| case["origin"][i].as_i64().unwrap() as i32);
        let ground = BlockState::from_id(case["ground"].as_u64().unwrap() as u16).unwrap();
        let terrain = case["terrain"].as_str().unwrap();
        let center = [origin[0] >> 4, origin[2] >> 4];
        let baseline = baselines
            .entry((origin, terrain.to_owned(), ground.id()))
            .or_insert_with(|| {
                let mut chunks = Vec::new();
                for cx in center[0] - 3..=center[0] + 3 {
                    for cz in center[1] - 3..=center[1] + 3 {
                        let mut chunk = generator.new_chunk([cx, cz]);
                        for x in 0..16 {
                            for z in 0..16 {
                                for y in -64..=origin[1] + 31 {
                                    let state = initial(
                                        [cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)],
                                        origin,
                                        terrain,
                                        ground,
                                    );
                                    if state.block() != Block::AIR {
                                        chunk.set_block(x, y, z, state);
                                    }
                                }
                            }
                        }
                        chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                    }
                }
                chunks
            });
        let seed = case["seed"].as_i64().unwrap();
        let label = format!("{name} {seed} {origin:?} {terrain}");
        let mut results = Vec::new();
        for behavior in [
            &OriginalTreeBehavior as &dyn WorldgenBlockBehavior,
            &GameWorldgenBlockBehavior,
        ] {
            let mut chunks = baseline.clone();
            let mut random =
                WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(seed)));
            let mut region = WorldGenRegion::new(
                &mut chunks,
                center,
                &[ChunkStatus::Terrain; 4],
                3,
                None,
                behavior,
                8675309,
                63,
            )
            .unwrap();
            region.set_has_skylight(true);
            region.set_block_tags(generator.data.block_tags.clone());
            region.set_fluid_tags(generator.data.fluid_tags.clone());
            region.set_random(Random::Legacy(LegacyRandomSource::new(123)));
            let result = feature.place(&mut FeatureContext {
                level: WorldGenLevel::Region(&mut region),
                random: FeatureRandomSelector::External(FeatureRandom::Region(&mut random)),
                origin,
                generator: &generator,
                registry: &registry,
            });
            results.push((result, random.next_long(), chunks));
        }
        let (original, _, _) = &results[0];
        let (result, next, chunks) = &results[1];
        match result {
            Err(error)
                if error.0 == "Unsupported: populate_worldgen_beehive"
                    || error.0 == "Unsupported: load_block_entity"
                    || error.0.contains("UnsupportedBlockMethod(")
                    || error.0 == "Unsupported: place_liquid" =>
            {
                *pending.entry(error.0.clone()).or_default() += 1;
                if error.0 == "Unsupported: populate_worldgen_beehive" {
                    assert_eq!(
                        *next,
                        case["before_bees_next"].as_i64().unwrap(),
                        "before bees RNG {label}"
                    );
                    assert_eq!(
                        placed(chunks, origin, terrain, ground),
                        case["before_bees"],
                        "before bees blocks {label}"
                    );
                }
            }
            Err(error) => panic!("unexpected tree bridge error {label}: {error}"),
            Ok(success) => {
                assert_eq!(
                    *success,
                    case["success"].as_bool().unwrap(),
                    "success {label}"
                );
                assert_eq!(*next, case["next"].as_i64().unwrap(), "RNG {label}");
                assert_eq!(
                    placed(chunks, origin, terrain, ground),
                    case["blocks"],
                    "blocks {label}"
                );
                let expected_entities = case["block_entities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entity| {
                        let pos =
                            std::array::from_fn(|i| entity["position"][i].as_i64().unwrap() as i32);
                        let nbt =
                            ferraloom_nbt::parse_compound(entity["nbt"].as_str().unwrap()).unwrap();
                        (pos, nbt)
                    })
                    .collect::<BTreeMap<[i32; 3], _>>();
                let actual_entities = chunks
                    .iter()
                    .flat_map(|chunk| chunk.block_entities.iter().map(|(pos, tag)| (*pos, tag)))
                    .collect::<BTreeMap<_, _>>();
                assert_eq!(
                    actual_entities.keys().collect::<Vec<_>>(),
                    expected_entities.keys().collect::<Vec<_>>(),
                    "block entity positions {label}"
                );
                let mut materialization_pending = None;
                for (pos, native) in &expected_entities {
                    let actual = actual_entities[pos];
                    if actual
                        .get_string("id")
                        .is_some_and(|id| id.to_string_lossy() == "DUMMY")
                    {
                        // WorldGenRegion#setBlock leaves this placeholder for every
                        // block entity until the canonical factory loads it. The
                        // ServerLevel oracle serializes a real receiver, so this
                        // is a prefix match.
                        let id = native.get_string("id").unwrap().to_string_lossy();
                        assert_ne!(id, "DUMMY", "{label}");
                        for (axis, value) in ["x", "y", "z"].into_iter().zip(pos) {
                            assert_eq!(actual.get_int(axis), Some(*value), "{label}");
                        }
                        materialization_pending = Some(id);
                    } else {
                        assert_eq!(actual, native, "block entity NBT {label}");
                    }
                }
                if let Some(id) = materialization_pending {
                    *pending
                        .entry(format!("{id} block-entity materialization"))
                        .or_default() += 1;
                    block_entity_prefixes += 1;
                    continue;
                }
                matches += 1;
                if original.is_err() {
                    unpended += 1;
                }
            }
        }
    }
    assert_eq!(matches + pending.values().sum::<usize>(), 2444);
    assert!(matches > 0);
    eprintln!(
        "tree game bridge: {matches}/2444 full matches, {unpended} un-pended; pending {pending:?}; {block_entity_prefixes} block-entity prefixes checked"
    );
}
