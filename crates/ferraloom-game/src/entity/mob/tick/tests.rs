use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{EntityState, LivingEntity},
    level::LevelError,
};
const VECTORS: &str = include_str!("../../../../tests/java/entity/mob_foundations/vectors.tsv");

#[test]
fn native_server_ai_step_reads_cadence_after_sensing_and_captures_it_for_both_selectors() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("mob-tick\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::persistence_level();
        let entity = level.next_entity_ref();
        level
            .add_entity(entity, EntityState::Cow(Box::default()))
            .unwrap();
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.tick_count = f[1].parse().unwrap();
        base.network_id = f[2].parse().unwrap();
        level
            .entity_mut::<LivingEntity>(entity)
            .unwrap()
            .set_no_action_time(i32::MAX);
        let mut trace = Vec::new();
        run_steps(entity, &mut level, |step, entity, level| {
            match step {
                Step::Sensing => {
                    trace.push(format!(
                        "sensing:{}",
                        level.entity::<LivingEntity>(entity)?.no_action_time()
                    ));
                    if f[3] == "true" {
                        let base = level.entity_mut::<EntityData>(entity)?;
                        base.tick_count = 3;
                        base.network_id = 2;
                    }
                }
                Step::Target(reduced) | Step::Goal(reduced) => {
                    let label = if matches!(step, Step::Target(_)) {
                        "target"
                    } else {
                        "goal"
                    };
                    trace.push(if reduced {
                        format!("{label}:running:false")
                    } else {
                        format!("{label}:full")
                    });
                    if matches!(step, Step::Target(_)) && f[4] == "true" {
                        level.entity_mut::<EntityData>(entity)?.tick_count = 0;
                    }
                }
                Step::Navigation => trace.push("navigation".into()),
                Step::Custom => trace.push("custom".into()),
                Step::Move => trace.push("move".into()),
                Step::Look => trace.push("look".into()),
                Step::Jump => trace.push("jump".into()),
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(trace.join(";"), f[5], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 128);
}
#[test]
fn pending_sensing_stops_strict_before_selectors_and_keeps_the_native_counter_increment() {
    let mut level = crate::entity::tests::persistence_level();
    level.set_unported_behavior(UnportedBehavior::Strict);
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Cow(Box::default()))
        .unwrap();
    assert!(
        matches!(server_ai_step(id,&mut level),Err(LevelError::UnsupportedMobAi{entity,entity_type:ferraloom_data::EntityType::COW,operation:ai::AiOperation::Sensing}) if entity==id)
    );
    assert_eq!(
        level.entity::<LivingEntity>(id).unwrap().no_action_time(),
        1
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}
