use crate::{
    block::UnportedBehavior,
    entity::{self, EntityData},
};
use ferraloom_util::{Mirror, Rotation};
use ferraloom_worldgen::feature::LiveWorldGenLevel;
use serde::Deserialize;

#[derive(Deserialize)]
struct Vector {
    rotation: String,
    mirror: String,
    tag: String,
    reason: String,
    ignore_checks: bool,
    position: [f64; 3],
    yaw_bits: u32,
    pitch_bits: u32,
}
#[test]
fn native_template_entity_single_root_rotation_snap_and_registration() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/place/template-entities.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 24);
    let mut level = entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    for vector in vectors {
        assert_eq!(vector.reason, "STRUCTURE");
        assert!(!vector.ignore_checks);
        let tag = ferraloom_nbt::parse_compound(&vector.tag).unwrap();
        assert!(!tag.contains_key("UUID"));
        // Saved passengers include an unported type. Single-root EntityType#create
        // ignores them; using the recursive summon loader would be a required error.
        assert!(tag.contains_key("Passengers"));
        let mirror = match vector.mirror.as_str() {
            "NONE" => Mirror::None,
            "LEFT_RIGHT" => Mirror::LeftRight,
            "FRONT_BACK" => Mirror::FrontBack,
            value => panic!("{value}"),
        };
        let rotation = match vector.rotation.as_str() {
            "NONE" => Rotation::None,
            "CLOCKWISE_90" => Rotation::Clockwise90,
            "CLOCKWISE_180" => Rotation::Clockwise180,
            "COUNTERCLOCKWISE_90" => Rotation::CounterClockwise90,
            value => panic!("{value}"),
        };
        let before = level.all_entities();
        level
            .place_template_entity([0, 0], tag, mirror, rotation, false)
            .unwrap();
        let after = level.all_entities();
        assert_eq!(after.len(), before.len() + 1);
        let root = after
            .into_iter()
            .find(|entity| !before.contains(entity))
            .unwrap();
        let base = level.entity::<EntityData>(root).unwrap();
        assert_eq!(
            [base.position.x, base.position.y, base.position.z],
            vector.position
        );
        assert_eq!(base.yaw.to_bits(), vector.yaw_bits);
        assert_eq!(base.pitch.to_bits(), vector.pitch_bits);
        assert!(base.passengers.is_empty());
    }
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}
