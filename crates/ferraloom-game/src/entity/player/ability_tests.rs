use super::*;

fn rows(prefix: &str) -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(move |line| line.starts_with(prefix))
        .map(|line| line.split('\t').collect())
}
fn bits(value: f32) -> String {
    format!("{:x}", value.to_bits())
}
fn player() -> ServerPlayer {
    ServerPlayer::new(
        Uuid::new(3, 4),
        "ability".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    )
}

#[test]
fn native_flying_speed_uses_independent_ability_sprint_and_passenger_gates() {
    let mut checked = 0;
    for row in rows("player-flying-speed\t") {
        let mut player = player();
        player.set_ability_flying_speed(f32::from_bits(u32::from_str_radix(row[1], 16).unwrap()));
        player.abilities.flying = row[2].parse().unwrap();
        player.vehicle.set(
            (row[3] == "true").then_some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2)),
        );
        player.set_shared_flag(3, row[4].parse().unwrap());
        assert_eq!(bits(player.flying_movement_speed()), row[5], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 32);
}

#[test]
fn native_saved_ability_speeds_survive_mode_updates_and_malformed_fields_default() {
    let mut checked = 0;
    for row in rows("player-ability-speeds\t") {
        let mut player = player();
        player
            .try_load(ferraloom_nbt::parse_compound(row[3]).unwrap())
            .unwrap();
        assert_eq!(bits(player.ability_flying_speed()), row[4], "{row:?}");
        assert_eq!(bits(player.ability_walking_speed()), row[5], "{row:?}");
        assert_eq!(player.abilities.flying.to_string(), row[6], "{row:?}");
        let saved = player
            .save_without_id(&crate::entity::tests::persistence_level())
            .unwrap();
        let abilities = saved.get_compound("abilities").unwrap();
        assert_eq!(bits(abilities.get_float("flySpeed").unwrap()), row[4]);
        assert_eq!(bits(abilities.get_float("walkSpeed").unwrap()), row[5]);
        checked += 1;
    }
    assert_eq!(checked, 20);
}
