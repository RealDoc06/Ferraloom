use crate::{
    block::{LevelReaderRef, containers::wall_hanging_sign_can_place},
    level::LevelError,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::io::Read;

#[test]
fn native_wall_hanging_sign_can_place_masks_and_short_circuit_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/interaction-helpers.jsonl.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, _) = super::tests::level();
    let pos = BlockPos::new(8, 5, 8);
    let mut checked = 0;
    let mut read_counts = [0; 3];
    for row in vectors.lines() {
        let case: serde_json::Value = serde_json::from_str(row).unwrap();
        if case["kind"] != "sign" {
            continue;
        }
        let state = BlockState::from_id(case["state"].as_u64().unwrap() as u16).unwrap();
        let facing = super::direction(state.get(ferraloom_data::properties::FACING).unwrap());
        for (side, field) in [
            (facing.clockwise(), "left"),
            (facing.counter_clockwise(), "right"),
        ] {
            level
                .set_block_state(
                    pos.relative(side, 1),
                    BlockState::from_id(case[field].as_u64().unwrap() as u16).unwrap(),
                )
                .unwrap();
        }
        assert_eq!(
            wall_hanging_sign_can_place(state, &LevelReaderRef::Server(&level), pos).unwrap(),
            case["result"].as_bool().unwrap(),
            "{row}",
        );
        read_counts[case["reads"].as_u64().unwrap() as usize] += 1;
        checked += 1;
    }
    assert_eq!(checked, 3744);
    assert!(read_counts[1] > 0 && read_counts[2] > 0);

    // The clockwise support accepts before the absent counterclockwise chunk
    // is queried. When that support fails, the second query must stay required.
    let edge = BlockPos::new(0, 5, 8);
    let state = Block::OAK_WALL_HANGING_SIGN.default_state();
    assert_eq!(
        super::direction(state.get(ferraloom_data::properties::FACING).unwrap()),
        Direction::North
    );
    level
        .set_block_state(edge.east(1), Block::STONE.default_state())
        .unwrap();
    assert!(wall_hanging_sign_can_place(state, &LevelReaderRef::Server(&level), edge).unwrap());
    level
        .set_block_state(edge.east(1), Block::AIR.default_state())
        .unwrap();
    assert_eq!(
        wall_hanging_sign_can_place(state, &LevelReaderRef::Server(&level), edge),
        Err(LevelError::MissingChunk(ChunkPos::new(-1, 0))),
    );
}
