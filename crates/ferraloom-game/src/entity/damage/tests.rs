use super::*;
use crate::{
    Behaviors,
    entity::{
        EntityData, EntityRef, EntityState,
        animal::Cow,
        item::PrimedTnt,
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelContext, Region},
};
use ferraloom_data::{EntityType, datapack::Pack};
use ferraloom_util::{Uuid, Vec3};
use std::{path::Path, sync::OnceLock};

const VECTORS: &str = include_str!("../../../tests/java/entity/vectors.tsv");
fn resources() -> ResourceManager {
    ResourceManager::new(vec![vanilla_pack()])
}
fn vanilla_pack() -> Pack {
    let jar =
        std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla resources");
    Pack::vanilla_jar(Path::new(&jar)).unwrap()
}
fn registry() -> &'static DamageTypeRegistry {
    static REGISTRY: OnceLock<DamageTypeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| DamageTypeRegistry::load(&resources()).unwrap())
}
fn fields(value: &DamageType) -> String {
    format!(
        "{}\t{}\t{:x}\t{}\t{}",
        value.message_id,
        value.scaling.name(),
        value.exhaustion.to_bits(),
        value.effects.name(),
        value.death_message_type.name()
    )
}

#[test]
fn loaded_damage_types_tags_effect_sounds_and_codec_match_vanilla() {
    let registry = registry();
    let mut types = 0;
    let mut codecs = 0;
    for row in VECTORS.lines() {
        let values: Vec<_> = row.split('\t').collect();
        match values[0] {
            "damage-type" => {
                let holder = registry.get(&values[1].parse().unwrap()).unwrap();
                assert_eq!(
                    fields(holder.value()),
                    values[2..7].join("\t"),
                    "{}",
                    values[1]
                );
                let mut tags: Vec<_> = DamageTypeTag::ALL
                    .iter()
                    .copied()
                    .filter(|&tag| holder.is_tag(tag))
                    .map(DamageTypeTag::name)
                    .collect();
                tags.sort_unstable();
                assert_eq!(tags.join(","), values[7], "{}", values[1]);
                assert_eq!(holder.value().effects.sound().name(), values[8]);
                types += 1;
            }
            "damage-codec" => {
                let result = DamageType::decode(&serde_json::from_str(values[2]).unwrap());
                assert_eq!(result.is_ok(), values[3] == "true", "{}", values[2]);
                if let Ok(value) = result {
                    assert_eq!(fields(&value), values[4..].join("\t"), "{}", values[2]);
                }
                codecs += 1;
            }
            _ => (),
        }
    }
    assert_eq!((types, codecs), (51, 19));
    assert_eq!(registry.len(), types);
}

#[test]
fn datapack_overrides_and_nested_damage_tags_drive_held_source_queries() {
    let mut pack = Pack::new("damage overrides");
    pack.insert(
        "minecraft:damage_type/freeze.json".parse().unwrap(),
        br#"{"message_id":"overridden","scaling":"always","exhaustion":0.75,"effects":"thorns"}"#
            .as_slice(),
    );
    pack.insert(
        "example:damage_type/custom.json".parse().unwrap(),
        br#"{"message_id":"custom","scaling":"never","exhaustion":0}"#.as_slice(),
    );
    pack.insert(
        "example:tags/damage_type/custom.json".parse().unwrap(),
        br#"{"values":["minecraft:freeze","example:custom"]}"#.as_slice(),
    );
    pack.insert(
        "minecraft:tags/damage_type/bypasses_cooldown.json"
            .parse()
            .unwrap(),
        br##"{"replace":true,"values":["#example:custom"]}"##.as_slice(),
    );
    let resources = ResourceManager::new(vec![vanilla_pack(), pack]);
    let registry = DamageTypeRegistry::load(&resources).unwrap();
    let source = registry.sources().freeze();
    assert_eq!(source.damage_type().message_id, "overridden");
    assert_eq!(source.damage_type().exhaustion, 0.75);
    assert_eq!(source.damage_type().effects, DamageEffects::Thorns);
    assert!(source.is_type(DamageTypeKey::Freeze));
    assert!(source.is_tag(DamageTypeTag::BypassesCooldown));
    assert!(
        !registry
            .sources()
            .generic()
            .is_tag(DamageTypeTag::BypassesCooldown)
    );
    let tag = registry
        .tags()
        .resolve_name(&"example:custom".parse().unwrap())
        .unwrap();
    let custom = registry.get(&"example:custom".parse().unwrap()).unwrap();
    assert!(registry.tags().contains(tag, custom.id()));
    assert!(registry.tags().contains(tag, source.type_holder().id()));
    assert!(custom.is_tag(DamageTypeTag::BypassesCooldown));
    assert_eq!(
        source.to_loot_source().damage_type.as_str(),
        "minecraft:freeze"
    );
    assert!(registry.tags().diagnostics.is_empty());
}

