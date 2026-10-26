use super::{fire, portal_shape::PortalShape, tests::POS};
use crate::{
    block::{BlockBehavior, BlockMethod, LevelReaderRef, fluid::FlowingFluid},
    entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    gamerules::{GameRule, GameRuleValue, GameRules},
    level::{
        Level, LevelContext, Region,
        chunk::{LevelChunk, LevelChunkSection},
        effects::LevelEffect,
    },
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
    properties as p,
};
use ferraloom_util::{
    Axis, BlockPos, ChunkPos, Direction, SectionPos, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};

fn decode(data: &[u8]) -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(data)
        .read_to_string(&mut text)
        .unwrap();
    text
}
static RADIUS: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-radius.tsv.gz"
    ))
});
static PORTALS: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-portal.tsv.gz"
    ))
});
static LAVA: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-lava.tsv.gz"
    ))
});
static TICKS: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-tick.tsv.gz"
    ))
});
static PLACEMENT: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-place.tsv.gz"
    ))
});
static CONTACTS: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-contact.tsv.gz"
    ))
});
static RAIN: LazyLock<String> = LazyLock::new(|| {
    decode(include_bytes!(
        "../../../tests/java/blocks/building/fire-rain.tsv.gz"
    ))
});
static TEMPERATURES: LazyLock<Arc<ferraloom_worldgen::biome_manager::BiomeTemperatures>> =
    LazyLock::new(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
            .expect("builder supplies native biome climates");
        Arc::new(
            ferraloom_worldgen::biome_manager::BiomeTemperatures::load(&ResourceManager::new(
                vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()],
            ))
            .unwrap(),
        )
    });
static ENVIRONMENT: LazyLock<Arc<crate::level::environment::EnvironmentDefinitions>> =
    LazyLock::new(|| {
        Arc::new(
            crate::level::environment::EnvironmentDefinitions::load(&ResourceManager::new(
                Vec::new(),
            ))
            .unwrap(),
        )
    });
