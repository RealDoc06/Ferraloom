use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::Block;
use ferraloom_util::ChunkPos;
use std::{cell::Cell, sync::Arc};

static EMPTY: Behaviors = Behaviors::EMPTY;
const VECTORS: &str = include_str!("../../../tests/java/level/block-scans.tsv");

fn numbers(s: &str) -> Vec<i32> {
    s.split(',').map(|s| s.parse().unwrap()).collect()
}
fn positions(s: &str) -> Vec<BlockPos> {
    s.split(';')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let xyz = numbers(p);
            BlockPos::new(xyz[0], xyz[1], xyz[2])
        })
        .collect()
}
fn level(loaded: &str, states: &str) -> Level {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.region_mut().min_y = 0;
    level.region_mut().max_y = 32;
    for key in loaded.split(';').filter(|key| !key.is_empty()) {
        let xz = numbers(key);
        let pos = ChunkPos::new(xz[0], xz[1]);
        let mut chunk = LevelChunk::new(
            pos,
            0..2,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        );
        for state in states.split(';').filter(|state| !state.is_empty()) {
            let xyz_id = numbers(state);
            let p = BlockPos::new(xyz_id[0], xyz_id[1], xyz_id[2]);
            if ChunkPos::containing(p) == pos {
                chunk
                    .set_block_state(p, BlockState::from_id(xyz_id[3] as u16).unwrap())
                    .unwrap();
            }
        }
        level.add_chunk(chunk).unwrap();
    }
    level
}
fn predicate(name: &str, state: BlockState) -> bool {
    match name {
        "fluid" => !state.data().fluid.empty,
        "air" => state.is_air(),
        "solid" => state.block() == Block::STONE,
        "none" => false,
        "all" => true,
        _ => panic!("unknown filter {name}"),
    }
}
fn format_positions(positions: &[BlockPos]) -> String {
    positions
        .iter()
        .map(|p| format!("{},{},{}", p.x, p.y, p.z))
        .collect::<Vec<_>>()
        .join(";")
}
fn result(value: Result<String, LevelError>) -> String {
    match value {
        Ok(value) => value,
        Err(LevelError::MissingChunk(pos)) => format!("missing:{},{}", pos.x, pos.z),
        Err(error) => panic!("unexpected query error {error:?}"),
    }
}
fn collect(matcher: &impl BlockMatcher) -> Result<String, LevelError> {
    let mut found = Vec::new();
    matcher.for_each(|pos, _| {
        found.push(pos);
        Ok(())
    })?;
    Ok(format_positions(&found))
}

#[test]
fn native_box_bounds_scan_order_waterlogging_and_required_reads() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if !matches!(f[0], "liquid" | "box") {
            continue;
        }
        let coords: Vec<f64> = f[4].split(',').map(|s| s.parse().unwrap()).collect();
        let bb = Aabb::new(
            coords[0], coords[1], coords[2], coords[3], coords[4], coords[5],
        );
        for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
            let mut level = level(f[2], f[3]);
            level.set_unported_behavior(policy);
            let actual = if f[0] == "liquid" {
                level.contains_any_liquid(bb).map(|value| value.to_string())
            } else {
                collect(
                    &level
                        .find_blocks_in(bb)
                        .filter_state(|state| predicate(f[5], state)),
                )
            };
            assert_eq!(result(actual), f[6], "{line}, {policy:?}");
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
        cases += 1;
    }
    assert_eq!(cases, 18);
}

#[test]
fn native_ordered_cache_preserves_duplicates_filters_and_outside_air() {
    let mut cases = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "ordered" {
            continue;
        }
        let level = level(f[2], f[3]);
        let ps = positions(f[4]);
        let matcher = level
            .find_blocks_at(ps)
            .filter_pos(|p| !f[5].ends_with("-pos") || p.x < 16)
            .filter_state(|state| predicate(f[5].trim_end_matches("-pos"), state));
        assert_eq!(result(collect(&matcher)), f[6], "{line}");
        cases += 1;
    }
    assert_eq!(cases, 6);
}