fn text(position: Option<Vec3>) -> String {
    position.map_or_else(
        || "none".to_owned(),
        |p| {
            format!(
                "{:x},{:x},{:x}",
                p.x.to_bits(),
                p.y.to_bits(),
                p.z.to_bits()
            )
        },
    )
}
#[test]
fn native_source_constructors_preserve_actor_identity_scaling_and_live_positions() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let registry = registry();
    let player = ServerPlayer::new(
        Uuid::new(0, 1),
        "damage".into(),
        1,
        Vec3::ZERO,
        GameType::Creative,
    );
    let mut cow_base = EntityData::new(EntityType::COW, 2, 0);
    cow_base.uuid = Uuid::new(0, 2);
    let cow = Cow::with_parent(cow_base);
    let mut tnt = PrimedTnt::new(3, 0);
    tnt.parent.uuid = Uuid::new(0, 3);
    for (index, state) in [
        EntityState::Player(Box::new(player)),
        EntityState::Cow(Box::new(cow)),
        EntityState::PrimedTnt(Box::new(tnt)),
    ]
    .into_iter()
    .enumerate()
    {
        level
            .add_entity(EntityRef(index as u64 + 1), state)
            .unwrap();
    }
    let actors = [
        None,
        Some(EntityRef(1)),
        Some(EntityRef(2)),
        Some(EntityRef(3)),
    ];
    let sources = registry.sources();
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("damage-source\t"))
    {
        let values: Vec<_> = row.split('\t').collect();
        let name = values[1];
        let source = if let Some(actor) = name.strip_prefix("cause-") {
            DamageSource::with_cause(
                registry.holder(DamageTypeKey::Generic).clone(),
                actors[actor.parse::<usize>().unwrap()],
            )
        } else if let Some(actors_text) = name.strip_prefix("explosion-") {
            let (direct, cause) = actors_text.split_once('-').unwrap();
            sources.explosion(
                actors[direct.parse::<usize>().unwrap()],
                actors[cause.parse::<usize>().unwrap()],
            )
        } else {
            match name {
                "indirect" => DamageSource::with_entities(
                    registry.holder(DamageTypeKey::Generic).clone(),
                    actors[3],
                    actors[2],
                ),
                "explicit" => DamageSource::at_position(
                    registry.holder(DamageTypeKey::Generic).clone(),
                    Vec3::new(-0.0, 100.0, 3.0),
                ),
                "player" => sources.player_attack(EntityRef(1)),
                "mob" => sources.mob_attack(EntityRef(2)),
                "thorns" => sources.thorns(EntityRef(3)),
                "bad-respawn" => sources.bad_respawn_point_explosion(Vec3::new(-0.0, 100.0, 3.0)),
                _ => panic!("unexpected source case {name}"),
            }
        };
        assert_eq!(source.type_holder().name().to_string(), values[2], "{name}");
        assert_eq!(
            source.direct_entity(),
            actors[values[3].parse::<usize>().unwrap()]
        );
        assert_eq!(
            source.causing_entity(),
            actors[values[4].parse::<usize>().unwrap()]
        );
        assert_eq!(source.is_direct(), values[5] == "true");
        assert_eq!(
            source.scales_with_difficulty(&level).unwrap(),
            values[6] == "true"
        );
        assert_eq!(
            source.is_creative_player(&level).unwrap(),
            values[7] == "true"
        );
        assert_eq!(text(source.source_position_raw()), values[8]);
        if let Some(direct) = source.direct_entity() {
            level
                .entity_mut::<EntityData>(direct)
                .unwrap()
                .set_pos(Vec3::new(1.0, 2.0, 3.0));
        }
        assert_eq!(text(source.source_position(&level).unwrap()), values[9]);
        if let Some(direct) = source.direct_entity() {
            level
                .entity_mut::<EntityData>(direct)
                .unwrap()
                .set_pos(Vec3::new(-4.0, 5.0, -6.0));
        }
        assert_eq!(text(source.source_position(&level).unwrap()), values[10]);
        let mut traced = Vec::new();
        source.trace_entity_refs(&mut traced);
        assert_eq!(
            traced,
            source
                .direct_entity()
                .into_iter()
                .chain(source.causing_entity())
                .collect::<Vec<_>>()
        );
        count += 1;
    }
    assert_eq!(count, 14);
}