static TAGS: LazyLock<Arc<TagSet<Block>>> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla tags");
    Arc::new(
        TagSet::load(&ResourceManager::new(vec![
            Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
        ]))
        .unwrap(),
    )
});
fn level(unloaded_corner: bool) -> Level {
    let mut level = Level::new(
        Region::new(&crate::Behaviors::EMPTY),
        LevelContext::fixture(0, 63),
    );
    let biome = ferraloom_chunk::BiomeId(0);
    for x in -1..=1 {
        for z in -1..=1 {
            if unloaded_corner && ((x == -1 && z <= 0) || (x == 0 && z == -1)) {
                continue;
            }
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(x, z),
                    -4..20,
                    LevelChunkSection::new(
                        ferraloom_chunk::Strategy::block_states(),
                        ferraloom_chunk::Strategy::biomes(Arc::new(
                            ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                        )),
                        biome,
                    ),
                ))
                .unwrap();
        }
    }
    level.set_block_tags(Arc::clone(&TAGS));
    environment(&mut level, "minecraft:overworld", false);
    level.rules_mut().fire_spread_radius_around_player = -1;
    level
}
fn environment(level: &mut Level, key: &str, increased_burnout: bool) {
    let dimension=ENVIRONMENT.dimension(&serde_json::json!({"attributes":{"gameplay/increased_fire_burnout":increased_burnout},"timelines":[]})).unwrap();
    level
        .set_environment_attributes(
            Arc::clone(&ENVIRONMENT),
            &dimension,
            key.parse().unwrap(),
            &[],
            crate::level::environment::ClockSnapshot::default(),
        )
        .unwrap();
}
fn raw(level: &mut Level, pos: BlockPos, block: Block) {
    level.set_block_state(pos, block.default_state()).unwrap();
}
fn writes(level: &mut Level) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::BlockUpdated { pos, new, .. } => {
                Some(format!("{},{},{}:{}", pos.x, pos.y, pos.z, new.id()))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn scheduled(level: &mut Level) -> String {
    let mut rows = Vec::new();
    for x in -1..=1 {
        for z in -1..=1 {
            let pos = ChunkPos::new(x, z);
            if !level.has_chunk_at(BlockPos::new(x * 16, 64, z * 16)) {
                continue;
            }
            let mut saved = ferraloom_storage::SerializableChunkData::default();
            level.save_chunk_ticks(pos, &mut saved).unwrap();
            for tick in saved.block_ticks.unwrap_or_default() {
                let block =
                    Block::from_name(&tick.id.as_ref().unwrap().to_string_checked().unwrap())
                        .unwrap();
                rows.push(format!(
                    "{},{},{}:{}:{}",
                    tick.x.unwrap(),
                    tick.y.unwrap(),
                    tick.z.unwrap(),
                    block.protocol_id(),
                    tick.delay.unwrap()
                ));
            }
        }
    }
    rows.sort();
    rows.join(";")
}
fn axis(value: &str) -> Axis {
    match value {
        "x" => Axis::X,
        "z" => Axis::Z,
        _ => panic!("unexpected native axis {value}"),
    }
}
fn describe(shape: PortalShape) -> String {
    format!(
        "{}:{},{},{}:{},{},{}",
        if shape.axis == Axis::X { "x" } else { "z" },
        shape.bottom_left.x,
        shape.bottom_left.y,
        shape.bottom_left.z,
        shape.width,
        shape.height,
        shape.portal_blocks
    )
}

#[test]
fn fire_spread_radius_matches_native_membership_spectators_and_exact_distance_boundaries() {
    let mut matched = 0;
    for line in RADIUS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = level(false);
        let mut rules = GameRules::default();
        rules
            .set(
                GameRule::FireSpreadRadiusAroundPlayer,
                GameRuleValue::Integer(f[1].parse().unwrap()),
            )
            .unwrap();
        rules.update_level_rules(level.rules_mut());
        level.rules_mut().spectators_generate_chunks = false;
        let at = Vec3::new(
            f64::from_bits(f[2].parse::<i64>().unwrap() as u64),
            f64::from_bits(f[3].parse::<i64>().unwrap() as u64),
            f64::from_bits(f[4].parse::<i64>().unwrap() as u64),
        );
        let player = level.next_entity_ref();
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "fire-radius".into(),
                    1,
                    at,
                    if f[6] == "true" {
                        GameType::Spectator
                    } else {
                        GameType::Survival
                    },
                ))),
            )
            .unwrap();
        if f[5] == "false" {
            level.remove_tracked_entity(player).unwrap();
        } else {
            level
                .update_player_tracking_section(player, SectionPos::from_position(at))
                .unwrap();
        }
        level.entity_mut::<ServerPlayer>(player).unwrap().game_mode = if f[7] == "true" {
            GameType::Spectator
        } else {
            GameType::Survival
        };
        assert_eq!(
            level.can_spread_fire_around(POS).unwrap(),
            f[8] == "true",
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 896);
}

#[test]
fn portal_search_and_creation_match_native_axes_frame_limits_holes_and_write_order() {
    let mut matched = 0;
    for line in PORTALS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let frame_axis = axis(f[1]);
        let width = f[2].parse::<i32>().unwrap();
        let height = f[3].parse::<i32>().unwrap();
        let pattern = f[4].parse::<i32>().unwrap();
        let mut level = level(false);
        let along = if frame_axis == Axis::X {
            Direction::East
        } else {
            Direction::South
        };
        for x in -1..=width {
            for y in -1..=height {
                if x == -1 || x == width || y == -1 || y == height {
                    raw(&mut level, POS.relative(along, x).above(y), Block::OBSIDIAN);
                }
            }
        }
        match pattern {
            1 => raw(
                &mut level,
                POS.relative(along, width / 2).below(1),
                Block::AIR,
            ),
            2 => raw(
                &mut level,
                POS.relative(along, -1).above(height / 2),
                Block::AIR,
            ),
            3 => raw(
                &mut level,
                POS.relative(along, width / 2).above(height),
                Block::AIR,
            ),
            4..=7 => raw(
                &mut level,
                POS.relative(along, width / 2).above(height / 2),
                [
                    Block::STONE,
                    Block::NETHER_PORTAL,
                    Block::FIRE,
                    Block::SOUL_FIRE,
                ][(pattern - 4) as usize],
            ),
            8 => {
                for x in [-1, width] {
                    for y in [-1, height] {
                        raw(&mut level, POS.relative(along, x).above(y), Block::AIR);
                    }
                }
            }
            _ => {}
        }
        let start = if f[5] == "true" {
            POS.relative(along, width / 2).above(height / 2)
        } else {
            POS
        };
        let preferred = axis(f[6]);
        let reader = LevelReaderRef::Server(&level);
        assert_eq!(
            describe(PortalShape::find_any(start, preferred, &reader).unwrap()),
            f[7],
            "{line}"
        );
        let empty = PortalShape::find_empty(start, preferred, &reader).unwrap();
        assert_eq!(empty.map_or_else(|| "-".into(), describe), f[8], "{line}");
        level.take_effects();
        if let Some(shape) = empty {
            shape.create_portal_blocks(&mut level).unwrap();
        }
        assert_eq!(writes(&mut level), f[9], "{line}");
        matched += 1;
    }
    assert_eq!(matched, 2592);
}

