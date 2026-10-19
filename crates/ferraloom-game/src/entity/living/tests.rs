use super::*;
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{
    block::UnportedBehavior,
    entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
};
use ferraloom_data::Item;
use ferraloom_protocol::component::{DataComponent, DataComponentPatch, UseEffects};
use ferraloom_util::{Uuid, Vec3};

fn state(living: &LivingEntity) -> String {
    format!(
        "{},{},{},{}",
        living.is_using_item(),
        living.entity_data.get(data::LIVING_ENTITY_FLAGS),
        living.get_use_item().is_empty(),
        living.get_use_item_remaining_ticks()
    )
}

#[test]
fn native_stop_using_item_preserves_flags_metadata_and_use_vibration_conditions() {
    let mut actual = Vec::new();
    for name in [
        "idle",
        "stored_without_flag",
        "active_empty",
        "active_default",
        "vibration_off",
        "vibration_on",
        "empty_payload",
        "other_flags",
        "signed_inactive",
    ] {
        let mut level = crate::entity::tests::level(false);
        game_events::observe(&mut level);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let entity = EntityRef(1);
        let player = ServerPlayer::new(
            Uuid::new(1, 2),
            "using".into(),
            1,
            Vec3::new(8.5, 64.0, 8.5),
            GameType::Survival,
        );
        assert!(!player.is_using_item());
        assert!(player.get_use_item().is_empty());
        assert_eq!(player.get_use_item_remaining_ticks(), 0);
        level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .unwrap();
        let mut stack = if matches!(name, "idle" | "active_empty" | "signed_inactive") {
            ItemStack::default()
        } else {
            ItemStack::new(Item::STICK, 1, DataComponentPatch::default())
        };
        if matches!(
            name,
            "vibration_off" | "vibration_on" | "empty_payload" | "other_flags"
        ) {
            stack
                .0
                .as_mut()
                .unwrap()
                .components
                .added
                .push(DataComponent::UseEffects(UseEffects {
                    can_sprint: true,
                    interact_vibrations: name != "vibration_off",
                    speed_multiplier: 1.0.into(),
                }));
        }
        if name == "empty_payload" {
            crate::entity::player::inventory::set_count(&mut stack, 0);
        }
        let flags: i8 = match name {
            "idle" => 0,
            "stored_without_flag" => 2,
            "active_empty" | "vibration_off" => 3,
            "vibration_on" => 7,
            "other_flags" => -1,
            "signed_inactive" => -128,
            _ => 1,
        };
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        living.use_item = stack;
        living.use_item_remaining = match name {
            "idle" => 0,
            "other_flags" => -5,
            _ => 17,
        };
        living.entity_data.set(data::LIVING_ENTITY_FLAGS, flags);
        let before = state(living);
        living.entity_data.pack_dirty();
        game_events::take(&mut level);
        stop_using_item(entity, &mut level).unwrap();
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        let after = state(living);
        let dirty = living.entity_data.pack_dirty().is_some();
        let events: Vec<_> = game_events::take(&mut level)
            .into_iter()
            .map(|effect| {
                let Observation::GameEvent {
                    event,
                    position,
                    source,
                    affected_state,
                } = effect
                else {
                    panic!("unexpected stop-use effect")
                };
                assert_eq!(source, Some(entity));
                assert!(affected_state.is_none());
                format!(
                    "{}:1:none:{:x},{:x},{:x}",
                    event.name(),
                    position.x.to_bits(),
                    position.y.to_bits(),
                    position.z.to_bits()
                )
            })
            .collect();
        actual.push(format!(
            "stop-use\t{name}\t{before}\t{after},{dirty}\t{}",
            if events.is_empty() {
                "-".into()
            } else {
                events.join("|")
            }
        ));
        // Repeated idle swaps must succeed in Strict without dirtying flags or emitting events.
        stop_using_item(entity, &mut level).unwrap();
        assert!(
            level
                .entity_mut::<LivingEntity>(entity)
                .unwrap()
                .entity_data
                .pack_dirty()
                .is_none()
        );
        assert!(game_events::take(&mut level).is_empty());
    }
    let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("stop-use\t"))
        .collect();
    assert_eq!(actual, expected);
}
