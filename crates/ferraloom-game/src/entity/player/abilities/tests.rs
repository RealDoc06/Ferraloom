use super::*;
use crate::entity::player::ServerPlayer;
use ferraloom_util::{Uuid, Vec3};
fn snapshot(a: PackedAbilities) -> String {
    format!(
        "{},{},{},{},{},{:x},{:x}",
        a.invulnerable,
        a.flying,
        a.mayfly,
        a.instabuild,
        a.may_build,
        a.flying_speed.to_bits(),
        a.walking_speed.to_bits()
    )
}
#[test]
fn native_packed_abilities_defaults_strict_errors_and_all_seven_fields_roundtrip() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("abilities-packed\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let tag = ferraloom_nbt::parse(row[2]).unwrap();
        let result = PackedAbilities::from_nbt(&tag);
        if row[3] == "error" {
            assert!(result.is_err(), "{row:?}");
        } else {
            let packed = result.unwrap();
            assert_eq!(snapshot(packed), row[3], "{row:?}");
            let encoded = packed.to_nbt().unwrap();
            assert_eq!(encoded.as_compound().unwrap().len(), 7);
            assert_eq!(
                snapshot(PackedAbilities::from_nbt(&encoded).unwrap()),
                row[3]
            );
            let native = ferraloom_nbt::parse(row[4]).unwrap();
            assert_eq!(
                snapshot(PackedAbilities::from_nbt(&native).unwrap()),
                row[3]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 14);
}
#[test]
fn native_game_type_updates_all_abilities_and_preserves_speeds_and_creative_flight() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("abilities-game-type\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let edited: bool = row[3].parse().unwrap();
        let mut a = Abilities {
            invulnerable: edited,
            flying: row[2].parse().unwrap(),
            mayfly: edited,
            instabuild: edited,
            may_build: !edited,
            flying_speed: 0.125,
            walking_speed: -0.25,
        };
        a.update_game_mode(GameType::from_id(row[1].parse().unwrap()));
        assert_eq!(snapshot(a.pack()), row[4], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 16);
}
#[test]
fn player_save_keeps_independent_abilities_and_malformed_packed_load_is_atomic() {
    let mut player = ServerPlayer::new(
        Uuid::new(3, 4),
        "abilities".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    player.abilities.invulnerable = true;
    player.abilities.mayfly = true;
    player.abilities.flying = true;
    player.abilities.set_flying_speed(0.125);
    player.abilities.set_walking_speed(-0.25);
    let saved = player.save();
    let packed = PackedAbilities::from_nbt(saved.get("abilities").unwrap()).unwrap();
    assert_eq!(snapshot(packed), snapshot(player.abilities.pack()));
    assert!(packed.invulnerable);
    assert_eq!(player.game_mode, GameType::Survival);
    let mut tag =
        ferraloom_nbt::parse_compound("{abilities:{flySpeed:0.25f,walkSpeed:'invalid'}}").unwrap();
    tag.insert("playerGameType", ferraloom_nbt::Tag::Int(1));
    player.try_load(tag).unwrap();
    assert_eq!(player.abilities.flying_speed(), 0.125);
    assert_eq!(player.abilities.walking_speed(), -0.25);
}
