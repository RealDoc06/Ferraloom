use super::*;
use crate::entity::{
    EntityData, EntityState, RemovalReason,
    animal::Cow,
    player::{GameType, PlayerBehavior, ServerPlayer},
};
use ferraloom_data::{BlockState, EntityType};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_util::{BlockPos, Uuid, Vec3};
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn fixture(terrain: bool) -> (Level, EntityRef, EntityRef) {
    let mut level = if terrain {
        crate::entity::tests::level(false)
    } else {
        crate::entity::tests::persistence_level()
    };
    let entity = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(7, 8),
        "combat".into(),
        level.next_entity_id(),
        Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    let attacker = level.next_entity_ref();
    let cow = Cow::with_parent(level.create_entity_data(EntityType::COW));
    level
        .add_entity(attacker, EntityState::Cow(Box::new(cow)))
        .unwrap();
    level.take_effects();
    (level, entity, attacker)
}
fn entries(tracker: &CombatTracker) -> String {
    let entries: Vec<_> = tracker
        .entries
        .iter()
        .map(|entry| {
            format!(
                "{},{:x},{},{:x}",
                if entry.source.causing_entity().is_some() {
                    "living"
                } else {
                    "env"
                },
                entry.damage.to_bits(),
                entry.fall_location.map(FallLocation::id).unwrap_or("-"),
                entry.fall_distance.to_bits()
            )
        })
        .collect();
    if entries.is_empty() {
        "-".into()
    } else {
        entries.join("/")
    }
}
fn state(entity: EntityRef, level: &Level) -> String {
    let living = level.entity::<LivingEntity>(entity).unwrap();
    let tracker = living.combat_tracker();
    format!(
        "{},{},{},{},{},{};{}",
        tracker.taking_damage,
        tracker.in_combat,
        tracker.last_damage_time,
        tracker.combat_start_time,
        tracker.combat_end_time,
        tracker.combat_duration(living.tick_count),
        entries(tracker)
    )
}
fn event(name: &str, entity: EntityRef, level: &mut Level) {
    let text = format!("{name}:{}", state(entity, level));
    let data = &mut level.entity_mut::<EntityData>(entity).unwrap().custom_data;
    if !data.contains_key("events") {
        data.insert("events", Tag::List(ListTag::default()));
    }
    let Tag::List(events) = data.get_mut("events").unwrap() else {
        panic!("event list")
    };
    events.0.push(Tag::String(text.into()));
}
struct Probe;
impl Entity for Probe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        state
            .as_living()
            .unwrap()
            .trace_damage_entity_refs(references);
    }
    fn is_alive(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        PlayerBehavior.is_alive(entity, level)
    }
    fn on_enter_combat(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        event("enter", entity, level);
        if level
            .entity::<EntityData>(entity)?
            .custom_data
            .get_boolean_or("reentrant", false)
        {
            record_damage(self, entity, level.damage_sources().generic(), 2.5, level)?;
            event("entered", entity, level);
        }
        Ok(())
    }
    fn on_leave_combat(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        event("leave", entity, level);
        if level
            .entity::<EntityData>(entity)?
            .custom_data
            .get_boolean_or("reentrant", false)
        {
            record_damage(self, entity, level.damage_sources().generic(), 3.5, level)?;
            event("left", entity, level);
        }
        Ok(())
    }
}
#[test]
fn native_combat_timeouts_wrapping_health_and_reentrant_callbacks_keep_entry_order() {
    let mut checked = 0;
    for reentrant in [false, true] {
        let (mut level, entity, attacker) = fixture(false);
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .custom_data
            .insert("reentrant", Tag::Byte(i8::from(reentrant)));
        for row in VECTORS
            .lines()
            .filter(|row| row.starts_with(&format!("combat-tracker\t{reentrant}\t")))
        {
            let f: Vec<_> = row.split('\t').collect();
            let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
            player.tick_count = f[4].parse().unwrap();
            player.set_health(f[5].parse().unwrap());
            player.fall_distance = 5.25;
            player.custom_data.remove("events");
            if f[3] == "check" {
                recheck_status(&Probe, entity, &mut level).unwrap();
            } else {
                let source = if f[3] == "env" {
                    level.damage_sources().generic()
                } else {
                    level.damage_sources().mob_attack(attacker)
                };
                record_damage(&Probe, entity, source, 4.0, &mut level).unwrap();
            }
            assert_eq!(state(entity, &level), f[6], "{row}");
            let output = level
                .entity::<EntityData>(entity)
                .unwrap()
                .custom_data
                .get_list("events")
                .map(|events| {
                    events
                        .iter()
                        .map(|value| value.as_string().unwrap().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("|")
                });
            assert_eq!(output.as_deref().unwrap_or("-"), f[7], "{row}");
            checked += 1;
        }
    }
    assert_eq!(checked, 30);
}
#[test]
fn native_fall_location_prefers_retained_climbing_over_water_and_preserves_float_narrowing() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("combat-fall-location\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity, _) = fixture(true);
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.was_touching_water = f[1].parse().unwrap();
        player.fall_distance = f64::MAX;
        let block: i32 = f[2].parse().unwrap();
        if block >= 0 {
            let pos = BlockPos::new(8, 64, 8);
            level
                .entity::<LivingEntity>(entity)
                .unwrap()
                .last_climbable_pos
                .set(Some(pos));
            level
                .set_block_state(pos, BlockState::from_id(block as u16).unwrap())
                .unwrap();
        }
        record_damage(
            &Probe,
            entity,
            level.damage_sources().fall(),
            1.0,
            &mut level,
        )
        .unwrap();
        assert_eq!(
            entries(
                level
                    .entity::<LivingEntity>(entity)
                    .unwrap()
                    .combat_tracker()
            ),
            f[3],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 20);
}
#[test]
fn player_combat_packets_are_emitted_at_the_enter_and_leave_callbacks() {
    use crate::level::effects::LevelEffect;
    use ferraloom_protocol::play::clientbound as cb;
    let (mut level, entity, attacker) = fixture(false);
    level.entity_mut::<EntityData>(entity).unwrap().tick_count = 17;
    record_damage(
        &PlayerBehavior,
        entity,
        level.damage_sources().mob_attack(attacker),
        3.0,
        &mut level,
    )
    .unwrap();
    level.entity_mut::<EntityData>(entity).unwrap().tick_count = 318;
    recheck_status(&PlayerBehavior, entity, &mut level).unwrap();
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .map(|effect| {
            let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                panic!("combat packet")
            };
            assert_eq!(recipients, vec![entity]);
            *packet
        })
        .collect();
    assert!(
        matches!(packets.as_slice(),[cb::Packet::PlayerCombatEnter(_),cb::Packet::PlayerCombatEnd(end)] if end.duration.0==301)
    );
}
#[test]
fn combat_history_retains_removed_sources_until_the_native_status_clear() {
    let (mut level, entity, attacker) = fixture(false);
    record_damage(
        &PlayerBehavior,
        entity,
        level.damage_sources().mob_attack(attacker),
        3.0,
        &mut level,
    )
    .unwrap();
    level
        .remove_entity_with_reason(attacker, RemovalReason::Discarded)
        .unwrap();
    level.finish_entity_management();
    assert!(level.entity_state(attacker).is_ok());
    level
        .entity_mut::<ServerPlayer>(entity)
        .unwrap()
        .set_health(0.0);
    recheck_status(&PlayerBehavior, entity, &mut level).unwrap();
    level.finish_entity_management();
    assert!(matches!(
        level.entity_state(attacker),
        Err(LevelError::MissingEntity(_))
    ));
}
#[test]
fn required_fall_location_read_does_not_append_a_damage_entry() {
    let (mut level, entity, _) = fixture(false);
    level
        .entity::<LivingEntity>(entity)
        .unwrap()
        .last_climbable_pos
        .set(Some(BlockPos::ZERO));
    assert_eq!(
        record_damage(
            &PlayerBehavior,
            entity,
            level.damage_sources().fall(),
            3.0,
            &mut level
        ),
        Err(LevelError::MissingChunk(ferraloom_util::ChunkPos::ZERO))
    );
    assert!(
        level
            .entity::<LivingEntity>(entity)
            .unwrap()
            .combat_tracker()
            .entries()
            .is_empty()
    );
}
