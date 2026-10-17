use super::*;
use crate::{
    biome::BiomeSource,
    feature::{FeatureLoader, FeatureRegistry},
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldgenBlockBehavior},
    settings::{DimensionType, NoiseGeneratorSettings},
};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_nbt::parse_compound;
use ferraloom_util::random::{Algorithm, WorldgenRandom};
use std::{collections::BTreeMap, io::Read, path::Path};

struct FixtureBehavior;
impl WorldgenBlockBehavior for FixtureBehavior {
    fn has_block_entity(&self, state: BlockState) -> bool {
        state.has_block_entity()
    }
    fn post_process_pos(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        p: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Ok(state.post_process_y_offset().map(|y| offset(p, [0, y, 0])))
    }
    fn has_sturdy_face(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
        face: Direction,
    ) -> Result<bool, Error> {
        state
            .cached_sturdy_face(face)
            .ok_or_else(|| Error("Unsupported: dynamic sturdy face".into()))
    }
}
fn cases() -> Vec<Value> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("vanilla.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
}
fn initial(terrain: &str, origin: [i32; 3], p: [i32; 3]) -> BlockState {
    if p[1] < -64 || p[1] >= 64 {
        return Block::VOID_AIR.default_state();
    }
    let [x, y, z] = std::array::from_fn(|i| p[i] - origin[i]);
    if let Some(name) = terrain.strip_prefix("spring:") {
        return if y == 0 && z == 0 && (0..=3).contains(&x) {
            Block::AIR.default_state()
        } else {
            Block::from_name(name).unwrap().default_state()
        };
    }
    let block = match terrain {
        "nether" => Block::NETHERRACK,
        "deepslate" => Block::DEEPSLATE,
        "surface" => {
            if y > 0 {
                Block::AIR
            } else if y >= -3 {
                Block::DIRT
            } else {
                Block::STONE
            }
        }
        "water" => {
            if (0..=7).contains(&y) {
                Block::WATER
            } else {
                Block::STONE
            }
        }
        "cave" => {
            if (0..=7).contains(&y) {
                Block::CAVE_AIR
            } else {
                Block::STONE
            }
        }
        "pockets" => {
            if x.rem_euclid(7) == 0 && z.rem_euclid(5) <= 1 && (-5..=3).contains(&y) {
                Block::AIR
            } else {
                Block::STONE
            }
        }
        "room" => {
            if (0..=1).contains(&y) && z == 0 && (0..=7).contains(&x) {
                Block::AIR
            } else {
                Block::STONE
            }
        }
        _ => Block::STONE,
    };
    block.default_state()
}
fn generator(resources: &ResourceManager) -> NoiseBasedChunkGenerator {
    let data = Arc::new(GenerationData::load(resources).unwrap());
    let settings = resources
        .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    NoiseBasedChunkGenerator::new(
        data,
        NoiseGeneratorSettings::decode(&settings).unwrap(),
        BiomeSource::Fixed("plains".parse().unwrap()),
        0,
    )
    .unwrap()
}
fn chunks(
    generator: &NoiseBasedChunkGenerator,
    terrain: &str,
    origin: [i32; 3],
) -> Vec<RegionChunk> {
    let center = [origin[0] >> 4, origin[2] >> 4];
    let mut out = Vec::new();
    for cx in center[0] - 2..=center[0] + 2 {
        for cz in center[1] - 2..=center[1] + 2 {
            let mut chunk = generator.new_chunk_in_dimension(
                [cx, cz],
                DimensionType {
                    min_y: -64,
                    height: 128,
                    logical_height: 128,
                    has_ceiling: false,
                    has_skylight: true,
                    coordinate_scale: 1.0,
                },
            );
            for y in -64..64 {
                for z in 0..16 {
                    for x in 0..16 {
                        chunk.set_block(
                            x,
                            y,
                            z,
                            initial(
                                terrain,
                                origin,
                                [cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)],
                            ),
                        );
                    }
                }
            }
            out.push(RegionChunk::new(chunk, ChunkStatus::Features));
        }
    }
    out
}
fn pending(case: &Value) -> Option<&'static str> {
    match case["config"]["type"].as_str().unwrap() {
        "minecraft:fossil" => Some("template placement game callbacks"),
        "minecraft:vegetation_patch" | "minecraft:waterlogged_vegetation_patch" => {
            Some("nested vegetation_feature and block survival")
        }
        "minecraft:multiface_growth" => {
            Some("MultifaceSpreadeableBlock placement and MultifaceSpreader")
        }
        "minecraft:sculk_patch" => Some("SculkBehaviour collision check and SculkSpreader"),
        "minecraft:monster_room" if case["result"] == true => {
            Some("RandomizableContainer loot table and SpawnerBlockEntity initialization")
        }
        _ => None,
    }
}
fn check(
    case: &Value,
    generator: &NoiseBasedChunkGenerator,
    resources: &ResourceManager,
    base: &[RegionChunk],
) {
    assert!(
        case.get("vanilla_pending").is_none(),
        "vanilla fixture incomplete: {case}"
    );
    let mut loader = FeatureLoader::new(resources).unwrap();
    let feature = loader.feature(&case["config"]).unwrap();
    let registry: FeatureRegistry = loader.finish();
    let seed = case["seed"].as_i64().unwrap();
    let origin = point(&case["origin"]);
    let terrain = case["terrain"].as_str().unwrap();
    let mut chunks = base.to_vec();
    let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(seed));
    let center = [origin[0] >> 4, origin[2] >> 4];
    let mut region = WorldGenRegion::new(
        &mut chunks,
        center,
        &[ChunkStatus::Features; 3],
        2,
        Some((-64, 128)),
        &FixtureBehavior,
        seed,
        63,
    )
    .unwrap();
    let mut context = FeatureContext {
        region: &mut region,
        random: &mut random,
        origin,
        generator,
        registry: &registry,
    };
    let result = feature.place(&mut context).unwrap();
    let label = format!(
        "{} seed={seed} origin={origin:?} {terrain}",
        case["feature"]
    );
    assert_eq!(result, case["result"].as_bool().unwrap(), "result {label}");
    assert_eq!(
        random.next_long(),
        case["next"].as_i64().unwrap(),
        "RNG {label}"
    );
    let mut blocks = BTreeMap::new();
    let mut entities = BTreeMap::new();
    let mut ticks = Vec::new();
    for c in &chunks {
        let chunk = &c.chunk;
        for y in -64..64 {
            for z in 0..16 {
                for x in 0..16 {
                    let p = [
                        chunk.position[0] * 16 + i32::from(x),
                        y,
                        chunk.position[1] * 16 + i32::from(z),
                    ];
                    let state = chunk.block(x, y, z);
                    if state != initial(terrain, origin, p) {
                        blocks.insert(p, state.id());
                    }
                }
            }
        }
        entities.extend(c.block_entities.iter().map(|(p, t)| (*p, t.clone())));
        for tick in c.block_ticks.ticks() {
            ticks.push(serde_json::json!([
                tick.position[0],
                tick.position[1],
                tick.position[2],
                "block",
                tick.kind.name(),
                tick.priority
            ]));
        }
        for tick in c.fluid_ticks.ticks() {
            ticks.push(serde_json::json!([
                tick.position[0],
                tick.position[1],
                tick.position[2],
                "fluid",
                tick.kind.name(),
                tick.priority
            ]));
        }
    }
    let expected = case["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (point(v), v[3].as_u64().unwrap() as u16))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(blocks, expected, "blocks {label}");
    let expected_entities = case["block_entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                point(&v[0]),
                parse_compound(v[1].as_str().unwrap()).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(entities, expected_entities, "block entities {label}");
    let mut expected_ticks = case["ticks"].as_array().unwrap().clone();
    ticks.sort_by_key(Value::to_string);
    expected_ticks.sort_by_key(Value::to_string);
    assert_eq!(ticks, expected_ticks, "scheduled ticks {label}");
}
fn run(select: impl Fn(&Value) -> bool) {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let generator = generator(&resources);
    let mut templates = BTreeMap::new();
    for case in cases().into_iter().filter(select) {
        let terrain = case["terrain"].as_str().unwrap();
        let origin = point(&case["origin"]);
        let base = templates
            .entry((terrain.to_owned(), origin))
            .or_insert_with(|| chunks(&generator, terrain, origin));
        check(&case, &generator, &resources, base);
    }
}
#[test]
fn real_ore_and_cave_feature_placements_match_vanilla() {
    run(|case| pending(case).is_none());
}
#[test]
#[ignore = "needs shared MultifaceSpreadeableBlock placement and MultifaceSpreader"]
fn pending_multiface_behavior() {
    run(|case| case["config"]["type"] == "minecraft:multiface_growth");
}
#[test]
#[ignore = "needs shared SculkBehaviour and SculkSpreader"]
fn pending_sculk_behavior() {
    run(|case| case["config"]["type"] == "minecraft:sculk_patch");
}
#[test]
#[ignore = "needs nested vegetation_feature implementations and block survival"]
fn pending_vegetation_behavior() {
    run(|case| {
        matches!(
            case["config"]["type"].as_str(),
            Some("minecraft:vegetation_patch" | "minecraft:waterlogged_vegetation_patch")
        )
    });
}
#[test]
#[ignore = "needs RandomizableContainer#setBlockEntityLootTable and SpawnerBlockEntity#setEntityId"]
fn pending_monster_room_block_entities() {
    run(|case| case["config"]["type"] == "minecraft:monster_room" && case["result"] == true);
}
#[test]
#[ignore = "needs template placement game block callbacks"]
fn pending_fossil_behavior() {
    run(|case| case["config"]["type"] == "minecraft:fossil");
}
