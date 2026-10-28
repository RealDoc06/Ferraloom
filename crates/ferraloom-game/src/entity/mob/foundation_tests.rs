use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        Entity, EntityState,
        ai::pathfinder::{MobAccess, PathType},
        animal::Cow,
        player::{GameType, ServerPlayer},
        synched_data::{self, EntityDataAccessor},
    },
};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_protocol::{
    codec::VarInt,
    component::{CowSoundVariant, CowVariant},
    play::Difficulty,
};
use ferraloom_util::{Uuid, Vec3, random::RandomSource};
use std::cell::Cell;
const VECTORS: &str = include_str!("../../../tests/java/entity/mob_foundations/vectors.tsv");
thread_local! {static INHERIT:Cell<bool>=const{Cell::new(false)};}
struct Probe;
impl Entity for Probe {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        crate::entity::animal::CowBehavior.trace_entity_refs(state, refs);
    }
    fn should_passengers_inherit_malus(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(INHERIT.get())
    }
    fn is_alive(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        crate::entity::animal::CowBehavior.is_alive(entity, level)
    }
}
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &Probe);
fn cow(level: &mut Level, n: i64) -> EntityRef {
    let id = level.next_entity_ref();
    let mut cow = Cow::default();
    cow.as_living_mut().uuid = Uuid::new(1, n);
    level
        .add_entity(id, EntityState::Cow(Box::new(cow)))
        .unwrap();
    id
}
fn difficulty(id: &str) -> Difficulty {
    match id {
        "0" => Difficulty::Peaceful,
        "1" => Difficulty::Easy,
        "2" => Difficulty::Normal,
        "3" => Difficulty::Hard,
        _ => panic!("{id}"),
    }
}
fn float(bits: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(bits, 16).unwrap())
}
#[test]
fn native_constructor_metadata_draw_precedes_living_yaw_and_uses_one_rng() {
    const MARKER: EntityDataAccessor<VarInt> = EntityDataAccessor::new(20);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("mob-constructor\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let context = crate::level::LevelContext::fixture(0, 63);
        let mut mob = Mob::with_parent_and_metadata(
            EntityData::new(EntityType::COW, 1, 0),
            &context,
            |parent, context| {
                parent.entity_data.define(synched_data::BABY, false);
                parent.entity_data.define(synched_data::AGE_LOCKED, false);
                parent.entity_data.define(
                    synched_data::COW_VARIANT,
                    CowVariant(default_variant(
                        context,
                        "minecraft:cow_variant",
                        "minecraft:temperate",
                    )?),
                );
                parent.entity_data.define(
                    synched_data::COW_SOUND_VARIANT,
                    CowSoundVariant(default_variant(
                        context,
                        "minecraft:cow_sound_variant",
                        "minecraft:classic",
                    )?),
                );
                // The Java probe seeds at this SAME callback point to isolate order;
                // production metadata constructors never reset or copy the RNG.
                parent.random.set_seed(f[1].parse().unwrap());
                let marker = parent.random.next_int_bound(13);
                parent.entity_data.define(MARKER, VarInt(marker));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            mob.parent.entity_data.get(MARKER).0.to_string(),
            f[2],
            "{row}"
        );
        assert_eq!(mob.parent.yaw.to_bits(), float(f[3]).to_bits(), "{row}");
        assert_eq!(
            mob.parent.health().to_bits(),
            float(f[4]).to_bits(),
            "{row}"
        );
        assert_eq!(
            format!("{:x}", mob.parent.random.next_long() as u64),
            f[5],
            "{row}"
        );
        assert_eq!(mob.xp_reward, 0);
        assert!(mob.parent.attributes.has_attribute(Attribute::FOLLOW_RANGE));
        checked += 1;
    }
    assert_eq!(checked, 8);
}
#[test]
fn native_malus_map_uses_controlled_vehicle_reads_and_local_writes() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("mob-malus\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let actor = cow(&mut level, 1);
        let vehicle = cow(&mut level, 2);
        let ty = PathType::by_id(f[1].parse().unwrap());
        INHERIT.set(f[5] == "true");
        if f[2] == "true" {
            set_pathfinding_malus(actor, ty, -2.25, &mut level).unwrap();
        }
        set_pathfinding_malus(vehicle, ty, 7.75, &mut level).unwrap();
        if f[3] == "true" {
            level
                .entity_mut::<EntityData>(actor)
                .unwrap()
                .vehicle
                .set(Some(vehicle));
        }
        if f[4] == "true" {
            level
                .entity_mut::<EntityData>(vehicle)
                .unwrap()
                .passengers
                .push(actor);
        }
        let mut access = CanonicalMobAccess::new(actor, &level).unwrap();
        assert_eq!(
            access.pathfinding_malus(ty, &level).unwrap().to_bits(),
            float(f[6]).to_bits(),
            "{row}"
        );
        access.set_pathfinding_malus(ty, 1.5, &mut level).unwrap();
        assert_eq!(
            access.pathfinding_malus(ty, &level).unwrap().to_bits(),
            float(f[7]).to_bits(),
            "{row}"
        );
        assert_eq!(
            get_pathfinding_malus(vehicle, ty, &level)
                .unwrap()
                .to_bits(),
            float(f[8]).to_bits(),
            "{row}"
        );
        assert_eq!(
            level.entity::<Mob>(actor).unwrap().pathfinding_malus[ty.id() as usize],
            Some(1.5)
        );
        checked += 1;
    }
    assert_eq!(checked, 432);
}
#[test]
fn native_target_filters_and_max_fall_distance_read_live_facts() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("mob-fall\t") || row.starts_with("mob-target\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(UnportedBehavior::Strict);
        level.set_difficulty(difficulty(f[1]));
        let actor = cow(&mut level, 1);
        if f[0] == "mob-fall" {
            if f[2] == "true" {
                let target = cow(&mut level, 2);
                set_target(actor, Some(target), &mut level).unwrap();
            }
            level
                .entity_mut::<LivingEntity>(actor)
                .unwrap()
                .set_health(float(f[3]));
            let access = CanonicalMobAccess::new(actor, &level).unwrap();
            assert_eq!(
                access.max_fall_distance(&level).unwrap().to_string(),
                f[4],
                "{row}"
            );
        } else {
            let id = level.next_entity_ref();
            let mode = match f[2] {
                "survival" => GameType::Survival,
                "creative" => GameType::Creative,
                "spectator" => GameType::Spectator,
                _ => unreachable!(),
            };
            let mut player =
                ServerPlayer::new(Uuid::new(1, 2), "target".into(), 2, Vec3::ZERO, mode);
            player.abilities.invulnerable = f[3] == "true";
            if f[4] == "false" {
                player.parent.set_health(0.0);
            }
            level
                .add_entity(id, EntityState::Player(Box::new(player)))
                .unwrap();
            set_target(actor, Some(id), &mut level).unwrap();
            assert_eq!(
                get_target_unchecked(actor, &level)
                    .unwrap()
                    .is_some()
                    .to_string(),
                f[5],
                "{row}"
            );
            level.entity_mut::<Mob>(actor).unwrap().target = Some(id);
            assert_eq!(
                get_target(actor, &level).unwrap().is_some().to_string(),
                f[6],
                "{row}"
            );
            assert_eq!(
                get_target_unchecked(actor, &level)
                    .unwrap()
                    .is_some()
                    .to_string(),
                f[7],
                "{row}"
            );
            assert_eq!(
                level
                    .entity_behavior(actor)
                    .unwrap()
                    .can_attack(actor, id, &level)
                    .unwrap()
                    .to_string(),
                f[8],
                "{row}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 120);
}
