use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    level::{LevelContext, Region, chunk::LevelChunk, weather::WeatherEnvironment},
};
use ferraloom_chunk::{IdMap, LevelChunkSection, Strategy};
use ferraloom_util::{ChunkPos, Vec3, random::LegacyRandomSource};
use ferraloom_worldgen::generator::ChunkGeneratorAccess;

fn fixture(biome: &str) -> (Level, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let context = LevelContext::fixture(0, 63);
    let data = context.generator.data();
    let biome = data.biome_ids[&biome.parse().unwrap()];
    let section = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![biome]).unwrap())),
        biome,
    );
    let weather = WeatherEnvironment {
        temperatures: data.temperatures.clone(),
        biome_names: Arc::new(data.biome_names.clone()),
        biome_zoom_seed: 0,
        sea_level: 63,
    };
    let mut level = Level::new(Region::new(&BEHAVIORS), context);
    level.set_unported_behavior(UnportedBehavior::Strict);
    level.set_weather_environment(weather);
    level
        .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), -4..20, section))
        .unwrap();
    let entity = level.prepare_unmanaged_entity(EntityType::COW).unwrap();
    level
        .set_entity_pos(entity, Vec3::new(8.5, 64.0, 8.5))
        .unwrap();
    (level, entity)
}
#[test]
fn cow_finalize_spawn_matches_native_variants_group_age_permanent_bonus_flags_and_both_rng_streams()
{
    let mut count = 0;
    for line in include_str!("../../../../tests/java/entity/summon/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("cow-finalize\t"))
    {
        let row = line.split('\t').collect::<Vec<_>>();
        let (mut level, entity) = fixture(row[1]);
        let scenario = row[3];
        let mut group = match scenario {
            "new" | "existing-bonus" => None,
            "force-baby" | "already-baby" | "locked-age" => {
                Some(AgeableMobGroupData::new(true, 1.0))
            }
            "never-baby" => Some(AgeableMobGroupData::new(false, 1.0)),
            _ => Some(AgeableMobGroupData::default()),
        };
        if scenario != "group-first"
            && let Some(group) = &mut group
        {
            group.increase_group_size_by_one();
        }
        {
            let cow = level.entity_mut::<Cow>(entity).unwrap();
            cow.as_living_mut()
                .entity_data
                .set(synched_data::MOB_FLAGS, 7);
            if scenario == "already-baby" {
                cow.set_age(-17);
            }
            if scenario == "locked-age" {
                cow.as_living_mut()
                    .entity_data
                    .set(synched_data::AGE_LOCKED, true);
            }
            if scenario == "existing-bonus" {
                cow.as_living_mut()
                    .attributes
                    .get_instance(Attribute::FOLLOW_RANGE)
                    .unwrap()
                    .add_permanent_modifier(Arc::new(AttributeModifier {
                        id: "minecraft:random_spawn_bonus".parse().unwrap(),
                        amount: JavaF64(0.25),
                        operation: AttributeOperation::AddValue,
                    }))
                    .unwrap();
            }
            cow.as_living_mut().random.set_seed(54321);
        }
        level.random().set_seed(row[2].parse().unwrap());
        let group = finalize_spawn(entity, EntitySpawnReason::Command, group, &mut level).unwrap();
        let cow = level.entity::<Cow>(entity).unwrap();
        assert_eq!(cow.variant().0.as_str(), row[4]);
        assert_eq!(cow.sound_variant().0.as_str(), row[5]);
        assert_eq!(cow.parent.parent.parent.age, row[6].parse::<i32>().unwrap());
        assert_eq!(cow.parent.parent.parent.age < 0, row[7] == "true");
        assert_eq!(
            *cow.as_living().entity_data.get(synched_data::MOB_FLAGS),
            row[8].parse::<i8>().unwrap()
        );
        assert_eq!(group.group_size, row[9].parse::<i32>().unwrap());
        assert_eq!(group.should_spawn_baby, row[10] == "true");
        assert_eq!(format!("{:x}", group.baby_spawn_chance.to_bits()), row[11]);
        let attributes = level
            .entity_mut::<LivingEntity>(entity)
            .unwrap()
            .attributes
            .get_instance(Attribute::FOLLOW_RANGE)
            .unwrap();
        let modifier = attributes
            .modifier(&"minecraft:random_spawn_bonus".parse().unwrap())
            .unwrap();
        assert_eq!(format!("{:x}", modifier.amount.0.to_bits()), row[12]);
        assert_eq!(modifier.operation as i32, row[13].parse::<i32>().unwrap());
        assert_eq!(
            attributes
                .permanent_modifiers()
                .iter()
                .any(|entry| Arc::ptr_eq(entry, modifier)),
            row[14] == "true"
        );
        drop(attributes);
        let dimensions = level
            .entity_behavior(entity)
            .unwrap()
            .dimensions(
                entity,
                ferraloom_protocol::entity_data::Pose::Standing,
                &level,
            )
            .unwrap();
        assert_eq!(
            format!(
                "{:x},{:x}",
                dimensions.width.to_bits(),
                dimensions.height.to_bits()
            ),
            row[15]
        );
        assert_eq!(level.random().next_long(), row[16].parse::<i64>().unwrap());
        assert_eq!(
            level
                .entity_mut::<EntityData>(entity)
                .unwrap()
                .random
                .next_long(),
            row[17].parse::<i64>().unwrap()
        );
        level.release_entity(entity);
        level.finish_entity_management();
        count += 1;
    }
    assert_eq!(count, 120);
}

#[test]
fn finalize_spawn_requires_biome_context_before_rng_and_other_types_stay_required() {
    let mut level = crate::entity::tests::persistence_level();
    let cow = level.prepare_unmanaged_entity(EntityType::COW).unwrap();
    level.random().set_seed(123);
    assert!(finalize_spawn(cow, EntitySpawnReason::Command, None, &mut level).is_err());
    assert_eq!(
        level.random().next_long(),
        LegacyRandomSource::new(123).next_long()
    );
    let tnt = level.prepare_unmanaged_entity(EntityType::TNT).unwrap();
    assert_eq!(
        finalize_spawn(tnt, EntitySpawnReason::Command, None, &mut level).unwrap_err(),
        LevelError::UnsupportedEntityMethod(EntityType::TNT, EntityMethod::FinalizeSpawn)
    );
    level.release_entity(cow);
    level.release_entity(tnt);
}
