//! Real ServerLevel callbacks with captured light/environment inputs and RNG.
use super::{
    super::BlockMethod,
    tests::{level_with_height, tags},
};
use crate::level::{
    Level,
    environment::{ClockSnapshot, EnvironmentDefinitions},
    weather::ChunkLightSnapshot,
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_light::DataLayer;
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{LegacyRandomSource, Random, RandomSource},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, OnceLock},
};

const POS: BlockPos = BlockPos::new(8, 100, 8);
type Cells = BTreeMap<(i32, i32, i32), BlockState>;
fn cells(text: &str) -> Cells {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(|cell| {
            let c: Vec<i32> = cell.split(':').map(|s| s.parse().unwrap()).collect();
            (
                (c[0], c[1], c[2]),
                BlockState::from_id(c[3] as u16).unwrap(),
            )
        })
        .collect()
}
fn snapshot(level: &Level) -> Cells {
    let mut result = BTreeMap::new();
    for z in -3..=3 {
        for y in -20..=20 {
            for x in -3..=3 {
                let state = level.get_block_state(POS.offset(x, y, z)).unwrap();
                if !state.is_air() {
                    result.insert((x, y, z), state);
                }
            }
        }
    }
    result
}
fn setup(before: &str, seed: i64) -> Level {
    let mut level = level_with_height(8);
    let (blocks, fluids) = tags();
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    level.rules_mut().block_drops = Some(false);
    super::fixtures::install_loot(&mut level);
    for ((x, y, z), state) in cells(before) {
        level.set_block_state(POS.offset(x, y, z), state).unwrap();
    }
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(seed)), 0);
    level
}
fn environment(level: &mut Level, sky_darken: i32, evaporates: bool, eyeblossom: &str) {
    static DEFINITIONS: OnceLock<Arc<EnvironmentDefinitions>> = OnceLock::new();
    let definitions = DEFINITIONS
        .get_or_init(|| {
            let mut pack = Pack::new("native-plant-observations");
            pack.insert(
                "worldgen/biome/fixture.json".parse().unwrap(),
                serde_json::to_vec(&json!({"attributes":{}})).unwrap(),
            );
            Arc::new(EnvironmentDefinitions::load(&ResourceManager::new(vec![pack])).unwrap())
        })
        .clone();
    // These are the observed inputs of the callback, not inferred clock/weather
    // defaults. The normal typed runtime performs every attribute query.
    let dimension = definitions
        .dimension(&json!({"attributes":{
            "gameplay/sky_light_level": 15 - sky_darken,
            "gameplay/water_evaporates": evaporates,
            "gameplay/eyeblossom_open": eyeblossom,
        }}))
        .unwrap();
    level
        .set_environment_attributes(
            definitions,
            &dimension,
            "overworld".parse().unwrap(),
            &["fixture".parse().unwrap()],
            ClockSnapshot::new([]),
        )
        .unwrap();
    assert_eq!(level.sky_darken().unwrap(), sky_darken);
}
fn ticks(level: &mut Level, fluid: bool) -> String {
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::containing(POS), &mut saved)
        .unwrap();
    let list = if fluid {
        saved.fluid_ticks
    } else {
        saved.block_ticks
    }
    .unwrap();
    // Saved ticks preserve source insertion order. Sort the captured records by
    // vanilla DRAIN_ORDER, as the Java harness does before recording its queue.
    let mut list: Vec<_> = list.into_iter().enumerate().collect();
    list.sort_by_key(|(order, tick)| (tick.delay.unwrap(), tick.priority.unwrap(), *order));
    list.into_iter()
        .filter_map(|(_, tick)| {
            let x = tick.x.unwrap() - POS.x;
            let y = tick.y.unwrap() - POS.y;
            let z = tick.z.unwrap() - POS.z;
            if x.abs() > 3 || y.abs() > 20 || z.abs() > 3 {
                return None;
            }
            let name = tick.id.unwrap().to_string_checked().unwrap();
            let id = if fluid {
                ferraloom_data::registries::fluid::Id::from_name(&name)
                    .unwrap()
                    .protocol_id()
            } else {
                Block::from_name(&name).unwrap().protocol_id()
            };
            Some(format!(
                "{x}:{y}:{z}:{id}:{}:{}",
                tick.delay.unwrap(),
                tick.priority.unwrap()
            ))
        })
        .collect::<Vec<_>>()
        .join(",")
}
#[test]
fn native_ground_ticks_match_changes_queues_and_rng() {
    let text = super::fixtures::native_text(
        "ground-tick",
        include_bytes!("../../../tests/java/blocks/plants/ground-ticks.tsv.gz"),
    );
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[8], f[2].parse().unwrap());
        let darken: i32 = f[5].parse().unwrap();
        environment(&mut level, darken, f[7] == "true", "default");
        let mut sky = DataLayer::new(0);
        let mut block = DataLayer::new(0);
        sky.set(8, (POS.y & 15) as u8, 8, f[3].parse().unwrap());
        block.set(8, (POS.y & 15) as u8, 8, f[4].parse().unwrap());
        // Only the raw maximum above is read here. The native record supplies
        // that value separately from the center's sky and block light layers.
        let above: u8 = f[6].parse().unwrap();
        sky.set(
            8,
            ((POS.y + 1) & 15) as u8,
            8,
            (i32::from(above) + darken).clamp(0, 15) as u8,
        );
        level.set_light_snapshot(
            ChunkPos::containing(POS),
            ChunkLightSnapshot {
                sky: [(POS.y >> 4, sky)].into(),
                block: [(POS.y >> 4, block)].into(),
            },
        );
        assert_eq!(
            level.max_local_raw_brightness(POS.above(1)).unwrap(),
            i32::from(above),
            "light {line}"
        );
        let method = if state.block() == Block::FROSTED_ICE {
            BlockMethod::Tick
        } else {
            BlockMethod::RandomTick
        };
        let behavior = level
            .required_block_behavior(state.block(), method)
            .unwrap();
        if method == BlockMethod::Tick {
            behavior.tick(state, POS, &mut level).unwrap();
        } else {
            behavior.random_tick(state, POS, &mut level).unwrap();
        }
        assert_eq!(snapshot(&level), cells(f[9]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[10], "block ticks {line}");
        assert_eq!(ticks(&mut level, true), f[11], "fluid ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[12].parse::<i64>().unwrap(),
            "random {line}"
        );
        count += 1;
    }
    assert_eq!(
        count,
        if std::env::var_os("FERRALOOM_PLANTS_ORACLE").is_some() {
            1360
        } else {
            720
        }
    );
}
#[test]
fn native_eyeblossom_transitions_match_queues_and_rng() {
    let text = super::fixtures::native_text(
        "eyeblossom-tick",
        include_bytes!("../../../tests/java/blocks/plants/eyeblossom-ticks.tsv.gz"),
    );
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[5], f[2].parse().unwrap());
        environment(&mut level, 0, false, f[4]);
        let method = if f[3] == "true" {
            BlockMethod::Tick
        } else {
            BlockMethod::RandomTick
        };
        let behavior = level
            .required_block_behavior(state.block(), method)
            .unwrap();
        if method == BlockMethod::Tick {
            behavior.tick(state, POS, &mut level).unwrap();
        } else {
            behavior.random_tick(state, POS, &mut level).unwrap();
        }
        assert_eq!(snapshot(&level), cells(f[6]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[7], "ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "random {line}"
        );
        count += 1;
    }
    assert_eq!(count, 192);
}

