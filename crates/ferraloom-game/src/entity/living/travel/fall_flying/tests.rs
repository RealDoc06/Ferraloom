use super::super::tests::{bits, double, float, rows, vector};
use super::*;
use crate::entity::{
    EntityState, LivingEntity,
    damage::DamageSource,
    effect::MobEffectInstance,
    player::{GameType, ServerPlayer},
    synched_data,
};
use ferraloom_data::{
    Block,
    registries::{
        attribute::Id as Attribute, mob_effect::Id as MobEffect, sound_event::Id as Sound,
    },
};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{BlockPos, Uuid};

fn state(player: bool, level: &Level, position: Vec3) -> EntityState {
    if player {
        EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(3, 4),
            "flight".into(),
            level.next_entity_id().unwrap(),
            position,
            GameType::Survival,
        )))
    } else {
        EntityState::Cow(Box::default())
    }
}
fn slow_falling(living: &mut LivingEntity) {
    let effect = MobEffectInstance::new(MobEffect::SLOW_FALLING, 100, 0, false, true, true, None);
    let nbt = Tag::List(ListTag(vec![effect.to_component().to_nbt().unwrap()]));
    assert!(living.active_effects.load(Some(&nbt)).is_empty());
}

#[test]
fn native_fall_flying_lift_pitch_and_gravity_preserve_double_bits() {
    let mut checked = 0;
    for row in rows("fall-flying-update\t") {
        let mut level = crate::entity::tests::level(false);
        let entity = level.next_entity_ref();
        let mut state = state(true, &level, Vec3::ZERO);
        let living = state.as_living_mut().unwrap();
        living.yaw = float(row[2]);
        living.pitch = float(row[3]);
        living.velocity = vector(row[4]);
        living
            .attributes
            .get_instance(Attribute::GRAVITY)
            .unwrap()
            .set_base_value(double(row[5]));
        living
            .entity_data
            .set(synched_data::NO_GRAVITY, row[6].parse().unwrap());
        if row[7] == "true" {
            slow_falling(living);
        }
        level.add_entity(entity, state).unwrap();
        let behavior = level.entity_behavior(entity).unwrap();
        let after = update_fall_flying_movement(behavior, entity, vector(row[4]), &level).unwrap();
        assert_eq!(bits(after), row[8], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 40);
}

#[test]
fn native_fall_flying_body_uses_collision_and_climbable_stop_metadata_order() {
    let mut checked = 0;
    for row in rows("travel-fall-flying\t") {
        let mut level = crate::entity::tests::level(true);
        if row[11] == "true" {
            level
                .set_block_state(BlockPos::new(8, 64, 8), Block::LADDER.default_state())
                .unwrap();
        }
        let entity = level.next_entity_ref();
        let mut state = state(row[1] == "player", &level, vector(row[3]));
        let living = state.as_living_mut().unwrap();
        living.set_pos(vector(row[3]));
        living.velocity = vector(row[4]);
        living.yaw = float(row[6]);
        living.pitch = float(row[7]);
        living
            .attributes
            .get_instance(Attribute::GRAVITY)
            .unwrap()
            .set_base_value(double(row[8]));
        living
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
            .unwrap()
            .set_base_value(0.17);
        living.speed = 0.17;
        living
            .entity_data
            .set(synched_data::NO_GRAVITY, row[9].parse().unwrap());
        if row[10] == "true" {
            slow_falling(living);
        }
        living.set_shared_flag(7, row[12].parse().unwrap());
        let _ = living.entity_data.pack_dirty();
        level.add_entity(entity, state).unwrap();
        let behavior = level.entity_behavior(entity).unwrap();
        travel_fall_flying(behavior, entity, vector(row[5]), &mut level).unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.position), row[13], "position {row:?}");
        assert_eq!(bits(base.velocity), row[14], "velocity {row:?}");
        let flags: String = [
            base.on_ground,
            base.horizontal_collision,
            base.vertical_collision,
            base.vertical_collision_below,
            base.minor_horizontal_collision,
        ]
        .map(|v| if v { '1' } else { '0' })
        .into_iter()
        .collect();
        assert_eq!(flags, row[15], "flags {row:?}");
        assert_eq!(
            format!("{:x}", base.fall_distance.to_bits()),
            row[16],
            "fall {row:?}"
        );
        assert_eq!(base.shared_flag(7).to_string(), row[17], "flight {row:?}");
        assert_eq!(
            base.entity_data.is_dirty().to_string(),
            row[18],
            "metadata {row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 24);
}

struct HitReceiver(bool);
fn record(entity: EntityRef, event: String, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    let mut events = match base.saved.remove("events") {
        Some(Tag::List(list)) => list,
        _ => ListTag(Vec::new()),
    };
    events.0.push(Tag::String(event.into()));
    base.saved.insert("events", Tag::List(events));
    Ok(())
}
impl Entity for HitReceiver {
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<EntityRef>,
    ) {
        crate::entity::animal::CowBehavior.trace_entity_refs(state, references);
    }

    fn fall_sounds(&self) -> crate::entity::living::FallSounds {
        if self.0 {
            crate::entity::player::PlayerBehavior.fall_sounds()
        } else {
            crate::entity::animal::CowBehavior.fall_sounds()
        }
    }
    fn play_sound(
        &self,
        entity: EntityRef,
        sound: Sound,
        volume: f32,
        pitch: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        record(
            entity,
            format!(
                "sound:{}:{:x}:{:x}",
                sound.name(),
                volume.to_bits(),
                pitch.to_bits()
            ),
            level,
        )
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        record(
            entity,
            format!(
                "hurt:{}:{:x}",
                source.damage_type().message_id,
                damage.to_bits()
            ),
            level,
        )?;
        Ok(false)
    }
}

#[test]
fn native_fall_flying_hits_select_sound_before_typed_damage_and_keep_float_thresholds() {
    let mut checked = 0;
    for row in rows("fall-flying-hit\t") {
        let mut level = crate::entity::tests::level(false);
        let entity = level.next_entity_ref();
        let mut state = state(row[1] == "player", &level, Vec3::ZERO);
        state.as_entity_mut().horizontal_collision = row[3].parse().unwrap();
        level.add_entity(entity, state).unwrap();
        handle_fall_flying_collisions(
            &HitReceiver(row[1] == "player"),
            entity,
            double(row[4]),
            double(row[5]),
            &mut level,
        )
        .unwrap();
        let actual = match level
            .entity::<EntityData>(entity)
            .unwrap()
            .saved
            .get("events")
        {
            Some(Tag::List(events)) => events
                .0
                .iter()
                .map(|tag| match tag {
                    Tag::String(value) => value.to_string_lossy(),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>()
                .join(","),
            _ => "-".to_owned(),
        };
        assert_eq!(actual, row[6], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 24);
}
