use super::*;
use crate::{
    Behaviors,
    block::{LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{
        Level, Region,
        chunk::{LevelChunk, LevelChunkSection},
        distance_manager::FullChunkStatus,
        effects::LevelEffect,
    },
};
use ferraloom_data::BlockState;
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random},
};
use std::sync::Arc;
const ORIGIN: BlockPos = BlockPos::new(8, 0, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY));
    for x in -1..=1 {
        for z in -1..=1 {
            let pos = ChunkPos::new(x, z);
            level
                .add_chunk(LevelChunk::new(
                    pos,
                    -4..20,
                    LevelChunkSection::new(
                        ferraloom_chunk::Strategy::block_states(),
                        ferraloom_chunk::Strategy::biomes(Arc::new(
                            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                        )),
                        ferraloom_chunk::BiomeId(0),
                    ),
                ))
                .unwrap();
            level
                .set_chunk_loading_status(pos, FullChunkStatus::BlockTicking, true)
                .unwrap();
        }
    }
    let values = include_str!("../../../tests/java/level/tags.tsv")
        .lines()
        .map(|line| {
            let (name, blocks) = line.split_once('\t').unwrap();
            (
                name.strip_prefix("tag:").unwrap().parse().unwrap(),
                blocks
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|n| Block::from_name(n).unwrap())
                    .collect(),
            )
        })
        .collect();
    level.set_block_tags(Arc::new(
        ferraloom_data::datapack::TagSet::from_tags(
            ferraloom_data::datapack::Tags {
                values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::BLOCKS.len(),
        )
        .unwrap(),
    ));
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(0)), 0);
    level
}
fn cases(prefix: &str) -> impl Iterator<Item = (&str, &str)> {
    include_str!("../../../tests/java/blocks/redstone/vectors.tsv")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(move |(name, _)| name.starts_with(prefix))
}
fn positions(positions: &[BlockPos]) -> String {
    positions
        .iter()
        .map(|p| format!("{}, {}, {}", p.x, p.y, p.z))
        .collect::<Vec<_>>()
        .join(";")
}
fn snapshot(level: &Level, positions: &[BlockPos]) -> String {
    positions
        .iter()
        .map(|p| {
            format!(
                "{}, {}, {}={}",
                p.x,
                p.y,
                p.z,
                level.get_block_state(*p).unwrap().id()
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn signal_sources_directions_and_conductor_relays_match_vanilla() {
    for (name, expected) in cases("signal:") {
        let state =
            BlockState::from_id(name.strip_prefix("signal:").unwrap().parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_state(ORIGIN, state).unwrap();
        let reader = LevelReaderRef::Server(&level);
        let mut values = vec![
            reader.is_signal_source(state).unwrap().to_string(),
            reader.own_signal(state, ORIGIN).unwrap().to_string(),
        ];
        for d in Direction::VALUES {
            values.push(format!(
                "{}:{}:{}:{}",
                reader.get_signal(ORIGIN, d).unwrap(),
                reader.get_direct_signal(ORIGIN, d).unwrap(),
                reader.get_control_input_signal(ORIGIN, d, false).unwrap(),
                reader.get_control_input_signal(ORIGIN, d, true).unwrap()
            ));
        }
        assert_eq!(values.join(";"), expected, "{name}");
    }
    for (name, expected) in cases("relay:") {
        let mut args = name.split(':').skip(1);
        let source = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let d = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let mut level = level();
        level
            .set_block_state(ORIGIN, Block::STONE.default_state())
            .unwrap();
        level
            .set_block_state(ORIGIN.relative(d, 1), source)
            .unwrap();
        assert_eq!(
            format!(
                "{}:{}:{}:{}",
                level.get_signal(ORIGIN, Direction::Down).unwrap(),
                level.get_direct_signal_to(ORIGIN).unwrap(),
                level.get_best_neighbor_signal(ORIGIN).unwrap(),
                level.has_neighbor_signal(ORIGIN).unwrap()
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn piston_limits_sticky_branches_and_collision_order_match_vanilla() {
    for (name, expected) in cases("piston-line:") {
        let mut args = name.split(':').skip(1);
        let d = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let extending = args.next().unwrap().parse().unwrap();
        let length = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        for i in 0..length {
            level
                .set_block_state(
                    ORIGIN.relative(d, i + if extending { 1 } else { 2 }),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        let mut resolver = piston::PistonStructureResolver::new(ORIGIN, d, extending);
        let ok = resolver.resolve(&level).unwrap();
        assert_eq!(
            format!(
                "{ok}|{}|{}",
                positions(resolver.to_push()),
                positions(resolver.to_destroy())
            ),
            expected,
            "{name}"
        );
    }
    for (name, expected) in cases("piston-branch:") {
        let mut args = name.split(':').skip(1);
        let sticky = if args.next().unwrap() == "slime" {
            Block::SLIME_BLOCK
        } else {
            Block::HONEY_BLOCK
        };
        let kind = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        let s = ORIGIN.east(1);
        level.set_block_state(s, sticky.default_state()).unwrap();
        let above = match kind {
            1 => Block::OBSIDIAN,
            2 => Block::REDSTONE_WIRE,
            3 => {
                if sticky == Block::SLIME_BLOCK {
                    Block::HONEY_BLOCK
                } else {
                    Block::SLIME_BLOCK
                }
            }
            _ => sticky,
        };
        level
            .set_block_state(s.above(1), above.default_state())
            .unwrap();
        for p in [s.below(1), s.north(1), s.south(1)] {
            level
                .set_block_state(p, Block::STONE.default_state())
                .unwrap();
        }
        if kind == 4 {
            for p in [s.east(1), s.above(1).east(1), s.above(1).east(2)] {
                level.set_block_state(p, sticky.default_state()).unwrap();
            }
        }
        if kind == 5 {
            for i in 0..12 {
                level
                    .set_block_state(s.south(i + 1), Block::STONE.default_state())
                    .unwrap();
            }
        }
        let mut resolver = piston::PistonStructureResolver::new(ORIGIN, Direction::East, true);
        let ok = resolver.resolve(&level).unwrap();
        assert_eq!(
            format!(
                "{ok}|{}|{}",
                positions(resolver.to_push()),
                positions(resolver.to_destroy())
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn default_and_experimental_wire_networks_match_each_vanilla_state_mutation() {
    for experimental in [false, true] {
        for shape in 0..3 {
            let mut level = level();
            level.rules_mut().redstone_experiments = experimental;
            let mut wires: Vec<_> = (0..7).map(|x| ORIGIN.east(x)).collect();
            if shape > 0 {
                for z in 1..=3 {
                    wires.push(ORIGIN.east(2).south(z));
                    wires.push(ORIGIN.east(4).north(z));
                }
            }
            if shape == 2 {
                wires.push(ORIGIN.east(3).above(1));
                level
                    .set_block_state(ORIGIN.east(3), Block::STONE.default_state())
                    .unwrap();
                wires.retain(|p| *p != ORIGIN.east(3));
            }
            for pos in &wires {
                level
                    .set_block_state(pos.below(1), Block::STONE.default_state())
                    .unwrap();
                level
                    .set_block_state(*pos, Block::REDSTONE_WIRE.default_state())
                    .unwrap();
            }
            for pos in &wires {
                let state = level.get_block_state(*pos).unwrap();
                let updated = level.update_from_neighbor_shapes(state, *pos).unwrap();
                level.set_block_state(*pos, updated).unwrap();
            }
            for change in 0..3 {
                level.take_effects();
                let block = if change == 1 {
                    Block::AIR
                } else {
                    Block::REDSTONE_BLOCK
                };
                level
                    .set_block(
                        ORIGIN.west(1),
                        block.default_state(),
                        UpdateFlags::ALL,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
                let name = format!("wire:{experimental}:{shape}:{change}");
                let expected = cases("wire:").find(|(key, _)| *key == name).unwrap().1;
                let mut parts = expected.split('|');
                assert_eq!(
                    snapshot(&level, &wires),
                    parts.next().unwrap(),
                    "{name} final state"
                );
                let changes = level
                    .take_effects()
                    .into_iter()
                    .filter_map(|e| match e {
                        LevelEffect::BlockUpdated { pos, new, .. } => {
                            Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                let expected_changes = parts
                    .next()
                    .unwrap()
                    .split(';')
                    .map(|s| s.split_once('/').unwrap().0)
                    .collect::<Vec<_>>()
                    .join(";");
                assert_eq!(changes, expected_changes, "{name} exact mutation order");
            }
        }
    }
}
