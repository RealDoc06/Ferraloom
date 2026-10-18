use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{EntityData, EntityState, player::GameType},
};
use ferraloom_nbt::Tag;
use ferraloom_util::{Uuid, Vec3, random::LegacyRandomSource};
fn tag(id: Option<&str>, variant: Option<i32>, silent: Option<bool>) -> CompoundTag {
    let mut tag = CompoundTag::new();
    if let Some(id) = id {
        tag.insert("id", Tag::String(id.into()));
    }
    if let Some(variant) = variant {
        tag.insert("Variant", Tag::Int(variant));
    }
    if let Some(silent) = silent {
        tag.insert("Silent", Tag::Byte(i8::from(silent)));
    }
    tag
}
fn player() -> ServerPlayer {
    ServerPlayer::new(
        Uuid::new(3, 4),
        "shoulders".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    )
}
fn fixture() -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Player(Box::new(player())))
        .unwrap();
    (level, id)
}
#[test]
fn native_shoulder_metadata_clamps_legacy_variants_without_dirtying_equal_values() {
    let mut player = player();
    player.entity_data.pack_dirty();
    let tags = [
        tag(None, None, None),
        tag(Some("minecraft:parrot"), Some(0), None),
        tag(Some("minecraft:parrot"), Some(0), Some(true)),
        tag(Some("minecraft:parrot"), Some(4), None),
        tag(Some("parrot"), Some(99), None),
        tag(Some("minecraft:parrot"), Some(-7), None),
        tag(Some("minecraft:pig"), Some(2), None),
        tag(Some("minecraft:parrot"), None, None),
        tag(None, None, None),
    ];
    let mut actual = Vec::new();
    for (index, tag) in tags.into_iter().enumerate() {
        player.set_shoulder_entity_left(tag.clone());
        let variant = player
            .shoulder_parrot_left()
            .map(|v| (v as i32).to_string())
            .unwrap_or("-".into());
        let dirty = player.entity_data.pack_dirty().is_some();
        actual.push(format!(
            "shoulder-metadata\t{index}\t{variant}\t{dirty}\t{}",
            player.shoulder_entity_left() == &tag
        ));
    }
    let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("shoulder-metadata\t"))
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn native_empty_silent_and_nonempty_shoulders_consume_the_native_random_draws() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("shoulder-rng\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let (mut level, id) = fixture();
        let mut a = CompoundTag::new();
        let mut b = CompoundTag::new();
        match row[1] {
            "1" => a = tag(Some("minecraft:parrot"), Some(2), Some(true)),
            "2" => a = tag(Some("minecraft:parrot"), Some(2), Some(false)),
            "3" => {
                a = tag(Some("minecraft:parrot"), Some(2), Some(false));
                b = tag(Some("minecraft:parrot"), Some(4), Some(false));
            }
            "4" => a = tag(Some("minecraft:pig"), None, Some(false)),
            "5" => a = tag(None, None, Some(false)),
            "6" => {
                a = tag(Some("minecraft:parrot"), Some(1), Some(true));
                b = tag(Some("minecraft:parrot"), Some(3), Some(false));
            }
            "7" => {
                a = tag(Some("minecraft:parrot"), Some(1), Some(false));
                a.insert("Silent", Tag::String("invalid".into()));
            }
            _ => {}
        }
        let player = level.entity_mut::<ServerPlayer>(id).unwrap();
        player.set_shoulder_entity_left(a);
        player.set_shoulder_entity_right(b);
        player.random.set_seed(row[2].parse().unwrap());
        player.entity_data.pack_dirty();
        handle_shoulder_entities(id, &mut level).unwrap();
        let player = level.entity_mut::<ServerPlayer>(id).unwrap();
        assert_eq!(player.random.next_long().to_string(), row[3], "{row:?}");
        assert_eq!(
            player.entity_data.pack_dirty().is_some().to_string(),
            row[4]
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}
#[test]
fn shoulder_tags_roundtrip_and_unsupported_effects_preserve_native_order() {
    let mut source = player();
    let mut left = tag(Some("minecraft:parrot"), Some(3), Some(true));
    left.insert("future", Tag::Long(99));
    source.set_shoulder_entity_left(left.clone());
    let right = tag(Some("minecraft:parrot"), Some(1), Some(true));
    source.set_shoulder_entity_right(right.clone());
    let mut saved = source.save();
    saved.remove("recipeBook");
    let mut restored = player();
    restored.try_load(saved).unwrap();
    assert_eq!(restored.shoulder_entity_left(), &left);
    assert_eq!(restored.shoulder_entity_right(), &right);
    assert_eq!(
        restored.shoulder_parrot_left(),
        Some(ParrotVariant::YellowBlue)
    );
    let (mut level, id) = fixture();
    let p = level.entity_mut::<ServerPlayer>(id).unwrap();
    p.set_shoulder_entity_left(left.clone());
    p.set_shoulder_entity_right(right);
    p.fall_distance = 1.0;
    p.time_entity_sat_on_shoulder = 20;
    level.set_game_time(40);
    handle_shoulder_entities(id, &mut level).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(id)
            .unwrap()
            .shoulder_entity_left(),
        &left
    );
    level.set_game_time(41);
    assert_eq!(
        handle_shoulder_entities(id, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::PLAYER,
            EntityMethod::RespawnEntityOnShoulder
        ))
    );
    assert_eq!(
        level
            .entity::<ServerPlayer>(id)
            .unwrap()
            .shoulder_entity_left(),
        &left
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    handle_shoulder_entities(id, &mut level).unwrap();
    assert!(
        level
            .entity::<ServerPlayer>(id)
            .unwrap()
            .shoulder_entity_left()
            .is_empty()
    );
    assert!(
        level
            .entity::<ServerPlayer>(id)
            .unwrap()
            .shoulder_entity_right()
            .is_empty()
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
    let mut seed = 0;
    while LegacyRandomSource::new(seed).next_int_bound(200) != 0 {
        seed += 1;
    }
    let (mut level, id) = fixture();
    let p = level.entity_mut::<ServerPlayer>(id).unwrap();
    p.set_shoulder_entity_left(tag(Some("minecraft:parrot"), Some(1), None));
    p.set_shoulder_entity_right(tag(Some("minecraft:parrot"), Some(2), None));
    p.random.set_seed(seed);
    assert_eq!(
        handle_shoulder_entities(id, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::PLAYER,
            EntityMethod::PlayShoulderEntityAmbientSound
        ))
    );
    let mut expected = LegacyRandomSource::new(seed);
    expected.next_int_bound(200);
    assert_eq!(
        level
            .entity_mut::<EntityData>(id)
            .unwrap()
            .random
            .next_long(),
        expected.next_long()
    );
}
