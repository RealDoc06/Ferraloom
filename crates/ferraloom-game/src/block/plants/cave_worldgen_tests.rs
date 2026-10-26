//! Original cave feature fixtures replayed through the real plant bridge.
use crate::block::GameWorldgenBlockBehavior;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::random::{Algorithm, LegacyRandomSource, Random, RandomSource, WorldgenRandom};
use ferraloom_worldgen::{
    Error,
    feature::{
        Feature, FeatureContext, FeatureLoader, FeatureRandom, FeatureRandomSelector,
        FeatureRegistry, WorldGenLevel,
    },
    generator::NoiseBasedChunkGenerator,
    region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::DimensionType,
};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};

struct FixtureBehavior;
impl WorldgenBlockBehavior for FixtureBehavior {
    fn can_survive(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
    ) -> Result<bool, Error> {
        Err(Error(format!(
            "Unsupported: {}#canSurvive ({})",
            state.block().class().java_name(),
            state.block().name()
        )))
    }
    fn post_process_pos(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Err(Error("Unsupported: contextual post-processing".into()))
    }
}

fn cases() -> Vec<Value> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../ferraloom-worldgen/src/feature/cave/vanilla.json.gz")
            .as_slice(),
    )
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
        "air" => Block::AIR,
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
                    fast_lava: false,
                    coordinate_scale: 1.0,
                    infiniburn: ferraloom_worldgen::structure::processor::BlockSet::Tag(
                        "minecraft:infiniburn_overworld".parse().unwrap(),
                    ),
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
fn check(
    case: &Value,
    generator: &NoiseBasedChunkGenerator,
    feature: &dyn Feature,
    registry: &FeatureRegistry,
    base: &[RegionChunk],
    behavior: &dyn WorldgenBlockBehavior,
) -> Result<(), Error> {
    assert!(
        case.get("vanilla_pending").is_none(),
        "vanilla fixture incomplete: {case}"
    );
    let seed = case["seed"].as_i64().unwrap();
    let origin = point(&case["origin"]);
    let terrain = case["terrain"].as_str().unwrap();
    let mut chunks = base.to_vec();
    let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(seed));
    let center = [origin[0] >> 4, origin[2] >> 4];
    let registry_context = crate::level::LevelContext::fixture(0, 63);
    let mut region = WorldGenRegion::new(
        &mut chunks,
        center,
        &[ChunkStatus::Features; 3],
        2,
        Some((-64, 128)),
        behavior,
        seed,
        63,
    )
    .unwrap();
    region.set_registry_context(registry_context.registry_context().unwrap());
    region.set_has_skylight(true);
    region.set_block_tags(generator.data.block_tags.clone());
    region.set_fluid_tags(generator.data.fluid_tags.clone());
    region.set_random(Random::Legacy(LegacyRandomSource::new(123)));
    let mut context = FeatureContext {
        level: WorldGenLevel::Region(&mut region),
        random: FeatureRandomSelector::External(FeatureRandom::Region(&mut random)),
        origin,
        generator,
        registry,
    };
    let result = feature.place(&mut context)?;
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
    for (c, before) in chunks.iter().zip(base) {
        let chunk = &c.chunk;
        for (section_index, (section, baseline)) in chunk
            .sections
            .iter()
            .zip(&before.chunk.sections)
            .enumerate()
        {
            let states = section.states();
            let old_states = baseline.states();
            if states.bits_per_entry() == old_states.bits_per_entry()
                && states.palette_entries() == old_states.palette_entries()
                && states.raw() == old_states.raw()
            {
                continue;
            }
            for y in 0..16 {
                for z in 0..16 {
                    for x in 0..16 {
                        let state = section.get_block_state(x, y, z);
                        if state != baseline.get_block_state(x, y, z) {
                            let p = [
                                chunk.position[0] * 16 + i32::from(x),
                                chunk.min_y + section_index as i32 * 16 + i32::from(y),
                                chunk.position[1] * 16 + i32::from(z),
                            ];
                            blocks.insert(p, state.id());
                        }
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
    // OreCaveGolden's proxy saves live block entities. Real WorldGenRegion
    // protochunks retain DUMMY position tags, covered by the real-region oracle.
    let expected_entities = case["block_entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let pos = point(&row[0]);
            let mut marker = ferraloom_nbt::CompoundTag::new();
            marker.insert("id", ferraloom_nbt::Tag::String("DUMMY".into()));
            for (key, value) in [("x", pos[0]), ("y", pos[1]), ("z", pos[2])] {
                marker.insert(key, ferraloom_nbt::Tag::Int(value));
            }
            (pos, marker)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        entities, expected_entities,
        "protochunk block entities {label}"
    );
    let group_ticks = |values: &[Value]| {
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for value in values {
            let p = point(value);
            groups
                .entry((p[0] >> 4, p[2] >> 4, value[3].as_str().unwrap().to_owned()))
                .or_default()
                .push(value.clone());
        }
        groups
    };
    assert_eq!(
        group_ticks(&ticks),
        group_ticks(case["ticks"].as_array().unwrap()),
        "scheduled tick order {label}"
    );
    Ok(())
}
#[test]
fn cave_multiface_and_vegetation_fixtures_use_real_game_bridge() {
    let (resources, generator) = super::worldgen_tests::setup();
    let selected = cases()
        .into_iter()
        .filter(|case| {
            matches!(
                case["config"]["type"].as_str(),
                Some(
                    "minecraft:multiface_growth"
                        | "minecraft:vegetation_patch"
                        | "minecraft:waterlogged_vegetation_patch"
                        | "minecraft:sculk_patch"
                )
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 264);
    let mut loader = FeatureLoader::new(&resources).unwrap();
    let mut features = BTreeMap::new();
    for case in &selected {
        let name = case["feature"].as_str().unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) = features.entry(name.to_owned()) {
            entry.insert(loader.feature(&case["config"]).unwrap());
        }
    }
    let registry = loader.finish();
    let mut templates = BTreeMap::new();
    let mut coverage = BTreeMap::<String, (usize, usize)>::new();
    for case in selected {
        let terrain = case["terrain"].as_str().unwrap();
        let origin = point(&case["origin"]);
        let base = templates
            .entry((terrain.to_owned(), origin))
            .or_insert_with(|| chunks(&generator, terrain, origin));
        let name = case["feature"].as_str().unwrap();
        let feature = features[name].as_ref();
        let original_pending = match check(
            &case,
            &generator,
            feature,
            &registry,
            base,
            &FixtureBehavior,
        ) {
            Ok(()) => false,
            Err(error) => {
                assert!(
                    error.0.contains("Unsupported") || error.0.contains("unavailable"),
                    "unexpected original error: {error}"
                );
                true
            }
        };
        check(
            &case,
            &generator,
            feature,
            &registry,
            base,
            &GameWorldgenBlockBehavior,
        )
        .unwrap_or_else(|error| panic!("{name} {terrain} seed={}: {error}", case["seed"]));
        let counts = coverage.entry(name.to_owned()).or_default();
        counts.0 += 1;
        counts.1 += usize::from(original_pending);
    }
    let unpended: usize = coverage.values().map(|counts| counts.1).sum();
    for (name, (matches, unpended)) in &coverage {
        eprintln!("Cave {name}: {matches} complete matches, {unpended} un-pended");
    }
    eprintln!("Cave game bridge: 264 complete matches, {unpended} un-pended");
    assert!(
        unpended > 0,
        "real plant bridge must complete pending cave fixtures"
    );
}
