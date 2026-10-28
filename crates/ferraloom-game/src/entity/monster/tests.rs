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
