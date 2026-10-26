use super::*;
use crate::entity::{EntityData, EntityState, RemovalReason, animal::Cow, player::GameType};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_nbt::ListTag;
use ferraloom_util::Vec3;
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn fixture(player: bool) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::persistence_level();
    let id = level.next_entity_ref();
    let parent = level
        .create_entity_data(if player {
            EntityType::PLAYER
        } else {
            EntityType::COW
        })
        .unwrap();
    let state = if player {
        EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(9, 9),
            "damage".into(),
            parent.network_id,
            Vec3::ZERO,
            GameType::Survival,
        )))
    } else {
        EntityState::Cow(Box::new(Cow::with_parent(parent)))
    };
    level.add_entity(id, state).unwrap();
    (level, id)
}
fn input(scenario: &str) -> CompoundTag {
    if scenario == "absent" {
        return CompoundTag::new();
    }
    let mut tag = ferraloom_nbt::parse_compound("{Health:7.5f,HurtTime:10s,DeathTime:12s,AbsorptionAmount:6.25f,FallFlying:1b,last_hurt_by_player:[I;1,2,3,4],last_hurt_by_player_memory_time:7,last_hurt_by_mob:[I;5,6,7,8],ticks_since_last_hurt_by_mob:2147483647}").unwrap();
    match scenario {
        "negative" => {
            tag.insert("AbsorptionAmount", Tag::Float(-5.0));
            tag.insert("HurtTime", Tag::Short(-1));
            tag.insert("DeathTime", Tag::Short(-3));
            tag.insert("last_hurt_by_player_memory_time", Tag::Int(-1));
            tag.insert("ticks_since_last_hurt_by_mob", Tag::Int(-1));
        }
        "nan" => {
            tag.insert("AbsorptionAmount", Tag::Float(f32::NAN));
        }
        "infinity" => {
            tag.insert("AbsorptionAmount", Tag::Float(f32::INFINITY));
        }
        "wrong" => {
            for key in [
                "AbsorptionAmount",
                "DeathTime",
                "HurtTime",
                "last_hurt_by_player_memory_time",
                "ticks_since_last_hurt_by_mob",
            ] {
                tag.insert(key, Tag::String("bad".into()));
            }
            tag.insert(
                "last_hurt_by_player",
                Tag::String("00000000-0000-0000-0000-000000000001".into()),
            );
            tag.insert("last_hurt_by_mob", Tag::IntArray(vec![1, 2, 3]));
        }
        "numeric" => {
            tag.insert(
                "last_hurt_by_player",
                Tag::LongArray(vec![1_i64 << 32, 2, 3, 4]),
            );
            tag.insert(
                "last_hurt_by_mob",
                Tag::List(ListTag(vec![
                    Tag::Double(1.5),
                    Tag::Double(f64::NAN),
                    Tag::Double(f64::INFINITY),
                    Tag::Double(f64::NEG_INFINITY),
                ])),
            );
        }
        "clamp" => {
            tag.insert("Health", Tag::Float(50.0));
        }
        "normal" => {}
        _ => panic!("unknown native damage load"),
    }
    tag
}
fn reference(tag: &CompoundTag, name: &str) -> String {
    tag.get_int_array(name)
        .map(|words| format!("{words:?}").replace(' ', ""))
        .unwrap_or("-".into())
}
fn absorption(entity: EntityRef, level: &Level) -> f32 {
    if let Ok(player) = level.entity::<ServerPlayer>(entity) {
        player.absorption_amount()
    } else {
        level
            .entity::<LivingEntity>(entity)
            .unwrap()
            .absorption_amount()
    }
}
#[test]
fn native_damage_memory_load_and_virtual_absorption_save_preserve_typed_references() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-memory-load\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(f[1] == "player");
        level.entity_mut::<EntityData>(entity).unwrap().tick_count = 23;
        if f[1] == "player" {
            level
                .entity_mut::<ServerPlayer>(entity)
                .unwrap()
                .try_load(input(f[2]))
                .unwrap();
        } else {
            level
                .entity_mut::<LivingEntity>(entity)
                .unwrap()
                .load_nbt(input(f[2]))
                .unwrap();
        }
        let saved = level.entity_nbt_to_compare(entity).unwrap();
        let living = level.entity::<LivingEntity>(entity).unwrap();
        assert_eq!(
            format!(
                "{:x}\t{}\t{}\t{:x}\t{}\t{}\t{}\t{}\t{}\t{}\t{:x}\t{}",
                living.health().to_bits(),
                living.hurt_time,
                living.death_time,
                absorption(entity, &level).to_bits(),
                living.shared_flag(7),
                living.last_hurt_by_mob_timestamp(),
                reference(&saved, "last_hurt_by_player"),
                saved.get_int_or("last_hurt_by_player_memory_time", -999),
                reference(&saved, "last_hurt_by_mob"),
                saved.get_int_or("ticks_since_last_hurt_by_mob", -999),
                saved.get_float_or("AbsorptionAmount", -999.0).to_bits(),
                saved.get_short_or("DeathTime", -999)
            ),
            f[3..].join("\t"),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 16);
}
#[test]
fn native_absorption_setter_clamps_using_live_maximum_and_keeps_player_metadata_canonical() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-absorption\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(f[1] == "player");
        let float = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
        level
            .entity_mut::<LivingEntity>(entity)
            .unwrap()
            .attributes
            .get_instance(Attribute::MAX_ABSORPTION)
            .unwrap()
            .set_base_value(f64::from(float(f[2])));
        if f[1] == "player" {
            let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
            player.set_absorption_amount(float(f[3]));
            assert_eq!(player.parent.parent.absorption_amount().to_bits(), 0);
        } else {
            level
                .entity_mut::<LivingEntity>(entity)
                .unwrap()
                .set_absorption_amount(float(f[3]));
        }
        assert_eq!(
            format!("{:x}", absorption(entity, &level).to_bits()),
            f[4],
            "{row}"
        );
        let saved = level.entity_nbt_to_compare(entity).unwrap();
        assert_eq!(
            format!(
                "{:x}",
                saved.get_float_or("AbsorptionAmount", -999.0).to_bits()
            ),
            f[5],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 24);
}
#[test]
fn cached_damage_references_keep_removed_owners_and_save_current_uuid_until_invalidated() {
    let (mut level, receiver) = fixture(true);
    let actor = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(1, 2),
        "attacker".into(),
        level.next_entity_id().unwrap(),
        Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(actor, EntityState::Player(Box::new(player)))
        .unwrap();
    level.entity_mut::<EntityData>(receiver).unwrap().tick_count = 17;
    set_last_hurt_by_mob(receiver, Some(actor), &mut level).unwrap();
    set_last_hurt_by_player(receiver, Some(actor), 19, &mut level).unwrap();
    let rows: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-memory-reference\t"))
        .collect();
    for row in &rows {
        let f: Vec<_> = row.split('\t').collect();
        match f[1] {
            "uuid-changed" => {
                level.entity_mut::<EntityData>(actor).unwrap().uuid = Uuid::new(3, 4);
            }
            "removed" => {
                level
                    .remove_entity_with_reason(actor, RemovalReason::Discarded)
                    .unwrap();
                level.finish_entity_management();
                assert!(level.entity::<EntityData>(actor).is_ok());
            }
            "cache-invalidated" => {
                assert_eq!(last_hurt_by_mob(receiver, &mut level).unwrap(), None);
                assert_eq!(last_hurt_by_player(receiver, &mut level).unwrap(), None);
                level.entity_mut::<EntityData>(actor).unwrap().uuid = Uuid::new(5, 6);
            }
            "cleared" => {
                set_last_hurt_by_mob(receiver, None, &mut level).unwrap();
                set_last_hurt_by_player(receiver, None, 0, &mut level).unwrap();
            }
            "captured" => {}
            _ => panic!("unknown reference step"),
        }
        let saved = level
            .entity::<ServerPlayer>(receiver)
            .unwrap()
            .save(&level)
            .unwrap();
        assert_eq!(
            format!(
                "{}\t{}\t{}\t{}",
                reference(&saved, "last_hurt_by_player"),
                reference(&saved, "last_hurt_by_mob"),
                saved.get_int_or("last_hurt_by_player_memory_time", -999),
                saved.get_int_or("ticks_since_last_hurt_by_mob", -999)
            ),
            f[2..].join("\t"),
            "{row}"
        );
    }
    assert_eq!(rows.len(), 5);
    level.finish_entity_management();
    assert!(matches!(
        level.entity_state(actor),
        Err(LevelError::MissingEntity(_))
    ));
}
#[test]
fn player_save_propagates_a_missing_reference_owner_instead_of_snapshotting_its_old_uuid() {
    let (mut level, receiver) = fixture(true);
    let actor = level.next_entity_ref();
    level
        .add_entity(actor, EntityState::Cow(Box::default()))
        .unwrap();
    set_last_hurt_by_mob(receiver, Some(actor), &mut level).unwrap();
    let other_level = crate::entity::tests::persistence_level();
    assert_eq!(
        level
            .entity::<ServerPlayer>(receiver)
            .unwrap()
            .save(&other_level),
        Err(LevelError::MissingEntity(actor))
    );
}