#[test]
fn lava_ignition_matches_native_walk_obstructions_waterlogging_soul_base_and_rng_tail() {
    let mut matched = 0;
    for line in LAVA.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let pattern = f[1].parse::<i32>().unwrap();
        let mut level = level(pattern == 5);
        raw(&mut level, POS, Block::LAVA);
        raw(&mut level, POS.below(1), Block::STONE);
        for x in -2..=2 {
            for z in -2..=2 {
                let at = POS.offset(x, 0, z);
                if pattern == 1 || pattern == 2 || pattern == 4 {
                    let material = if pattern == 2 {
                        Block::OAK_FENCE
                            .default_state()
                            .with(p::WATERLOGGED, true)
                            .unwrap()
                    } else {
                        Block::OAK_PLANKS.default_state()
                    };
                    level.set_block_state(at, material).unwrap();
                }
                if pattern == 3 {
                    raw(&mut level, at.above(1), Block::STONE);
                }
                if pattern == 4 {
                    raw(&mut level, at.below(1), Block::SOUL_SOIL);
                }
            }
        }
        raw(&mut level, POS, Block::LAVA);
        let origin = if pattern == 5 {
            raw(&mut level, POS, Block::AIR);
            raw(&mut level, POS.below(1), Block::AIR);
            let at = BlockPos::new(0, 64, 0);
            raw(&mut level, at, Block::LAVA);
            raw(&mut level, at.below(1), Block::STONE);
            at
        } else {
            POS
        };
        level.rules_mut().fire_spread_radius_around_player = f[2].parse().unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        level.take_effects();
        FlowingFluid::Lava
            .random_tick(&mut level, origin, Block::LAVA.default_state().data().fluid)
            .unwrap();
        assert_eq!(writes(&mut level), f[4], "{line}");
        assert_eq!(scheduled(&mut level), f[5], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[6].parse::<i64>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 576);
}

#[test]
fn fire_ticks_match_native_aging_burnout_neighbor_order_scheduling_and_rng_tail() {
    compare_ticks(&TICKS, false);
}
#[test]
fn fire_rain_extinguishing_and_roofs_match_native_weather_queries_and_rng_tail() {
    compare_ticks(&RAIN, true);
}
fn compare_ticks(vectors: &str, rainy: bool) {
    let mut matched = 0;
    for line in vectors.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = level(false);
        let state = Block::FIRE
            .default_state()
            .with(p::AGE, f[2].parse::<u8>().unwrap())
            .unwrap();
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_state(
                POS.below(1),
                BlockState::from_id(f[1].parse().unwrap()).unwrap(),
            )
            .unwrap();
        if f[3] == "true" {
            for side in Direction::VALUES {
                if side != Direction::Down {
                    raw(&mut level, POS.relative(side, 1), Block::OAK_PLANKS);
                }
            }
        }
        level.rules_mut().fire_spread_radius_around_player = f[4].parse().unwrap();
        environment(&mut level, "minecraft:overworld", f[5] == "true");
        if rainy {
            level.set_has_skylight(true);
            level.set_weather_environment(crate::level::weather::WeatherEnvironment {
                temperatures: Arc::clone(&TEMPERATURES),
                biome_names: Arc::new(vec!["minecraft:plains".parse().unwrap()]),
                biome_zoom_seed: 0,
                sea_level: 63,
            });
            level.load_weather(crate::level::weather::WeatherData {
                raining: true,
                ..Default::default()
            });
            for x in -1..=1 {
                for z in -1..=1 {
                    let pos = ChunkPos::new(x, z);
                    level.set_light_snapshot(
                        pos,
                        crate::level::weather::ChunkLightSnapshot::default(),
                    );
                    level.prime_chunk_heightmaps(pos).unwrap();
                }
            }
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[6].parse().unwrap())),
            0,
        );
        level.take_effects();
        fire::FIRE.tick(state, POS, &mut level).unwrap();
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[7].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(writes(&mut level), f[8], "{line}");
        assert_eq!(scheduled(&mut level), f[9], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[10].parse::<i64>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, if rainy { 960 } else { 1920 });
}

