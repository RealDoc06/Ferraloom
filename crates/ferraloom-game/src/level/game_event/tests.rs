use super::*;
use test_support::{Action, add};
const POS: BlockPos = BlockPos::new(8, 100, 8);
const SOURCE: Vec3 = Vec3::new(8.5, 100.5, 8.5);
fn level() -> Level {
    let mut level = crate::entity::tests::level(false);
    for (_, chunk) in level.region_mut().chunks.iter_mut() {
        chunk.loaded.set(true);
    }
    level.take_effects();
    level
}
fn register(level: &mut Level, id: usize) {
    let pos = level.region().game_event_listeners.tests.listeners[id].pos;
    level
        .register_listener(GameEventListenerRef::Test(id), pos)
        .unwrap();
}
fn emit(level: &mut Level) -> Result<(), LevelError> {
    level.game_event(GameEvent::STEP, SOURCE, None, None)
}
fn trace(level: &mut Level) -> String {
    std::mem::take(&mut level.region_mut().game_event_listeners.tests.trace).join(",")
}
fn actions(level: &mut Level, id: usize, actions: Vec<Action>) {
    level.region_mut().game_event_listeners.tests.listeners[id].actions = actions;
}

#[test]
fn immediate_delivery_precedes_stable_distance_order() {
    let mut level = level();
    for (pos, mode, label) in [
        (
            POS.relative(ferraloom_util::Direction::East, 4),
            DeliveryMode::ByDistance,
            "C",
        ),
        (POS, DeliveryMode::Unspecified, "A"),
        (
            POS.relative(ferraloom_util::Direction::East, 1),
            DeliveryMode::ByDistance,
            "D",
        ),
        (POS, DeliveryMode::Unspecified, "B"),
        (
            POS.relative(ferraloom_util::Direction::West, 1),
            DeliveryMode::ByDistance,
            "E",
        ),
    ] {
        let id = add(&mut level, pos, 32, mode, label);
        register(&mut level, id);
    }
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "A,B,D,E,C");
}
#[test]
fn block_distance_and_notification_section_bounds_match_native() {
    let mut level = level();
    let a = add(&mut level, POS, 8, DeliveryMode::Unspecified, "A");
    register(&mut level, a);
    let b = add(&mut level, POS, 7, DeliveryMode::Unspecified, "B");
    register(&mut level, b);
    level
        .game_event(GameEvent::STEP, Vec3::new(16.9, 100.5, 8.5), None, None)
        .unwrap();
    assert_eq!(trace(&mut level), "A");
    // Notification radius selects sections; it is not a second exact-distance
    // filter. This listener is twelve blocks from a radius-ten jukebox event.
    let c = add(
        &mut level,
        BlockPos::new(20, 100, 8),
        32,
        DeliveryMode::Unspecified,
        "C",
    );
    register(&mut level, c);
    level
        .game_event(GameEvent::JUKEBOX_PLAY, SOURCE, None, None)
        .unwrap();
    assert_eq!(trace(&mut level), "A,B,C");
    assert_eq!(GameEvent::JUKEBOX_PLAY.notification_radius(), 10);
    assert_eq!(GameEvent::SHRIEK.notification_radius(), 32);
}
#[test]
fn deferred_mutations_preserve_duplicate_listener_identity() {
    let mut level = level();
    let a = add(&mut level, POS, 16, DeliveryMode::Unspecified, "A");
    let b = add(&mut level, POS, 16, DeliveryMode::Unspecified, "B");
    let c = add(&mut level, POS, 16, DeliveryMode::Unspecified, "C");
    let d = add(&mut level, POS, 16, DeliveryMode::Unspecified, "D");
    for id in [a, b, b, c] {
        register(&mut level, id);
    }
    actions(
        &mut level,
        a,
        vec![Action::Unregister(b), Action::Register(d)],
    );
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "A,B,C");
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "A,B,C,D");
}
#[test]
fn later_sections_are_read_after_earlier_callbacks() {
    let mut level = level();
    let a = add(
        &mut level,
        BlockPos::new(8, 95, 8),
        16,
        DeliveryMode::Unspecified,
        "A",
    );
    let b = add(&mut level, POS, 16, DeliveryMode::Unspecified, "B");
    register(&mut level, a);
    actions(&mut level, a, vec![Action::Register(b)]);
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "A,B");
}
#[test]
fn nested_flush_reproduces_native_concurrent_modification_in_both_policies() {
    for policy in [
        crate::block::UnportedBehavior::Strict,
        crate::block::UnportedBehavior::Warn,
    ] {
        let mut level = level();
        level.set_unported_behavior(policy);
        let a = add(&mut level, POS, 16, DeliveryMode::Unspecified, "A");
        let b = add(&mut level, POS, 16, DeliveryMode::Unspecified, "B");
        let c = add(&mut level, POS, 16, DeliveryMode::Unspecified, "C");
        register(&mut level, a);
        register(&mut level, b);
        actions(
            &mut level,
            a,
            vec![Action::Register(c), Action::Reenter(SOURCE)],
        );
        assert_eq!(
            emit(&mut level),
            Err(LevelError::ConcurrentGameEventListenerModification)
        );
        assert_eq!(trace(&mut level), "A,A,B");
        emit(&mut level).unwrap();
        assert_eq!(trace(&mut level), "A,B,C");
    }
}
#[test]
fn failing_listener_stops_post_before_later_listeners_in_both_policies() {
    for policy in [
        crate::block::UnportedBehavior::Strict,
        crate::block::UnportedBehavior::Warn,
    ] {
        let mut level = level();
        level.set_unported_behavior(policy);
        let a = add(&mut level, POS, 16, DeliveryMode::Unspecified, "A");
        let b = add(&mut level, POS, 16, DeliveryMode::Unspecified, "B");
        register(&mut level, a);
        register(&mut level, b);
        actions(&mut level, a, vec![Action::Fail]);
        assert_eq!(
            emit(&mut level),
            Err(LevelError::ConcurrentGameEventListenerModification)
        );
        assert_eq!(trace(&mut level), "A");
    }
}
#[test]
fn detaching_chunk_preserves_current_visit_and_queued_delivery() {
    let mut level = level();
    let a = add(&mut level, POS, 16, DeliveryMode::Unspecified, "A");
    let b = add(&mut level, POS, 16, DeliveryMode::ByDistance, "B");
    register(&mut level, a);
    register(&mut level, b);
    actions(
        &mut level,
        a,
        vec![Action::Detach(ChunkPos::containing(POS))],
    );
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "A,B");
    emit(&mut level).unwrap();
    assert_eq!(trace(&mut level), "");
    // No old detached owner or registry survives its final in-flight post.
    assert!(
        level
            .region()
            .game_event_listeners
            .owners
            .values()
            .all(|owner| owner.attached)
    );
    assert!(
        level
            .region()
            .game_event_listeners
            .registries
            .values()
            .all(|registry| registry.attached)
    );
}
#[test]
fn absent_chunks_are_not_loaded_for_game_event_delivery() {
    let mut level = level();
    let chunks = level.region().chunks.values().count();
    level
        .game_event(
            GameEvent::SHRIEK,
            Vec3::new(10000.0, 100.0, 10000.0),
            None,
            None,
        )
        .unwrap();
    assert_eq!(level.region().chunks.values().count(), chunks);
    assert!(level.region().game_event_listeners.owners.is_empty());
}
