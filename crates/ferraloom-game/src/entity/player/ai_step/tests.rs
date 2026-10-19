use super::*;
use crate::entity::{
    EntityState,
    player::{GameType, PlayerBehavior},
};
use ferraloom_nbt::Tag;
use ferraloom_util::{BlockPos, Uuid};
fn vec_bits(v: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        v.x.to_bits(),
        v.y.to_bits(),
        v.z.to_bits()
    )
}
fn vector(value: &str) -> Vec3 {
    let mut v = value
        .split(',')
        .map(|v| f64::from_bits(u64::from_str_radix(v, 16).unwrap()));
    Vec3::new(v.next().unwrap(), v.next().unwrap(), v.next().unwrap())
}
fn event(entity: EntityRef, text: String, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity_mut::<EntityData>(entity)?;
    let previous = state
        .custom_data
        .get_string("ai_events")
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    state.custom_data.insert(
        "ai_events",
        Tag::String(
            if previous.is_empty() {
                text
            } else {
                format!("{previous};{text}")
            }
            .into(),
        ),
    );
    Ok(())
}
struct Probe;
impl Entity for Probe {
    fn can_freeze(&self, _: EntityRef, _: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }

    fn can_simulate_movement(&self, _: EntityRef, _: &Level) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn is_affected_by_fluids(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        PlayerBehavior.is_affected_by_fluids(entity, level)
    }
    fn jump_from_ground(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        event(entity, "jump".into(), level)?;
        living::jump::jump_from_ground(self, entity, level)
    }
    fn travel(&self, entity: EntityRef, input: Vec3, level: &mut Level) -> Result<(), LevelError> {
        let p = level.entity::<LivingEntity>(entity)?;
        let text = format!(
            "travel:{}:{}:{:x}:{}",
            vec_bits(input),
            vec_bits(p.velocity),
            p.fall_distance.to_bits(),
            p.no_jump_delay
        );
        event(entity, text, level)
    }
    fn apply_effects_from_blocks(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        event(entity, "blocks".into(), level)?;
        level.entity_mut::<EntityData>(entity)?.set_ticks_frozen(42);
        Ok(())
    }
    fn push_entities(
        &self,
        entity: EntityRef,
        level: &mut Level,
        tick_rate: &TickRateManager,
    ) -> Result<(), LevelError> {
        let p = level.entity::<LivingEntity>(entity)?;
        let text = format!(
            "push:{}:{:x}",
            p.ticks_frozen(),
            p.attributes
                .value(Attribute::MOVEMENT_SPEED)
                .unwrap()
                .to_bits()
        );
        let runs = tick_rate.runs_normally();
        level
            .entity_mut::<EntityData>(entity)?
            .custom_data
            .insert("runs_normally", Tag::Byte(i8::from(runs)));
        event(entity, text, level)
    }
}
#[test]
fn native_player_ai_step_preserves_input_jump_travel_block_freeze_push_and_tail_order() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("player-ai-step\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::level(true);
        let id = level.next_entity_ref();
        let mut p = ServerPlayer::new(
            Uuid::new(3, 4),
            "ai".into(),
            level.next_entity_id(),
            Vec3::new(8.5, 64.0, 8.5),
            GameType::Survival,
        );
        p.jumping = row[2].parse().unwrap();
        p.on_ground = row[3].parse().unwrap();
        p.no_jump_delay = row[4].parse().unwrap();
        p.abilities.flying = row[5].parse().unwrap();
        if row[6] == "true" {
            p.set_health(0.0);
        }
        if row[7] == "true" {
            p.entity_data
                .set(synched_data::SLEEPING_POS, Some(BlockPos::new(8, 64, 8)));
        }
        p.velocity = vector(row[8]);
        p.xxa = 0.75;
        p.yya = -0.125;
        p.zza = -0.3;
        p.jump_trigger_time = 2;
        p.set_y_rot(37.25);
        p.fall_distance = 5.0;
        level
            .add_entity(id, EntityState::Player(Box::new(p)))
            .unwrap();
        let rate = TickRateManager::default();
        ai_step(&Probe, id, &mut level, &rate).unwrap();
        let p = level.entity::<ServerPlayer>(id).unwrap();
        let actual = format!(
            "{}\t{:x},{:x},{:x}\t{}\t{}\t{}\t{:x}\t{:x}\t{:x}\t{}",
            vec_bits(p.velocity),
            p.xxa.to_bits(),
            p.yya.to_bits(),
            p.zza.to_bits(),
            p.jumping,
            p.no_jump_delay,
            p.jump_trigger_time,
            p.y_head_rot.to_bits(),
            p.speed.to_bits(),
            p.fall_distance.to_bits(),
            p.custom_data.get_string("ai_events").unwrap()
        );
        assert_eq!(actual, row[9..].join("\t"), "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 32);
}
#[test]
fn auto_spin_uses_the_native_predecrement_guard_and_stops_before_push_in_strict() {
    use crate::block::UnportedBehavior;
    for initial in [0, 1, 2] {
        let mut level = crate::entity::tests::level(true);
        let id = level.next_entity_ref();
        let mut p = ServerPlayer::new(
            Uuid::new(3, 4),
            "spin".into(),
            1,
            Vec3::new(8.5, 64.0, 8.5),
            GameType::Survival,
        );
        p.auto_spin_attack_ticks = initial;
        level
            .add_entity(id, EntityState::Player(Box::new(p)))
            .unwrap();
        level.set_unported_behavior(UnportedBehavior::Strict);
        let mut rate = TickRateManager::default();
        rate.set_frozen(true);
        rate.tick();
        let result = ai_step(&Probe, id, &mut level, &rate);
        let p = level.entity::<ServerPlayer>(id).unwrap();
        assert_eq!(
            p.auto_spin_attack_ticks,
            if initial > 0 { initial - 1 } else { initial }
        );
        if initial > 0 {
            assert_eq!(
                result,
                Err(LevelError::UnsupportedEntityMethod(
                    EntityType::PLAYER,
                    EntityMethod::CheckAutoSpinAttack
                ))
            );
            assert!(p.custom_data.get("runs_normally").is_none());
        } else {
            result.unwrap();
            assert!(!p.custom_data.get_boolean_or("runs_normally", true));
        }
    }
}
