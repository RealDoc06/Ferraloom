use super::*;
use crate::{
    Behaviors,
    entity::{EntityState, animal::Cow, synched_data},
};
use ferraloom_nbt::Tag;

fn bits(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
fn vector(text: &str) -> Vec3 {
    let parts: Vec<_> = text
        .split(',')
        .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
        .collect();
    Vec3::new(parts[0], parts[1], parts[2])
}
fn event(entity: EntityRef, text: String, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    let previous = base
        .custom_data
        .get_string("events")
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    base.custom_data.insert(
        "events",
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
    fn is_local_instance_authoritative(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(level
            .entity::<EntityData>(entity)?
            .custom_data
            .get_boolean_or("simulate", true))
    }
    fn can_freeze(&self, _: EntityRef, _: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn server_ai_step(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        event(entity, "ai".into(), level)
    }
    fn travel(&self, entity: EntityRef, input: Vec3, level: &mut Level) -> Result<(), LevelError> {
        let velocity = level.entity::<EntityData>(entity)?.velocity;
        event(
            entity,
            format!("travel:{}:{}", bits(input), bits(velocity)),
            level,
        )
    }
    fn apply_effects_from_blocks(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        event(entity, "blocks".into(), level)
    }
    fn push_entities(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _: &TickRateManager,
    ) -> Result<(), LevelError> {
        event(entity, "push".into(), level)
    }
}
const BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &Probe);

#[test]
fn native_nonplayer_thresholds_no_ai_and_immobility_preserve_phase_order() {
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/entity/living_tick/vectors.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level_with_behaviors(true, &BEHAVIORS);
        let entity = level.next_entity_ref();
        let mut cow = Cow::default();
        let state = cow.as_living_mut();
        state.set_pos(Vec3::new(8.5, 64.0, 8.5));
        state.set_delta_movement(vector(f[2]));
        state.set_health(if f[4] == "true" { 0.0 } else { 10.0 });
        state.xxa = 0.75;
        state.yya = -0.125;
        state.zza = -0.3;
        state
            .entity_data
            .set(synched_data::MOB_FLAGS, if f[3] == "true" { 1 } else { 0 });
        state
            .custom_data
            .insert("simulate", Tag::Byte(i8::from(f[5] == "true")));
        level
            .add_entity(entity, EntityState::Cow(Box::new(cow)))
            .unwrap();
        ai_step(&Probe, entity, &mut level, &TickRateManager::default()).unwrap();
        let state = level.entity::<LivingEntity>(entity).unwrap();
        let result = format!(
            "{}\t{:x},{:x},{:x}\t{}",
            bits(state.velocity),
            state.xxa.to_bits(),
            state.yya.to_bits(),
            state.zza.to_bits(),
            state.custom_data.get_string("events").unwrap()
        );
        assert_eq!(result, f[6..].join("\t"), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 48);
}

#[test]
fn live_mob_ai_is_required_after_input_and_before_jump_or_travel() {
    let mut level = crate::entity::tests::level(true);
    let entity = level.next_entity_ref();
    let mut cow = Cow::default();
    cow.as_living_mut().xxa = 0.75;
    level
        .add_entity(entity, EntityState::Cow(Box::new(cow)))
        .unwrap();
    assert_eq!(
        ai_step(
            &crate::entity::animal::CowBehavior,
            entity,
            &mut level,
            &TickRateManager::default()
        ),
        Err(LevelError::UnsupportedMobAi {
            entity,
            entity_type: EntityType::COW,
            operation: crate::entity::ai::AiOperation::Sensing
        })
    );
    let state = level.entity::<LivingEntity>(entity).unwrap();
    assert_eq!(state.xxa, 0.75 * 0.98);
    assert_eq!(state.no_action_time, 1);
}
