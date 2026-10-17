//! Actual player ticks through the normal, frozen, stepped and refrozen states.

use super::*;
use crate::data::player_server::test::{
    PlayerFixture, close_fixture, player_fixture, stop_chunk_workers,
};
use std::sync::atomic::AtomicUsize;
use uuid::Uuid;

async fn fixture(position: Vector3<f64>) -> PlayerFixture {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    fixture.player.get_entity().set_pos(position);
    fixture.player.set_client_loaded(true);
    fixture
}

/// Observe the real push filter before cramming without sampling its random roll.
/// The living entity and the default `EntityBase::push` supply all push behavior.
struct PushWitness {
    living: LivingEntity,
    pushable_checks: AtomicUsize,
}

impl EntityBase for PushWitness {
    fn get_entity(&self) -> &Entity {
        &self.living.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        Some(&self.living)
    }

    fn is_pushable(&self) -> bool {
        self.pushable_checks.fetch_add(1, Relaxed);
        self.living.is_pushable()
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn tick_with_push_witness(fixture: &PlayerFixture, witness: &PushWitness, simulation_tick: bool) {
    let player = &fixture.player;
    let player_entity = player.get_entity();
    let target = witness.get_entity();
    assert_eq!(
        fixture.server.tick_rate_manager.runs_normally(),
        simulation_tick
    );
    assert!(witness.living.is_pushable());
    assert!(player.is_pushable());
    assert!(
        target
            .bounding_box
            .load()
            .intersects(&player_entity.bounding_box.load())
    );
    target.velocity.store(Vector3::default());
    player_entity.velocity.store(if simulation_tick {
        Vector3::default()
    } else {
        Vector3::new(1.0, 0.0, 0.0)
    });
    witness.pushable_checks.store(0, Relaxed);
    let ticks = player.tick_counter.load(Relaxed);

    player.tick(&fixture.server);

    assert_eq!(player.tick_counter.load(Relaxed), ticks + 1);
    let target_velocity = target.velocity.load();
    let player_velocity = player_entity.velocity.load();
    if simulation_tick {
        assert!(witness.pushable_checks.load(Relaxed) > 0);
        assert!(
            target_velocity.x > 0.0,
            "overlapping living entity is pushed"
        );
        assert!(player_velocity.x < 0.0, "player receives the opposite push");
        assert_eq!(target_velocity.y, 0.0);
        assert_eq!(target_velocity.z, 0.0);
        assert_eq!(player_velocity.y, 0.0);
        assert_eq!(player_velocity.z, 0.0);
    } else {
        // The filter precedes the cramming branch. Zero calls proves it was
        // skipped regardless of the random roll or the player's hurt cooldown.
        assert_eq!(witness.pushable_checks.load(Relaxed), 0);
        assert_eq!(target_velocity, Vector3::default());
        assert_eq!(
            player_velocity,
            Vector3::new(0.91, 0.0, 0.0),
            "frozen player friction still runs"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn player_push_only_runs_on_normal_and_stepped_ticks() {
    let fixture = fixture(Vector3::new(0.5, 65.0, 0.5)).await;
    let world = fixture.player.world();
    let witness = Arc::new(PushWitness {
        living: LivingEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.75, 65.0, 0.5),
            &EntityType::COW,
        )),
        pushable_checks: AtomicUsize::new(0),
    });
    assert!(world.spawn_entity(witness.clone()));
    assert!(matches!(
        world.get_game_rule(&GameRule::MaxEntityCramming),
        GameRuleValue::Int(24)
    ));

    let manager = &fixture.server.tick_rate_manager;
    assert!(!manager.is_frozen());
    assert!(manager.runs_normally());
    manager.tick();
    tick_with_push_witness(&fixture, &witness, true);

    manager.set_frozen(&fixture.server, true);
    manager.tick();
    assert!(manager.is_frozen());
    tick_with_push_witness(&fixture, &witness, false);

    assert!(manager.step_game_if_paused(&fixture.server, 1));
    assert!(manager.is_stepping_forward());
    manager.tick();
    assert!(manager.is_frozen(), "stepping keeps the frozen flag set");
    assert!(!manager.is_stepping_forward(), "the sole step is consumed");
    tick_with_push_witness(&fixture, &witness, true);

    manager.tick();
    assert!(manager.is_frozen());
    assert!(!manager.is_stepping_forward());
    tick_with_push_witness(&fixture, &witness, false);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frozen_player_overlapping_twenty_four_no_ai_dragons_keeps_health() {
    let fixture = fixture(Vector3::new(0.0, 0.0, 2.0)).await;
    let world = fixture.player.world();
    let manager = &fixture.server.tick_rate_manager;
    manager.set_frozen(&fixture.server, true);
    manager.tick();
    assert!(!manager.runs_normally());
    assert!(matches!(
        world.get_game_rule(&GameRule::MaxEntityCramming),
        GameRuleValue::Int(24)
    ));
    for _ in 0..24 {
        let dragon = crate::entity::r#type::from_type(
            &EntityType::ENDER_DRAGON,
            Vector3::new(0.0, 0.0, 0.0),
            &world,
            Uuid::new_v4(),
        );
        dragon.get_mob().unwrap().get_mob_entity().set_no_ai(true);
        assert!(world.spawn_entity(dragon));
    }

    let player = &fixture.player;
    let living = &player.living_entity;
    let player_box = living.entity.bounding_box.load();
    let crowded = world
        .get_all_at_box(&player_box)
        .into_iter()
        .filter(|entity| entity.get_entity().entity_id != player.entity_id())
        .collect::<Vec<_>>();
    assert_eq!(crowded.len(), 24);
    for dragon in &crowded {
        let entity = dragon.get_entity();
        assert_eq!(entity.entity_type, &EntityType::ENDER_DRAGON);
        assert_eq!(
            entity.bounding_box.load().min,
            Vector3::new(-8.0, 0.0, -8.0)
        );
        assert_eq!(entity.bounding_box.load().max, Vector3::new(8.0, 8.0, 8.0));
        assert!(entity.no_physics.load(Relaxed));
        assert!(dragon.get_mob().unwrap().get_mob_entity().is_no_ai());
        assert!(dragon.is_pushable());
        assert!(!dragon.is_passenger());
    }
    assert_eq!(player.gamemode.load(), GameMode::Survival);
    assert_eq!(living.health.load(), 20.0);
    assert_eq!(living.hurt_cooldown.load(Relaxed), 0);
    let ticks = player.tick_counter.load(Relaxed);

    // One real frozen player tick in the reported geometry. The other test
    // proves branch exclusion deterministically; this is not a random-roll loop.
    player.tick(&fixture.server);

    assert_eq!(player.tick_counter.load(Relaxed), ticks + 1);
    assert_eq!(living.health.load(), 20.0);
    assert_eq!(living.last_damage_taken.load(), 0.0);
    assert_eq!(living.hurt_cooldown.load(Relaxed), 0);
    assert!(!living.dead.load(Relaxed));
    assert_eq!(living.entity.pos.load(), Vector3::new(0.0, 0.0, 2.0));
    close_fixture(&fixture).await;
}
