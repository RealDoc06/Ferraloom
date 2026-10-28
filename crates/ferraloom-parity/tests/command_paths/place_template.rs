//! Native resource templates use one Strict boot per backend. Seeded loot and
//! the complete client block footprint stay exact across mirrors and rotations.
use super::harness::{Case, Check, Step};

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (index, (rotation, mirror, footprint, chest, furnace)) in [
        ("none", "none", ([0, 0], [8, 10]), [6, 4], [4, 3]),
        ("clockwise_90", "none", ([-10, 0], [0, 8]), [-4, 6], [-3, 4]),
        ("180", "front_back", ([0, -10], [8, 0]), [6, -4], [4, -3]),
        (
            "counterclockwise_90",
            "left_right",
            ([-10, -8], [0, 0]),
            [-4, -6],
            [-3, -4],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 80 + index as i32 * 16;
        let mut placed = Step::feedback(
            format!(
                "place template minecraft:village/plains/houses/plains_weaponsmith_1 0 {y} 0 {rotation} {mirror} 1 42 strict"
            ),
            "commands.place.template.success",
        );
        placed.checks.push(Check::BlockSnapshot {
            min: [footprint.0[0], y, footprint.0[1]],
            max: [footprint.1[0], y + 7, footprint.1[1]],
        });
        cases.push(Case {
            name: format!("place.template.weaponsmith.{rotation}.{mirror}"),
            path: None,
            pending: None,
            steps: vec![
                placed,
                Step::feedback(
                    format!("data get block {} {} {}", chest[0], y + 1, chest[1]),
                    "commands.data.block.query",
                ),
                Step::feedback(
                    format!("data get block {} {} {}", furnace[0], y + 2, furnace[1]),
                    "commands.data.block.query",
                ),
            ],
        });
    }
    for (name, command, key) in [
        (
            "missing",
            "place template ferraloom:missing 0 80 0",
            "commands.place.template.invalid",
        ),
        (
            "out_of_world",
            "place template minecraft:fossil/skull_1 0 400 0",
            "argument.pos.outofworld",
        ),
        (
            "unloaded",
            "place template minecraft:fossil/skull_1 16000 80 16000",
            "argument.pos.unloaded",
        ),
        (
            "low_integrity",
            "place template minecraft:fossil/skull_1 0 80 0 none none -0.1",
            "argument.float.low",
        ),
        (
            "high_integrity",
            "place template minecraft:fossil/skull_1 0 80 0 none none 1.1",
            "argument.float.big",
        ),
    ] {
        cases.push(Case {
            name: format!("place.template.{name}"),
            path: None,
            pending: None,
            steps: vec![Step::feedback(command, key)],
        });
    }
    cases
}
