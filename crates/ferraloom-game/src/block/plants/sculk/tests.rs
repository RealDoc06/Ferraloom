use super::*;
use crate::{
    Behaviors,
    level::{Level, LevelContext, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::{
    datapack::{TagSet, load_tags},
    registries::fluid,
};
use ferraloom_protocol::play::clientbound;
use ferraloom_util::{
    ChunkPos,
    random::{LegacyRandomSource, Random, WorldgenRandom, XoroshiroRandomSource},
};
use ferraloom_worldgen::{
    feature::FeatureRandom,
    region::{ChunkStatus, RegionChunk, WorldGenRegion},
};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, sync::Arc};

static EMPTY: Behaviors = Behaviors::EMPTY;
fn point(v: &Value) -> BlockPos {
    BlockPos::new(
        v[0].as_i64().unwrap() as i32,
        v[1].as_i64().unwrap() as i32,
        v[2].as_i64().unwrap() as i32,
    )
}
fn case_label(case: &Value) -> String {
    format!(
        "{} mode={} seed={} canonical={} distance={} charge={}",
        case["kind"],
        case["mode"],
        case["seed"],
        case["canonical"],
        case["distance"],
        case["charge"]
    )
}
fn cursor_json(spreader: &SculkSpreader) -> Value {
    serde_json::to_value(
        spreader
            .cursors()
            .iter()
            .map(|c| {
                serde_json::json!([
                    c.pos.x,
                    c.pos.y,
                    c.pos.z,
                    c.charge,
                    c.decay_delay,
                    c.update_delay,
                    c.facings
                ])
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn initial(origin: BlockPos, mode: i64, server: bool) -> BTreeMap<[i32; 3], BlockState> {
    let mut states = BTreeMap::new();
    let position = |x, y, z| [origin.x + x, origin.y + y, origin.z + z];
    for x in -16..=16 {
        for z in -16..=16 {
            for y in -3..=-1 {
                states.insert(position(x, y, z), Block::STONE.default_state());
            }
        }
    }
    if mode == 1 {
        for x in -16..=16 {
            for z in -16..=16 {
                for y in 0..=1 {
                    states.insert(position(x, y, z), Block::WATER.default_state());
                }
            }
        }
    }
    if server || mode == 2 || mode == 3 {
        for x in -16..=16 {
            for z in -16..=16 {
                states.insert(position(x, 1, z), Block::STONE.default_state());
            }
        }
    }
    if mode == 4 {
        for z in -2..=2 {
            for y in 0..=2 {
                states.insert(position(-1, y, z), Block::STONE.default_state());
            }
        }
    }
    if matches!(mode, 2..=4) {
        for x in -2..=2 {
            for z in -2..=2 {
                states.insert(position(x, 0, z), Block::SCULK.default_state());
            }
        }
    }
    if mode == 7 {
        for x in 4..=8 {
            for z in -2..=2 {
                states.insert(position(x, 0, z), Block::SCULK.default_state());
            }
        }
    }
    states
}
fn changed(
    level: &LevelReaderRef<'_, '_>,
    origin: BlockPos,
    before: &BTreeMap<[i32; 3], BlockState>,
) -> Value {
    let mut cells = Vec::new();
    for x in -16..=16 {
        for y in -5..=5 {
            for z in -16..=16 {
                let pos = origin.offset(x, y, z);
                let state = level.get_block_state(pos).unwrap();
                if state
                    != before
                        .get(&[pos.x, pos.y, pos.z])
                        .copied()
                        .unwrap_or(Block::AIR.default_state())
                {
                    cells.push(serde_json::json!([pos.x, pos.y, pos.z, state.id()]));
                }
            }
        }
    }
    Value::Array(cells)
}
fn random(case: &Value) -> Random {
    let seed = if case["canonical"] == true {
        case["seed"].as_i64().unwrap()
    } else {
        123
    };
    match case["level_random"].as_str().unwrap() {
        "XoroshiroRandomSource" => Random::Xoroshiro(XoroshiroRandomSource::new(seed)),
        "LegacyRandomSource" => Random::Legacy(LegacyRandomSource::new(seed)),
        other => panic!("unclassified native random {other}"),
    }
}
fn run(case: &Value, level: &mut LevelAccessRef<'_, '_>) {
    let label = case_label(case);
    let origin = point(&case["origin"]);
    let mut external = WorldgenRandom::new(Random::Legacy(LegacyRandomSource::new(
        case["seed"].as_i64().unwrap(),
    )));
    if case["kind"] == "patch" {
        assert_eq!(case["success"], true);
        let LevelAccessRef::Worldgen(region) = level else {
            unreachable!()
        };
        region
            .sculk_spread(
                [origin.x, origin.y, origin.z],
                &mut external,
                case["charges"].as_i64().unwrap() as i32,
                case["amount"].as_i64().unwrap() as i32,
                case["attempts"].as_i64().unwrap() as i32,
                case["growth"].as_i64().unwrap() as i32,
                case["rounds"].as_i64().unwrap() as i32,
            )
            .unwrap();
    } else {
        let mut selector = if case["canonical"] == true {
            FeatureRandomSelector::Canonical
        } else {
            FeatureRandomSelector::External(FeatureRandom::Region(&mut external))
        };
        let mut spreader = if case["kind"] == "region-worldgen" {
            SculkSpreader::create_world_gen_spreader()
        } else {
            SculkSpreader::create_level_spreader()
        };
        spreader.cursors = case["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| ChargeCursor {
                pos: point(c),
                charge: c[3].as_i64().unwrap() as i32,
                decay_delay: c[4].as_i64().unwrap() as i32,
                update_delay: c[5].as_i64().unwrap() as i32,
                facings: c[6].as_u64().map(|n| n as u8),
            })
            .collect();
        for _ in 0..case["calls"].as_u64().unwrap() {
            spreader
                .update_cursors(
                    level,
                    origin,
                    &mut selector,
                    case["spread"].as_bool().unwrap(),
                )
                .unwrap_or_else(|e| panic!("{label}: {e:?}"));
        }
        assert_eq!(cursor_json(&spreader), case["cursors"], "cursor {label}");
    }
    let next = if case["canonical"] == true {
        level.random().unwrap().next_long()
    } else {
        external.next_long()
    };
    assert_eq!(next, case["next"].as_i64().unwrap(), "RNG {label}");
    assert_eq!(
        level.random().unwrap().next_long(),
        case["level_next"].as_i64().unwrap(),
        "level RNG {label}"
    );
}

#[test]
fn native_worldgen_and_server_cursors_blocks_events_and_rng_match() {
    let bytes = std::env::var_os("FERRALOOM_SCULK_ORACLE").map(|path| std::fs::read(path).unwrap());
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes.as_deref().unwrap_or(include_bytes!(
        "../../../../tests/java/blocks/plants/sculk.jsonl.gz"
    )))
    .read_to_string(&mut text)
    .unwrap();
    let (resources, generator) = super::super::worldgen_tests::setup();
    let fluid_tags = Arc::new(
        TagSet::<fluid::Id>::from_tags(
            load_tags(&resources, "fluid", |id, _| {
                fluid::from_name(&id.to_string())
            }),
            fluid::ALL.len(),
        )
        .unwrap(),
    );
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let label = case_label(&case);
        let kind = case["kind"].as_str().unwrap();
        *counts.entry(kind.to_owned()).or_insert(0_usize) += 1;
        if kind == "decay" {
            let origin = BlockPos::new(8, 100, 8);
            let mut live = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, -63));
            live.add_chunk(LevelChunk::new(
                ChunkPos::ZERO,
                -4..20,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
            live.set_block_state(origin, Block::SCULK.default_state())
                .unwrap();
            live.set_block_state(origin.above(1), Block::STONE.default_state())
                .unwrap();
            let spreader = if case["generation"] == true {
                SculkSpreader::create_world_gen_spreader()
            } else {
                SculkSpreader::create_level_spreader()
            };
            let cursor = ChargeCursor::new(origin, case["charge"].as_i64().unwrap() as i32);
            let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
            let actual = SculkBehaviour::Sculk
                .attempt_use_charge(
                    &cursor,
                    &mut LevelAccessRef::Server(&mut live),
                    origin.east(case["distance"].as_i64().unwrap() as i32),
                    &mut FeatureRandomSelector::External(FeatureRandom::Live(&mut random)),
                    spreader.settings,
                    true,
                )
                .unwrap();
            assert_eq!(
                actual,
                case["remaining"].as_i64().unwrap() as i32,
                "decay {label}"
            );
            assert_eq!(
                random.next_long(),
                case["next"].as_i64().unwrap(),
                "decay RNG {label}"
            );
            continue;
        }
        if kind == "add" {
            let mut s = SculkSpreader::create_world_gen_spreader();
            s.add_cursors(
                BlockPos::new(8, 100, 8),
                case["amount"].as_i64().unwrap() as i32,
            );
            assert_eq!(cursor_json(&s), case["cursors"]);
            continue;
        }
        let origin = point(&case["origin"]);
        let mode = case["mode"].as_i64().unwrap();
        let server = kind == "server";
        let before = initial(origin, mode, server);
        let center = ChunkPos::containing(origin);
        if server {
            let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, -63));
            level.set_has_skylight(true);
            level.set_block_tags(generator.data.block_tags.clone());
            level.set_fluid_tags(fluid_tags.clone());
            level.rules_mut().block_drops = Some(false);
            super::super::fixtures::install_loot(&mut level);
            for x in center.x - 1..=center.x + 1 {
                for z in center.z - 1..=center.z + 1 {
                    let p = ChunkPos::new(x, z);
                    level
                        .add_chunk(LevelChunk::new(
                            p,
                            -4..20,
                            ferraloom_chunk::LevelChunkSection::new(
                                ferraloom_chunk::Strategy::block_states(),
                                ferraloom_chunk::Strategy::biomes(Arc::new(
                                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)])
                                        .unwrap(),
                                )),
                                ferraloom_chunk::BiomeId(0),
                            ),
                        ))
                        .unwrap();
                    level
                        .set_chunk_block_ticking(p, case["tickable"].as_bool().unwrap())
                        .unwrap();
                }
            }
            for (&[x, y, z], &state) in &before {
                level
                    .set_block_state(BlockPos::new(x, y, z), state)
                    .unwrap();
            }
            level.set_random_streams(random(&case), 0);
            run(&case, &mut LevelAccessRef::Server(&mut level));
            assert_eq!(
                changed(&LevelReaderRef::Server(&level), origin, &before),
                case["blocks"],
                "blocks {label}"
            );
            let events = level
                .take_effects()
                .into_iter()
                .filter_map(|effect| {
                    if let LevelEffect::PlayerPacket { packet, .. } = effect
                        && let clientbound::Packet::LevelEvent(event) = *packet
                        && event.event_type == 3006
                    {
                        Some(format!(
                            "{}:{}:{}:{}",
                            event.pos.x - origin.x,
                            event.pos.y - origin.y,
                            event.pos.z - origin.z,
                            event.data
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::to_value(events).unwrap(),
                case["events"],
                "events {label}"
            );
        } else {
            let mut chunks = Vec::new();
            for x in center.x - 1..=center.x + 1 {
                for z in center.z - 1..=center.z + 1 {
                    let mut chunk = generator.new_chunk([x, z]);
                    for (&[px, py, pz], &state) in &before {
                        if px >> 4 == x && pz >> 4 == z {
                            chunk.set_block((px & 15) as u8, py, (pz & 15) as u8, state);
                        }
                    }
                    chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                }
            }
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [center.x, center.z],
                &[ChunkStatus::Terrain; 2],
                1,
                None,
                &crate::block::GameWorldgenBlockBehavior,
                0,
                -63,
            )
            .unwrap();
            region.set_has_skylight(true);
            region.set_block_tags(generator.data.block_tags.clone());
            region.set_fluid_tags(fluid_tags.clone());
            region.set_random(random(&case));
            run(&case, &mut LevelAccessRef::Worldgen(&mut region));
            assert_eq!(
                changed(&LevelReaderRef::Worldgen(&region), origin, &before),
                case["blocks"],
                "blocks {label}"
            );
            drop(region);
            let mut post = chunks
                .iter()
                .flat_map(|c| {
                    c.chunk
                        .post_processing
                        .iter()
                        .map(|p| format!("{}:{}:{}", p[0], p[1], p[2]))
                })
                .collect::<Vec<_>>();
            post.sort();
            assert_eq!(
                serde_json::to_value(post).unwrap(),
                case["post"],
                "post-processing {label}"
            );
            let expected = case["block_entities"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let p = point(row);
                    (
                        [p.x, p.y, p.z],
                        ferraloom_nbt::parse_compound(row[3].as_str().unwrap()).unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let actual = chunks
                .iter()
                .flat_map(|c| c.block_entities.iter().map(|(p, nbt)| (*p, nbt.clone())))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(actual, expected, "protochunk block-entity NBT {label}");
        }
    }
    assert_eq!(counts.get("decay"), Some(&768));
    assert_eq!(counts.get("add"), Some(&10));
    assert_eq!(counts.get("region-worldgen"), Some(&64));
    assert_eq!(counts.get("region-level"), Some(&64));
    assert_eq!(counts.get("server"), Some(&64));
    assert_eq!(counts.get("patch"), Some(&64));
    eprintln!("Sculk native spread oracle: {counts:?}");
}
