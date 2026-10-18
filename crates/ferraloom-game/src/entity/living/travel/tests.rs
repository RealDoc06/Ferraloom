use super::*;
use crate::entity::{EntityData, player::ServerPlayer};
use ferraloom_util::Uuid;

fn float(bits: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(bits, 16).unwrap())
}
fn double(bits: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
fn vector(value: &str) -> Vec3 {
    let mut parts = value.split(',').map(double);
    Vec3::new(
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    )
}
fn bits(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
fn rows(prefix: &str) -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(move |line| line.starts_with(prefix))
        .map(|line| line.split('\t').collect())
}

#[test]
fn input_acceleration_and_look_vector_match_native_float_and_double_bits() {
    let mut count = 0;
    for row in rows("travel-input\t") {
        let mut base = EntityData::new(ferraloom_data::EntityType::PLAYER, 1, 0);
        base.yaw = float(row[3]);
        base.pitch = float(row[4]);
        base.velocity = vector(row[6]);
        base.move_relative(float(row[2]), vector(row[5]));
        assert_eq!(bits(base.velocity), row[7], "input case {}", row[1]);
        assert_eq!(bits(base.look_angle()), row[8], "look case {}", row[1]);
        assert_eq!(base.needs_sync.to_string(), row[9]);
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn ground_and_air_friction_preserve_native_float_threshold_and_products() {
    let mut count = 0;
    for row in rows("travel-friction\t") {
        let speed = friction_influenced_speed(
            float(row[2]),
            float(row[3]),
            row[4].parse().unwrap(),
            float(row[5]),
        );
        assert_eq!(format!("{:x}", speed.to_bits()), row[6], "case {}", row[1]);
        assert_eq!(
            format!(
                "{:x}",
                compute_modified_friction(float(row[7]), float(row[8])).to_bits()
            ),
            row[9],
            "modified case {}",
            row[1]
        );
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn fluid_falling_adjustment_uses_captured_falling_state_and_live_sprint_flag() {
    let mut player = ServerPlayer::new(
        Uuid::new(3, 4),
        "travel".into(),
        1,
        Vec3::ZERO,
        crate::entity::player::GameType::Survival,
    );
    let mut count = 0;
    for row in rows("travel-fluid-drag\t") {
        player.set_shared_flag(3, row[4].parse().unwrap());
        assert_eq!(
            bits(player.fluid_falling_adjusted_movement(
                double(row[2]),
                row[3].parse().unwrap(),
                vector(row[5])
            )),
            row[6],
            "case {}",
            row[1]
        );
        count += 1;
    }
    assert_eq!(count, 9);
}
