use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        EntityState,
        animal::{Cow, CowBehavior},
        player::GameType,
    },
};
use ferraloom_data::EntityType;
use ferraloom_util::{Uuid, Vec3, random::RandomSource};
use std::cell::{Cell, RefCell};

const VECTORS: &str = include_str!("../../../../tests/java/entity/death_seams/vectors.tsv");
thread_local! {
    static TRACE:RefCell<Vec<String>>=const {RefCell::new(Vec::new())};
    static CONSUME:Cell<bool>=const {Cell::new(false)};
}
struct DeathReceiver;
impl Entity for DeathReceiver {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        CowBehavior.trace_entity_refs(state, refs);
    }
    fn handle_killing_blow(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        TRACE.with(|trace| {
            trace.borrow_mut().push(format!(
                "killing:{}",
                level.entity::<LivingEntity>(entity).unwrap().dead
            ))
        });
        level.entity_mut::<LivingEntity>(entity)?.dead = true;
        Ok(())
    }
    fn game_event(
        &self,
        entity: EntityRef,
        event: game_event::Id,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let living = level.entity::<LivingEntity>(entity)?;
        TRACE.with(|trace| {
            trace.borrow_mut().push(format!(
                "event:{}:{}:{}:{}:{}",
                event.name(),
                living.dead,
                living.was_experience_consumed(),
                living.use_item.is_empty(),
                living.use_item_remaining
            ))
        });
        if CONSUME.get() {
            level
                .entity_mut::<LivingEntity>(entity)?
                .skip_drop_experience();
        }
        Ok(())
    }
    fn drop_all_death_loot(
        &self,
        entity: EntityRef,
        _source: &DamageSource,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let living = level.entity::<LivingEntity>(entity)?;
        TRACE.with(|trace| {
            trace.borrow_mut().push(format!(
                "loot:{}:{}",
                living.dead,
                living.was_experience_consumed()
            ))
        });
        Err(required(entity, EntityMethod::DropAllDeathLoot, level))
    }
}
static DEATH: DeathReceiver = DeathReceiver;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &DEATH);

#[test]
fn native_catalyst_reward_gates_rng_and_death_callback_consumption_order() {
    let mut checked = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::persistence_level();
        level.set_unported_behavior(UnportedBehavior::Strict);
        if f[0] == "death-xp-cow" {
            let id = level.next_entity_ref();
            let mut cow = Cow::default();
            cow.parent.parent.parent.age = if f[2] == "true" { -1 } else { 0 };
            cow.as_living_mut()
                .entity_data
                .set(synched_data::BABY, f[2] == "true");
            if f[3] == "true" {
                cow.as_living_mut().skip_drop_experience();
            }
            cow.as_living_mut().random.set_seed(f[1].parse().unwrap());
            level
                .add_entity(id, EntityState::Cow(Box::new(cow)))
                .unwrap();
            let behavior = level.entity_behavior(id).unwrap();
            // Catalyst asks for reward before the baby eligibility gate.
            let reward = behavior
                .get_experience_reward(id, None, &mut level)
                .unwrap();
            assert_eq!(
                behavior
                    .should_drop_experience(id, &level)
                    .unwrap()
                    .to_string(),
                f[4],
                "{line}"
            );
            assert_eq!(reward.to_string(), f[5], "{line}");
            assert_eq!(
                format!(
                    "{:x}",
                    level
                        .entity_mut::<EntityData>(id)
                        .unwrap()
                        .random
                        .next_long() as u64
                ),
                f[6],
                "{line}"
            );
            assert_eq!(
                level
                    .entity::<LivingEntity>(id)
                    .unwrap()
                    .was_experience_consumed()
                    .to_string(),
                f[7],
                "{line}"
            );
        } else if f[0] == "death-xp-player" {
            level.rules_mut().keep_inventory = f[1] == "true";
            let id = level.next_entity_ref();
            let mut player = ServerPlayer::new(
                Uuid::new(1, 2),
                "xp".into(),
                1,
                Vec3::ZERO,
                if f[2] == "true" {
                    GameType::Spectator
                } else {
                    GameType::Survival
                },
            );
            player.experience_level = f[3].parse().unwrap();
            level
                .add_entity(id, EntityState::Player(Box::new(player)))
                .unwrap();
            let killer = match f[4] {
                "living" => {
                    let actor = level.next_entity_ref();
                    level
                        .add_entity(actor, EntityState::Cow(Box::default()))
                        .unwrap();
                    Some(actor)
                }
                "nonliving" => {
                    let actor = level.next_entity_ref();
                    level
                        .add_entity(
                            actor,
                            EntityState::PrimedTnt(Box::new(crate::entity::item::PrimedTnt::new(
                                20, 0,
                            ))),
                        )
                        .unwrap();
                    Some(actor)
                }
                "none" => None,
                value => panic!("{value}"),
            };
            assert_eq!(
                level
                    .entity_behavior(id)
                    .unwrap()
                    .get_experience_reward(id, killer, &mut level)
                    .unwrap()
                    .to_string(),
                f[5],
                "{line}"
            );
        } else {
            let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
            level.set_unported_behavior(UnportedBehavior::Strict);
            let id = level.next_entity_ref();
            level
                .add_entity(id, EntityState::Cow(Box::default()))
                .unwrap();
            if f[1] == "true" {
                level.remove_entity(id).unwrap();
            }
            if f[2] == "true" {
                level
                    .entity_mut::<LivingEntity>(id)
                    .unwrap()
                    .skip_drop_experience();
            }
            CONSUME.set(f[3] == "true");
            TRACE.with(|trace| trace.borrow_mut().clear());
            let source = level.damage_sources().generic();
            let result = DEATH.die(id, &source, &mut level);
            let status = match result {
                Ok(()) => "ok",
                Err(LevelError::UnsupportedEntityMethod(
                    EntityType::COW,
                    EntityMethod::DropAllDeathLoot,
                )) => "pending-loot",
                Err(e) => panic!("{line}: {e:?}"),
            };
            assert_eq!(status, f[4], "{line}");
            DEATH.die(id, &source, &mut level).unwrap();
            assert_eq!(TRACE.with(|trace| trace.borrow().join(";")), f[5], "{line}");
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
        checked += 1;
    }
    assert_eq!(checked, 136);
}

#[test]
fn actual_entity_die_dispatch_precedes_the_required_loot_boundary_and_is_not_repeated() {
    use crate::level::game_event::test_support::{self, Observation};
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Cow(Box::default()))
        .unwrap();
    test_support::observe(&mut level);
    let source = level.damage_sources().generic();
    let behavior = level.entity_behavior(id).unwrap();
    assert!(matches!(
        behavior.die(id, &source, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::COW,
            EntityMethod::DropAllDeathLoot
        ))
    ));
    let events = test_support::take(&mut level);
    assert_eq!(events.iter().filter(|event|matches!(event,Observation::GameEvent{event,source,..} if *event==game_event::Id::ENTITY_DIE&&*source==Some(id))).count(),1);
    assert!(level.entity::<LivingEntity>(id).unwrap().is_dead());
    behavior.die(id, &source, &mut level).unwrap();
    assert!(test_support::take(&mut level).is_empty());
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}