fn cow(level: &mut Level, y: f64, on_ground: bool) -> crate::entity::EntityRef {
    use crate::entity::{EntityData, EntityState, animal::Cow};
    let actor = level.next_entity_ref();
    let mut data = EntityData::new(ferraloom_data::EntityType::COW, 1, 0);
    data.set_pos(ferraloom_util::Vec3::new(8.5, y, 8.5));
    data.on_ground = on_ground;
    level
        .add_entity(actor, EntityState::Cow(Box::new(Cow::with_parent(data))))
        .unwrap();
    actor
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_chorus_growth_matches_changes_ticks_and_rng() {
    let text = super::fixtures::native_text("chorus-tick", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[3], f[2].parse().unwrap());
        level
            .required_block_behavior(state.block(), BlockMethod::RandomTick)
            .unwrap()
            .random_tick(state, POS, &mut level)
            .unwrap();
        assert_eq!(snapshot(&level), cells(f[4]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[5], "ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[6].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 1344);
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_dripleaf_callbacks_match_signal_tilt_queues_and_rng() {
    let text = super::fixtures::native_text("dripleaf-tick", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[5], f[2].parse().unwrap());
        let mode: u8 = f[3].parse().unwrap();
        if mode >= 2 {
            let y = f64::from(POS.y) + if mode == 3 { 0.6875 } else { 1.0 };
            let actor = cow(&mut level, y, mode != 4);
            level
                .required_block_behavior(state.block(), BlockMethod::EntityInside)
                .unwrap()
                .entity_inside(state, POS, actor, &mut level)
                .unwrap();
        } else if mode == 1 {
            level
                .required_block_behavior(state.block(), BlockMethod::NeighborChanged)
                .unwrap()
                .neighbor_changed(state, POS, Block::REDSTONE_BLOCK, None, false, &mut level)
                .unwrap();
        } else {
            level
                .required_block_behavior(state.block(), BlockMethod::Tick)
                .unwrap()
                .tick(state, POS, &mut level)
                .unwrap();
        }
        assert_eq!(snapshot(&level), cells(f[6]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[7], "ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 1280);
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_harvest_keeps_named_loot_and_level_random_streams() {
    use crate::{
        block::UseContext,
        entity::{
            EntityState,
            player::{GameType, ServerPlayer},
        },
    };
    use ferraloom_protocol::play::{BlockHitResult, InteractionHand};
    use ferraloom_util::{Direction, Uuid, Vec3};
    let text = super::fixtures::native_text("harvest", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup("", f[2].parse().unwrap());
        assert_eq!(level.context().seed, f[7].parse::<i64>().unwrap());
        level
            .set_block_state(POS.below(1), Block::DIRT.default_state())
            .unwrap();
        level.set_block_state(POS, state).unwrap();
        let actor = level.next_entity_ref();
        level
            .add_entity(
                actor,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(0, 1),
                    "fixture".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let context = UseContext {
            player: actor,
            hand: InteractionHand::MainHand,
            hit: BlockHitResult {
                block_pos: POS,
                direction: Direction::Up,
                offset_x: 0.5,
                offset_y: 0.5,
                offset_z: 0.5,
                inside: false,
                world_border_hit: false,
            },
        };
        let result = level
            .required_block_behavior(state.block(), BlockMethod::UseWithoutItem)
            .unwrap()
            .use_without_item(state, POS, &context, &mut level)
            .unwrap();
        assert_eq!(result.consumes_action(), f[3] == "true", "result {line}");
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[4].parse::<u16>().unwrap(),
            "state {line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[5].parse::<i64>().unwrap(),
            "level RNG {line}"
        );
        let table =
            ferraloom_protocol::Identifier::parse(if state.block() == Block::SWEET_BERRY_BUSH {
                "harvest/sweet_berry_bush"
            } else {
                "harvest/cave_vine"
            })
            .unwrap();
        let runtime = level.container_runtime_mut();
        assert_eq!(
            runtime
                .sequences
                .get(&table, runtime.world_seed)
                .next_long(),
            f[6].parse::<i64>().unwrap(),
            "named RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 464);
}
fn shape_hex(shape: &ferraloom_util::VoxelShape) -> String {
    shape
        .to_aabbs()
        .iter()
        .map(|bb| {
            [bb.min_x, bb.min_y, bb.min_z, bb.max_x, bb.max_y, bb.max_z]
                .map(|v| format!("{:x}", v.to_bits()))
                .join(":")
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_powder_snow_context_shapes_match_thresholds_equipment_and_placement() {
    use crate::{
        block::{BlockCollisionContext, LevelReaderRef},
        entity::{EntityData, LivingEntity, collision_context::EntityCollisionView},
    };
    use ferraloom_protocol::component::{EquipmentSlot, ItemStack};
    let text = super::fixtures::native_text("powder-shape", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = setup("", 42);
        let offset = f64::from_bits(u64::from_str_radix(f[4], 16).unwrap());
        let actor = cow(&mut level, f64::from(POS.y) + offset, false);
        let base = level.entity_mut::<EntityData>(actor).unwrap();
        base.fall_distance = f64::from_bits(u64::from_str_radix(f[2], 16).unwrap());
        base.set_shared_flag(1, f[3] == "true");
        if f[1] == "true" {
            level
                .entity_mut::<LivingEntity>(actor)
                .unwrap()
                .equipment
                .set(
                    EquipmentSlot::Feet,
                    ItemStack::new(ferraloom_data::Item::LEATHER_BOOTS, 1, Default::default()),
                );
        }
        let mut context = EntityCollisionView::capture(actor, &level, false).unwrap();
        context.placement = f[5] == "true";
        let reader = LevelReaderRef::Server(&level);
        let state = Block::POWDER_SNOW.default_state();
        assert_eq!(
            shape_hex(
                &reader
                    .collision_shape(state, POS, &BlockCollisionContext::Entity(context))
                    .unwrap()
            ),
            f[6],
            "collision {line}"
        );
        assert_eq!(
            shape_hex(
                &reader
                    .entity_inside_collision_shape(state, POS, actor)
                    .unwrap()
            ),
            f[7],
            "inside {line}"
        );
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 96);
}

fn feature_services() -> Arc<crate::level::LevelContext> {
    static SERVICES: OnceLock<Arc<crate::level::LevelContext>> = OnceLock::new();
    SERVICES
        .get_or_init(|| {
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
                .expect("builder supplies native datapack");
            let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
            use ferraloom_worldgen::generator::{
                ChunkGenerator, GenerationData,
                flat::{FlatLevelGeneratorSettings, FlatLevelSource},
            };
            let settings = FlatLevelGeneratorSettings::decode(
                &json!({
                    "biome":"minecraft:plains",
                    "layers":[{"block":"minecraft:bedrock","height":1}],
                }),
                &resources,
            )
            .unwrap();
            let generator = ChunkGenerator::Flat(Box::new(
                FlatLevelSource::new(
                    Arc::new(GenerationData::load(&resources).unwrap()),
                    settings,
                )
                .unwrap(),
            ));
            let features = ferraloom_worldgen::feature::FeatureLoader::new(&resources)
                .unwrap()
                .load_all()
                .unwrap();
            let base = crate::level::LevelContext::fixture(0, -63);
            Arc::new(crate::level::LevelContext::new(
                0,
                -63,
                Arc::new(generator),
                Arc::new(features),
                base.structures.clone(),
                base.damage_types().clone(),
                base.initial_difficulty,
            ))
        })
        .clone()
}
/// Real native fixture dimension services and biome palette, with enough owned
/// chunks for the grass walk and flower feature's full synchronous read range.
fn feature_level(biome: &str, seed: i64) -> Level {
    use crate::{
        Behaviors,
        level::{Region, chunk::LevelChunk, weather::WeatherEnvironment},
    };
    use ferraloom_chunk::{IdMap, LevelChunkSection, Strategy};
    static EMPTY: Behaviors = Behaviors::EMPTY;
    let services = feature_services();
    let data = services.generator.access().data();
    let biome = data.biome_ids[&biome.parse().unwrap()];
    let mut level = {
        let mut fixture_level = Level::new(Region::new(&EMPTY), services.clone());
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    level.set_has_skylight(true);
    level.set_block_tags(data.block_tags.clone());
    level.set_fluid_tags(data.fluid_tags.clone());
    level.set_weather_environment(WeatherEnvironment {
        temperatures: data.temperatures.clone(),
        biome_names: Arc::new(data.biome_names.clone()),
        biome_zoom_seed: ferraloom_worldgen::biome_manager::obfuscate_seed(services.seed),
        sea_level: services.sea_level,
    });
    for x in -1..=1 {
        for z in -1..=1 {
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(x, z),
                    -4..20,
                    LevelChunkSection::new(
                        Strategy::block_states(),
                        Strategy::biomes(Arc::new(IdMap::new(vec![biome]).unwrap())),
                        biome,
                    ),
                ))
                .unwrap();
        }
    }
    for x in -16..32 {
        for z in -16..32 {
            level
                .set_block_state(BlockPos::new(x, -64, z), Block::BEDROCK.default_state())
                .unwrap();
        }
    }
    super::fixtures::install_loot(&mut level);
    level.rules_mut().block_drops = Some(false);
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(seed)), 0);
    level
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_sapling_targets_preserve_height_flower_mega_checks_and_rng() {
    use super::{BonemealSource, bonemealable_behavior};
    use crate::block::LevelAccessRef;
    use ferraloom_data::properties as p;
    let text = super::fixtures::native_text("sapling-target", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = feature_level("plains", f[3].parse().unwrap());
        if f[2] == "1" {
            for x in 0..=1 {
                for z in 0..=1 {
                    level
                        .set_block_state(POS.offset(x, -1, z), Block::DIRT.default_state())
                        .unwrap();
                    level.set_block_state(POS.offset(x, 0, z), state).unwrap();
                }
            }
            level
                .set_block_state(POS.offset(-1, 0, 1), Block::DANDELION.default_state())
                .unwrap();
        }
        let pos = BlockPos::new(POS.x, f[4].parse().unwrap(), POS.z);
        level
            .set_block_state(pos.below(1), Block::DIRT.default_state())
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        let behavior = bonemealable_behavior(state.block()).unwrap();
        let mut access = LevelAccessRef::Server(&mut level);
        let valid = behavior
            .is_valid_bonemeal_target(state, pos, &mut access, BonemealSource::Interaction)
            .unwrap();
        let success = valid
            && behavior
                .is_bonemeal_success(state, pos, &mut access, BonemealSource::Interaction)
                .unwrap();
        assert_eq!(valid, f[5] == "true", "target {line}");
        assert_eq!(success, f[6] == "true", "success {line}");
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        if state.block() == Block::MANGROVE_PROPAGULE && state.get(p::HANGING) == Some(true) {
            assert_eq!(valid, state.get(p::AGE) != Some(4));
        }
        count += 1;
    }
    assert_eq!(count, 1440);
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_grass_bonemeal_matches_live_feature_blocks_and_rng() {
    use super::{BonemealSource, bonemealable_behavior};
    use crate::block::LevelAccessRef;
    let text = super::fixtures::native_text("grass-bonemeal", &[]);
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = feature_level(f[3], f[1].parse().unwrap());
        assert_eq!(level.context().seed, f[4].parse::<i64>().unwrap());
        assert_eq!(level.context().sea_level, f[5].parse::<i32>().unwrap());
        for ((x, y, z), state) in cells(f[6]) {
            level.set_block_state(POS.offset(x, y, z), state).unwrap();
        }
        assert_eq!(level.biome_at(POS).unwrap().to_string(), f[3]);
        bonemealable_behavior(Block::GRASS_BLOCK)
            .unwrap()
            .perform_bonemeal(
                Block::GRASS_BLOCK.default_state(),
                POS.below(1),
                &mut LevelAccessRef::Server(&mut level),
                BonemealSource::Interaction,
            )
            .unwrap();
        assert_eq!(snapshot(&level), cells(f[7]), "blocks {line}");
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "RNG {line}"
        );
        count += 1;
    }
    assert_eq!(count, 48);
}

#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_ground() {
    native_ground_ticks_match_changes_queues_and_rng();
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_eyeblossom() {
    native_eyeblossom_transitions_match_queues_and_rng();
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_bonemeal() {
    super::tests::native_bonemeal_snapshots_and_rng_match_registered_growth();
}
#[test]
#[ignore = "requires actual ServerLevel oracle capture"]
fn native_live_oracle_crops() {
    super::tests::crop_random_ticks_match_real_server_outcomes_and_rng();
}
