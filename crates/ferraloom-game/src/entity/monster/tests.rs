use super::{cubemob, spider, zombie};
use crate::entity::{
    EntityData, LivingEntity,
    synched_data::{self, EntityDataAccessor, SynchedEntityData, SynchedValue},
};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_protocol::{
    component::{VillagerProfession, VillagerType},
    entity_data::{EntityDataValue, VillagerData},
};
use std::collections::BTreeMap;

const VECTORS: &str = include_str!("../../../tests/java/entity/monster/vectors.tsv");

#[test]
fn native_bootstrap_suppliers_match_every_family_attribute_exactly() {
    let mut expected = BTreeMap::<&str, BTreeMap<&str, u64>>::new();
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("monster-attribute\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let bits = u64::from_str_radix(fields[3], 16).unwrap();
        assert!(
            expected
                .entry(fields[1])
                .or_default()
                .insert(fields[2], bits)
                .is_none()
        );
        checked += 1;
    }
    assert_eq!(expected.len(), 9);
    for (name, expected) in expected {
        let kind = EntityType::from_name(name).unwrap();
        let actual: BTreeMap<_, _> = kind
            .data()
            .attributes
            .iter()
            .map(|(attribute, value)| (attribute.name(), value.to_bits()))
            .collect();
        assert_eq!(actual, expected, "{name}");
        let map = crate::entity::ai::attributes::AttributeMap::new(kind);
        for (name, bits) in expected {
            let attribute = Attribute::from_name(name).unwrap();
            assert_eq!(map.base_value(attribute).unwrap().to_bits(), bits);
        }
    }
    assert_eq!(checked, 257);
}

fn describe<T: SynchedValue>(
    data: &SynchedEntityData,
    accessor: EntityDataAccessor<T>,
) -> (u8, i32, String) {
    let value = data.get(accessor).clone().into_value();
    let initial = match &value {
        EntityDataValue::Boolean(value) => value.to_string(),
        EntityDataValue::Byte(value) => value.to_string(),
        EntityDataValue::Int(value) => value.0.to_string(),
        EntityDataValue::VillagerData(_) => "runtime".to_owned(),
        _ => panic!("unexpected monster metadata serializer"),
    };
    (accessor.id(), value.serializer() as i32, initial)
}

#[test]
fn native_metadata_ids_serializers_and_defaults_preserve_the_parent_chain() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("monster-data\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let kind = EntityType::from_name(fields[1]).unwrap();
        let mut living = LivingEntity::with_parent(EntityData::new(kind, 1, 0));
        let data = &mut living.entity_data;
        data.define(synched_data::MOB_FLAGS, 0);
        if matches!(kind, EntityType::SLIME | EntityType::MAGMA_CUBE) {
            data.define(synched_data::BABY, false);
            data.define(synched_data::AGE_LOCKED, false);
            cubemob::define_synched_data(data);
        } else if matches!(kind, EntityType::SPIDER | EntityType::CAVE_SPIDER) {
            spider::define_synched_data(data);
        } else {
            zombie::define_synched_data(data);
            if kind == EntityType::ZOMBIE_VILLAGER {
                zombie::villager::define_synched_data(
                    data,
                    VillagerData {
                        villager_type: VillagerType("minecraft:plains".parse().unwrap()),
                        profession: VillagerProfession("minecraft:none".parse().unwrap()),
                        level: 1,
                    },
                );
            }
        }
        let actual = match (fields[3], fields[4]) {
            ("Zombie", "DATA_BABY_ID") => describe(data, zombie::DATA_BABY_ID),
            ("Zombie", "DATA_SPECIAL_TYPE_ID") => describe(data, zombie::DATA_SPECIAL_TYPE_ID),
            ("Zombie", "DATA_DROWNED_CONVERSION_ID") => {
                describe(data, zombie::DATA_DROWNED_CONVERSION_ID)
            }
            ("ZombieVillager", "DATA_CONVERTING_ID") => {
                describe(data, zombie::villager::DATA_CONVERTING_ID)
            }
            ("ZombieVillager", "DATA_VILLAGER_DATA") => {
                describe(data, zombie::villager::DATA_VILLAGER_DATA)
            }
            ("ZombieVillager", "DATA_VILLAGER_DATA_FINALIZED") => {
                describe(data, zombie::villager::DATA_VILLAGER_DATA_FINALIZED)
            }
            ("Spider", "DATA_FLAGS_ID") => describe(data, spider::DATA_FLAGS_ID),
            ("AgeableMob", "DATA_BABY_ID") => describe(data, synched_data::BABY),
            ("AgeableMob", "AGE_LOCKED") => describe(data, synched_data::AGE_LOCKED),
            ("AbstractCubeMob", "ID_SIZE") => describe(data, cubemob::ID_SIZE),
            _ => panic!("unhandled native field: {row}"),
        };
        assert_eq!(
            actual,
            (
                fields[2].parse().unwrap(),
                fields[5].parse().unwrap(),
                fields[6].to_owned()
            ),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 26);
}