#[test]
fn native_matcher_queries_counts_inversion_and_first_match() {
    let water = format!("0,0,0,{}", Block::WATER.default_state().id());
    let level = level("0,0", &water);
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "queries" => {
                let matcher = level
                    .find_blocks_between(BlockPos::ZERO, BlockPos::new(1, 0, 0))
                    .filter_state(|state| predicate(f[1], state));
                let mut actual = vec![
                    matcher.any_matched().unwrap(),
                    matcher.none_matched().unwrap(),
                    matcher.all_matched().unwrap(),
                ];
                for n in [-1, 0, 1, 2, i32::MAX] {
                    actual.extend([
                        matcher.at_least_matched(n).unwrap(),
                        matcher.at_most_matched(n).unwrap(),
                    ]);
                }
                assert_eq!(
                    actual
                        .iter()
                        .map(bool::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    f[2],
                    "{line}"
                );
            }
            "first" => {
                let matcher = level
                    .find_blocks_at([BlockPos::new(1, 0, 0), BlockPos::ZERO, BlockPos::ZERO])
                    .filter_state(|state| !state.data().fluid.empty);
                assert_eq!(
                    format_positions(&[matcher.find_first().unwrap().unwrap()]),
                    f[1]
                );
                assert_eq!(
                    format_positions(&[matcher
                        .find_first_where(|p, _| p.x == 0)
                        .unwrap()
                        .unwrap()]),
                    f[2]
                );
                assert_eq!(matcher.find_first_where(|p, _| p.x == 1).unwrap(), None);
            }
            "reversed" => {
                assert_eq!(
                    collect(&level.find_blocks_between(BlockPos::new(1, 1, 1), BlockPos::ZERO))
                        .unwrap(),
                    f[1]
                );
            }
            _ => (),
        }
    }
    let calls = Cell::new(0);
    assert!(
        !level
            .find_blocks_in(Aabb::new(0., 0., 0., 1., 0., 0.))
            .filter_state(|_| false)
            .filter_state(|_| {
                calls.set(calls.get() + 1);
                true
            })
            .any_matched()
            .unwrap()
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn native_palette_pruning_and_eight_slot_rejection_cache() {
    let water = format!("0,0,0,{}", Block::WATER.default_state().id());
    let level = level("0,0;1,0;2,0", &water);
    let ps = [
        BlockPos::ZERO,
        BlockPos::new(1, 0, 0),
        BlockPos::new(16, 0, 0),
        BlockPos::new(0, 16, 0),
        BlockPos::new(32, 0, 0),
        BlockPos::ZERO,
    ];
    for line in VECTORS.lines().filter(|line| line.starts_with("prune\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let calls = Cell::new(0);
        let reject = |_| {
            calls.set(calls.get() + 1);
            false
        };
        if f[1] == "true" {
            assert!(
                !level
                    .find_blocks_between(BlockPos::ZERO, BlockPos::new(32, 16, 0))
                    .filter_state(reject)
                    .any_matched()
                    .unwrap()
            );
        } else {
            assert!(
                !level
                    .find_blocks_at(ps)
                    .filter_state(reject)
                    .any_matched()
                    .unwrap()
            );
        }
        assert_eq!(calls.get(), f[2].parse::<i32>().unwrap(), "{line}");
    }
}

#[test]
fn native_manhattan_iterators_and_loaded_block_stream() {
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "manhattan" => {
                let level = level("0,0", "");
                let matcher = if f[1] == "true" {
                    level.find_blocks_in_box_by_manhattan_distance(BlockPos::ZERO, 1, 1)
                } else {
                    level.find_blocks_in_manhattan(BlockPos::ZERO, 1)
                };
                let matcher = matcher.filter_pos(|p| p.x >= 0 && p.z >= 0);
                assert_eq!(collect(&matcher).unwrap(), f[2], "{line}");
            }
            "if-loaded" => {
                let water = format!("15,0,0,{}", Block::WATER.default_state().id());
                let level = level(f[1], &water);
                let actual = level
                    .get_block_states_if_loaded(Aabb::new(15., 0., 0., 16., 1., 0.))
                    .map(|state| state.unwrap().id().to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                assert_eq!(actual, f[2], "{line}");
                assert_eq!(
                    level
                        .get_block_states_if_loaded(Aabb::new(16., 32., 0., 17., 33., 0.))
                        .count(),
                    0
                );
            }
            _ => (),
        }
    }
}

#[test]
fn abort_and_callback_errors_stop_before_a_required_missing_neighbor() {
    let water = format!("15,0,0,{}", Block::WATER.default_state().id());
    let level = level("0,0", &water);
    let bb = Aabb::new(15., 0., 0., 16., 0., 0.);
    assert_eq!(level.contains_any_liquid(bb), Ok(true));
    let matcher = level
        .find_blocks_in(bb)
        .filter_state(|state| !state.data().fluid.empty);
    assert_eq!(
        matcher.for_each_until(|_, _| Ok(Continuation::Abort)),
        Ok(true)
    );
    assert_eq!(
        matcher.for_each_until(|_, _| Err(LevelError::UnsupportedWeatherQuery)),
        Err(LevelError::UnsupportedWeatherQuery)
    );
    let dry = level.find_blocks_in(bb).filter_state(|_| false);
    assert_eq!(
        dry.any_matched(),
        Err(LevelError::MissingChunk(ChunkPos::new(1, 0)))
    );
}
