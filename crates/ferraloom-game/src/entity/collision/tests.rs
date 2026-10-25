use super::*;
use crate::block::UnportedBehavior;
use ferraloom_data::properties;

fn box_bits(bb: Aabb) -> String {
    [bb.min_x, bb.min_y, bb.min_z, bb.max_x, bb.max_y, bb.max_z]
        .map(|value| format!("{:x}", value.to_bits()))
        .join(",")
}

#[test]
fn native_full_block_fast_path_preserves_thin_queries_and_partial_shape_epsilon() {
    let mut checked = 0;
    for row in include_str!("../../../tests/java/entity/collision/vectors.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let state = match fields[1] {
            "minecraft:stone" => Block::STONE.default_state(),
            "minecraft:stone_slab" => Block::STONE_SLAB
                .default_state()
                .with(properties::TYPE_, properties::Type::Top)
                .unwrap(),
            "minecraft:air" => Block::AIR.default_state(),
            _ => panic!("unknown native collision fixture"),
        };
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let pos = BlockPos::new(0, 64, 0);
        level.set_block_state(pos, state).unwrap();
        let values: Vec<_> = fields[3]
            .split(',')
            .map(|value| f64::from_bits(u64::from_str_radix(value, 16).unwrap()))
            .collect();
        let query = Aabb::new(
            values[0], values[1], values[2], values[3], values[4], values[5],
        );
        assert_eq!(
            state
                .collision_shape()
                .unwrap()
                .ptr_eq(&shapes::block())
                .to_string(),
            fields[4],
            "{row}"
        );
        let hits = block_collisions(&level, None, query).unwrap();
        assert!(hits.iter().all(|(actual, _)| *actual == pos), "{row}");
        assert_eq!(hits.len().to_string(), fields[5], "{row}");
        assert_eq!(
            level.no_collision(None, query, false).unwrap().to_string(),
            fields[6],
            "{row}"
        );
        let bounds = if hits.is_empty() {
            "-".into()
        } else {
            hits.iter()
                .map(|(_, shape)| box_bits(shape.bounds()))
                .collect::<Vec<_>>()
                .join(";")
        };
        assert_eq!(bounds, fields[7], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 15);
}
