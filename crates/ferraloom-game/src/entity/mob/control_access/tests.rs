use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        EntityData, EntityState, Mob,
        animal::CowBehavior,
        player::{GameType, ServerPlayer},
    },
};
use ferraloom_data::EntityType;
use ferraloom_util::{Uuid, Vec3};
use std::cell::{Cell, RefCell};
const VECTORS: &str = include_str!("../../../../tests/java/entity/mob_accessors/vectors.tsv");
thread_local! {
    static REMOVE:Cell<bool>=const{Cell::new(false)};
    static TRACE:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};
}
struct Probe;
impl Entity for Probe {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        CowBehavior.trace_entity_refs(state, refs);
    }
    fn set_zza(&self, entity: EntityRef, value: f32, level: &mut Level) -> Result<(), LevelError> {
        let living = level.entity::<LivingEntity>(entity)?;
        TRACE.with(|trace| {
            trace.borrow_mut().push(format!(
                "zza:{:x}:{:x}",
                living.speed.to_bits(),
                living.zza.to_bits()
            ))
        });
        if REMOVE.get() {
            level.remove_entity(entity)?;
            level.finish_entity_management();
        }
        level.entity_mut::<Mob>(entity)?.parent.zza = value;
        Ok(())
    }
}
static PROBE: Probe = Probe;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &PROBE);
fn values(living: &LivingEntity) -> String {
    format!(
        "{:x},{:x},{},{:x},{:x}",
        living.speed.to_bits(),
        living.zza.to_bits(),
        living.jumping,
        living.y_head_rot.to_bits(),
        living.y_body_rot.to_bits()
    )
}
#[test]
fn native_speed_parent_write_is_visible_to_zza_and_holds_the_receiver_through_removal() {
    let mut checked = 0;
    for row in VECTORS.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let value = f32::from_bits(u32::from_str_radix(f[1], 16).unwrap());
        let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let id = level.next_entity_ref();
        let state = if f[0] == "mob-accessor" {
            EntityState::Cow(Box::default())
        } else {
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "living".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            )))
        };
        level.add_entity(id, state).unwrap();
        let living = level.entity_mut::<LivingEntity>(id).unwrap();
        living.speed = 0.25;
        living.zza = -0.5;
        living.jumping = true;
        REMOVE.set(f[0] == "mob-accessor" && f[2] == "true");
        TRACE.with(|trace| trace.borrow_mut().clear());
        let behavior = level.entity_behavior(id).unwrap();
        behavior.set_speed(id, value, &mut level).unwrap();
        behavior.set_jumping(id, value != 0.0, &mut level).unwrap();
        behavior.set_y_head_rot(id, value, &mut level).unwrap();
        behavior.set_y_body_rot(id, -value, &mut level).unwrap();
        let actual = if f[0] == "mob-accessor" {
            behavior.set_xxa(id, value, &mut level).unwrap();
            behavior.set_yya(id, -value, &mut level).unwrap();
            let living = level.entity::<LivingEntity>(id).unwrap();
            format!(
                "{},{:x},{:x}",
                values(living),
                living.xxa.to_bits(),
                living.yya.to_bits()
            )
        } else {
            values(level.entity::<LivingEntity>(id).unwrap())
        };
        assert_eq!(
            behavior.y_head_rot(id, &level).unwrap().to_bits(),
            value.to_bits(),
            "{row}"
        );
        if f[0] == "mob-accessor" {
            assert_eq!(TRACE.with(|trace| trace.borrow().join(";")), f[3], "{row}");
            assert_eq!(actual, f[4], "{row}");
            assert_eq!(
                level.entity::<EntityData>(id).unwrap().removed.to_string(),
                f[5],
                "{row}"
            );
            assert_eq!(
                format!(
                    "{},{},{}",
                    behavior.get_max_head_x_rot(id, &level).unwrap(),
                    behavior.get_max_head_y_rot(id, &level).unwrap(),
                    behavior.get_head_rot_speed(id, &level).unwrap()
                ),
                f[6],
                "{row}"
            );
            if f[2] == "true" {
                level.finish_entity_management();
                assert!(level.entity_state(id).is_err());
            }
        } else {
            assert_eq!(actual, f[2], "{row}");
            assert!(TRACE.with(|trace| trace.borrow().is_empty()));
        }
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        checked += 1;
    }
    assert_eq!(checked, 24);
}