#[test]
fn canonical_constructors_match_native_parent_xp_health_and_navigation() {
    use crate::entity::ai::{AiOperation, NavigationKind};
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("monster-constructor\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let kind = EntityType::from_name(f[1]).unwrap();
        let base = EntityData::new(kind, 1, 0);
        let (monster, navigation) = match kind {
            EntityType::DROWNED => (
                zombie::drowned::Drowned::with_level(base, &level)
                    .unwrap()
                    .parent
                    .parent,
                NavigationKind::Amphibious,
            ),
            EntityType::ZOMBIFIED_PIGLIN => (
                zombie::zombified_piglin::ZombifiedPiglin::with_level(base, &level)
                    .unwrap()
                    .parent
                    .parent,
                NavigationKind::Ground,
            ),
            EntityType::ZOMBIE => (
                zombie::Zombie::with_level(base, &level).unwrap().parent,
                NavigationKind::Ground,
            ),
            EntityType::HUSK => (
                zombie::Husk::with_level(base, &level)
                    .unwrap()
                    .parent
                    .parent,
                NavigationKind::Ground,
            ),
            EntityType::SPIDER => (
                spider::Spider::with_level(base, &level).unwrap().parent,
                NavigationKind::WallClimber,
            ),
            EntityType::CAVE_SPIDER => (
                spider::CaveSpider::with_level(base, &level)
                    .unwrap()
                    .parent
                    .parent,
                NavigationKind::WallClimber,
            ),
            _ => continue,
        };
        assert_eq!(monster.as_mob().xp_reward, f[5].parse::<i32>().unwrap());
        let living = monster.as_living();
        assert_eq!(
            living.dimensions.width.to_bits(),
            u32::from_str_radix(f[6], 16).unwrap(),
            "{} width",
            f[1]
        );
        assert_eq!(
            living.dimensions.height.to_bits(),
            u32::from_str_radix(f[7], 16).unwrap(),
            "{} height",
            f[1]
        );
        assert_eq!(
            living.eye_height.to_bits(),
            u32::from_str_radix(f[8], 16).unwrap(),
            "{} eye",
            f[1]
        );
        assert_eq!(
            living.health().to_bits(),
            u32::from_str_radix(f[9], 16).unwrap()
        );
        assert_eq!(
            monster.as_mob().ai.navigation.require(),
            Err(AiOperation::Navigation(navigation))
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
}

#[test]
fn zombie_subclass_save_load_round_trips_match_vanilla() {
    use ferraloom_nbt::{CompoundTag, Tag};
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("zombie-nbt\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let kind = EntityType::from_name(f[1]).unwrap();
        let mut mob = if kind == EntityType::DROWNED {
            zombie::drowned::Drowned::with_level(EntityData::new(kind, 1, 0), &level)
                .unwrap()
                .parent
        } else {
            zombie::Zombie::with_level(EntityData::new(kind, 1, 0), &level).unwrap()
        };
        let mut tag = CompoundTag::new();
        tag.insert("IsBaby", Tag::Byte(i8::from(f[2] == "true")));
        tag.insert("InWaterTime", Tag::Int(37));
        tag.insert("DrownedConversionTime", Tag::Int(f[3].parse().unwrap()));
        // Parent loading is owned by entity core. These are the subclass read
        // steps, with the dimension callback boundary between metadata and speed.
        mob.set_baby_metadata(tag.get_boolean_or("IsBaby", false));
        mob.update_baby_speed();
        assert!(!tag.get_boolean_or("CanBreakDoors", false));
        mob.read_drowning_data(&tag);
        let mut saved = CompoundTag::new();
        mob.add_zombie_save_data(&mut saved, false);
        assert_eq!(saved.get_boolean_or("IsBaby", false), f[4] == "true");
        assert_eq!(saved.get_boolean_or("CanBreakDoors", false), f[5] == "true");
        assert_eq!(
            saved.get_int_or("InWaterTime", 0),
            f[6].parse::<i32>().unwrap()
        );
        assert_eq!(
            saved.get_int_or("DrownedConversionTime", 0),
            f[7].parse::<i32>().unwrap()
        );
        assert_eq!(
            mob.as_living()
                .attributes
                .value(Attribute::MOVEMENT_SPEED)
                .unwrap()
                .to_bits(),
            u64::from_str_radix(f[8], 16).unwrap()
        );
        assert_eq!(mob.is_under_water_converting(), f[9] == "true");
        let mut again = zombie::Zombie::with_level(EntityData::new(kind, 2, 1), &level).unwrap();
        again.set_baby_metadata(saved.get_boolean_or("IsBaby", false));
        again.update_baby_speed();
        again.read_drowning_data(&saved);
        let mut repeated = CompoundTag::new();
        again.add_zombie_save_data(&mut repeated, false);
        assert_eq!(saved, repeated);
        checked += 1;
    }
    assert_eq!(checked, 30);
}

#[test]
fn native_drowned_and_piglin_constructor_fields_have_one_canonical_owner() {
    use crate::entity::ai::{AiOperation, MoveControlKind, NavigationKind, pathfinder::PathType};
    let level = crate::entity::tests::level(false);
    let drowned =
        zombie::drowned::Drowned::with_level(EntityData::new(EntityType::DROWNED, 1, 0), &level)
            .unwrap();
    let piglin = zombie::zombified_piglin::ZombifiedPiglin::with_level(
        EntityData::new(EntityType::ZOMBIFIED_PIGLIN, 2, 0),
        &level,
    )
    .unwrap();
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("monster-subclass\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        match f[1] {
            "drowned" => {
                assert_eq!(f[2], "AmphibiousPathNavigation");
                assert_eq!(f[3], "DrownedMoveControl");
                let ai = &drowned.parent.as_mob().ai;
                assert_eq!(
                    ai.navigation.require(),
                    Err(AiOperation::Navigation(NavigationKind::Amphibious))
                );
                assert_eq!(
                    ai.move_control.require(),
                    Err(AiOperation::MoveControl(MoveControlKind::Drowned))
                );
                assert_eq!(
                    drowned.parent.as_mob().pathfinding_malus[PathType::Water.id() as usize]
                        .unwrap()
                        .to_bits(),
                    u32::from_str_radix(f[4], 16).unwrap()
                );
                assert_eq!(drowned.searching_for_land, f[5] == "true");
                assert_eq!(
                    drowned.ranged_attack_uncertainty.to_bits(),
                    u32::from_str_radix(f[6], 16).unwrap()
                );
            }
            "zombified_piglin" => {
                assert_eq!(
                    piglin.parent.as_mob().pathfinding_malus[PathType::Lava.id() as usize]
                        .unwrap()
                        .to_bits(),
                    u32::from_str_radix(f[2], 16).unwrap()
                );
                assert_eq!(
                    piglin.play_first_anger_sound_in,
                    f[3].parse::<i32>().unwrap()
                );
                assert_eq!(
                    piglin.persistent_anger_end_time,
                    f[4].parse::<i64>().unwrap()
                );
                assert_eq!(piglin.ticks_until_next_alert, f[5].parse::<i32>().unwrap());
                assert!(piglin.persistent_anger_target.is_none());
            }
            name => panic!("unrecognized subclass {name}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn native_piglin_anger_nbt_prioritizes_absolute_time_and_round_trips_references() {
    use zombie::zombified_piglin::ZombifiedPiglin;
    let level = crate::entity::tests::level(false);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("piglin-anger\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let input = ferraloom_nbt::parse_compound(f[2]).unwrap();
        let mut piglin = ZombifiedPiglin::with_level(
            EntityData::new(EntityType::ZOMBIFIED_PIGLIN, 1, 0),
            &level,
        )
        .unwrap();
        piglin.read_anger_end_time(&input, f[1].parse().unwrap());
        piglin.read_anger_target(&input);
        let mut saved = ferraloom_nbt::CompoundTag::new();
        piglin.add_anger_save_data(&mut saved, &level).unwrap();
        assert_eq!(
            saved.get_long_or("anger_end_time", 0),
            f[3].parse::<i64>().unwrap()
        );
        let expected_target = if f[4] == "-" {
            None
        } else {
            Some(ferraloom_nbt::parse(f[4]).unwrap())
        };
        assert_eq!(saved.get("angry_at"), expected_target.as_ref());
        // The native mocked ServerLevel has no entity with that UUID. Resolution
        // is a live read and does not erase the retained unresolved reference.
        let target = piglin
            .persistent_anger_target
            .as_mut()
            .and_then(|r| r.resolve(&level).unwrap());
        assert_eq!(target.is_none(), f[5] == "true");
        let mut again = ZombifiedPiglin::with_level(
            EntityData::new(EntityType::ZOMBIFIED_PIGLIN, 2, 1),
            &level,
        )
        .unwrap();
        again.read_anger_end_time(&saved, f[1].parse().unwrap());
        again.read_anger_target(&saved);
        let mut repeated = ferraloom_nbt::CompoundTag::new();
        again.add_anger_save_data(&mut repeated, &level).unwrap();
        assert_eq!(saved, repeated);
        checked += 1;
    }
    assert_eq!(checked, 24);
}