#[test]
fn fire_on_place_matches_native_dimension_gate_same_block_guard_portals_and_tick_draws() {
    let mut matched = 0;
    for line in PLACEMENT.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = level(false);
        environment(&mut level, f[1], false);
        let state = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let old = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_state(
                POS.below(1),
                BlockState::from_id(f[4].parse().unwrap()).unwrap(),
            )
            .unwrap();
        let frame = f[5].parse::<u8>().unwrap();
        if frame != 0 {
            let along = if frame == 1 {
                Direction::East
            } else {
                Direction::South
            };
            for x in -1..=2 {
                for y in -1..=3 {
                    if x == -1 || x == 2 || y == -1 || y == 3 {
                        raw(&mut level, POS.relative(along, x).above(y), Block::OBSIDIAN);
                    }
                }
            }
        }
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[7].parse().unwrap())),
            0,
        );
        level.take_effects();
        level
            .required_block_behavior(state.block(), BlockMethod::OnPlace)
            .unwrap()
            .on_place(state, POS, old, f[6] == "true", &mut level)
            .unwrap();
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[8].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(writes(&mut level), f[9], "{line}");
        assert_eq!(scheduled(&mut level), f[10], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[11].parse::<i64>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 864);
}

#[test]
fn fire_contact_queues_native_freeze_clear_ignition_and_distinct_damage_without_applying_early() {
    use crate::entity::{
        EntityData,
        inside_block::{InsideBlockAction, InsideBlockEffectType},
        synched_data,
    };
    let mut level = level(false);
    let entity: EntityRef = level.next_entity_ref();
    let receiver =
        crate::entity::item::PrimedTnt::new_at(&mut level, Vec3::new(8.5, 64.0, 8.5), None)
            .unwrap();
    level
        .add_entity(entity, EntityState::PrimedTnt(Box::new(receiver)))
        .unwrap();
    let mut matched = 0;
    for line in CONTACTS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.entity_data.set(
            synched_data::TICKS_FROZEN,
            ferraloom_protocol::codec::VarInt(40),
        );
        base.remaining_fire_ticks = -1;
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
        level
            .required_block_behavior(state.block(), BlockMethod::EntityInside)
            .unwrap()
            .entity_inside(state, POS, entity, &mut level)
            .unwrap();
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        assert_eq!(
            (base.ticks_frozen(), base.remaining_fire_ticks),
            (40, -1),
            "{line}"
        );
        let actions = base
            .inside_effect_collector
            .drain()
            .into_iter()
            .map(|action| match action {
                InsideBlockAction::Effect(InsideBlockEffectType::ClearFreeze) => {
                    "apply:CLEAR_FREEZE".into()
                }
                InsideBlockAction::Effect(InsideBlockEffectType::FireIgnite) => {
                    "apply:FIRE_IGNITE".into()
                }
                InsideBlockAction::InFireHurt(damage) => {
                    format!("after:FIRE_IGNITE:minecraft:in_fire:{}", damage.to_bits())
                }
                other => panic!("unexpected native fire action {other:?}"),
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actions, f[3], "{line}");
        assert_eq!(
            level.random().next_long(),
            f[4].parse::<i64>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 1026);
}
