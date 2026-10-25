//! Diagnostic replay consumed by trace_piston_phases.py. Native observations
//! are captured independently on devbox and are never committed as fixtures.
use super::{moving_piston::PistonMovingBlockEntity, state, tests};
use crate::{block::entity::BlockEntityState, level::Level};
use ferraloom_data::Block;
use ferraloom_util::{BlockPos, Direction};

const POS: BlockPos = BlockPos::new(8, 64, 8);
static EMPTY: crate::Behaviors = crate::Behaviors::EMPTY;

fn cell(level: &Level, pos: BlockPos) -> String {
    let state = level.get_block_state(pos).unwrap();
    let moving = match level.block_entity_ref(pos).unwrap() {
        Some(handle) => match level.block_entity_state(handle).unwrap() {
            BlockEntityState::MovingPiston(_) => {
                let entity = level
                    .block_entity::<PistonMovingBlockEntity>(handle)
                    .unwrap();
                format!(
                    "{},{},{},{},{}",
                    entity.get_progress(0.0).to_bits(),
                    entity.get_progress(1.0).to_bits(),
                    entity.last_ticked,
                    entity.extending,
                    entity.is_source_piston,
                )
            }
            _ => "-".to_owned(),
        },
        None => "-".to_owned(),
    };
    format!("{}:{moving}", state.id())
}
fn snapshot(level: &Level, sticky: bool, off: i64, phase: &str) {
    println!(
        "PHASE\t{sticky}\t{off}\t{}\t{phase}\t{}\t{}\t{}",
        level.game_time(),
        cell(level, POS),
        cell(level, POS.east(1)),
        cell(level, POS.east(2)),
    );
}

/// This test only emits a replay. Its observations must be compared with the
/// fresh Java trace; a successful cargo invocation alone establishes no parity.
#[test]
#[ignore = "diagnostic replay: compare output with trace_piston_phases.py --compare-rust"]
fn replay_canonical_piston_tick_phases_for_native_comparison() {
    for sticky in [false, true] {
        for off in [0, 1, 2, 4] {
            let mut level = tests::level_with_behaviors(1, &EMPTY);
            let piston = state::with_facing(
                if sticky {
                    Block::STICKY_PISTON
                } else {
                    Block::PISTON
                }
                .default_state(),
                Direction::East,
            );
            level.set_block_state(POS, piston).unwrap();
            level
                .set_block_state(POS.east(1), Block::STONE.default_state())
                .unwrap();
            level
                .set_block_and_update(POS.west(1), Block::REDSTONE_BLOCK.default_state())
                .unwrap();
            snapshot(&level, sticky, off, "command-on");
            if off == 0 {
                level
                    .set_block_and_update(POS.west(1), Block::AIR.default_state())
                    .unwrap();
                snapshot(&level, sticky, off, "command-off");
            }
            for tick in 1..=8 {
                level.set_game_time(tick);
                level.set_handling_tick(true);
                level.tick_block_phase(tick, 65536).unwrap();
                snapshot(&level, sticky, off, "chunk-broadcast");
                level.run_block_events().unwrap();
                snapshot(&level, sticky, off, "block-events");
                level.set_handling_tick(false);
                level.tick_block_entities(true).unwrap();
                snapshot(&level, sticky, off, "block-entities");
                level.tick_block_entities(false).unwrap();
                snapshot(&level, sticky, off, "frozen-broadcast");
                if tick == off {
                    level
                        .set_block_and_update(POS.west(1), Block::AIR.default_state())
                        .unwrap();
                    snapshot(&level, sticky, off, "command-off");
                }
            }
        }
    }
}
