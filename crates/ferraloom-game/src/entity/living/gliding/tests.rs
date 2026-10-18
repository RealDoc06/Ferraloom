use super::*;
use crate::entity::{
    EntityState,
    effect::MobEffectInstance,
    player::{GameType, ServerPlayer},
};
use crate::{block::UnportedBehavior, level::effects::LevelEffect};
use ferraloom_data::Item;
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::{
    codec::VarInt,
    component::{DataComponentPatch, Persistent, Unit},
};
use ferraloom_util::{Uuid, Vec3};

fn stack(config: usize, slot: EquipmentSlot) -> ItemStack {
    let original = ItemStack::new(Item::ELYTRA, 1, DataComponentPatch::default());
    let Some(DataComponent::Equippable(mut equippable)) =
        component(&original, ComponentType::EQUIPPABLE).cloned()
    else {
        panic!("elytra equippable")
    };
    equippable.slot = if config == 3 {
        EquipmentSlot::Chest
    } else {
        slot
    };
    let mut patch = DataComponentPatch::default();
    patch.set(DataComponent::Equippable(equippable));
    match config {
        1 => patch.remove(ComponentType::GLIDER),
        2 => patch.remove(ComponentType::EQUIPPABLE),
        4 => patch.set(DataComponent::Damage(VarInt(431))),
        5 => patch.set(DataComponent::Damage(VarInt(432))),
        6 => patch.set(DataComponent::Damage(VarInt(-7))),
        7 => {
            patch.set(DataComponent::Damage(VarInt(432)));
            patch.set(DataComponent::Unbreakable(Unit));
        }
        8 => patch.remove(ComponentType::MAX_DAMAGE),
        9 => patch.remove(ComponentType::DAMAGE),
        _ => {}
    }
    let mut stack = ItemStack::new(Item::ELYTRA, 1, patch);
    if config == 10 {
        stack.0.as_mut().unwrap().count = 0;
    }
    if config == 11 {
        stack.0.as_mut().unwrap().count = -1;
    }
    stack
}
fn slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Mainhand => "mainhand",
        EquipmentSlot::Offhand => "offhand",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}
fn slot(name: &str) -> EquipmentSlot {
    SLOTS
        .into_iter()
        .find(|slot| slot_name(*slot) == name)
        .unwrap()
}
fn fixture(row: &[&str]) -> (Level, EntityRef) {
    let kind: usize = row[2].parse().unwrap();
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let entity = level.next_entity_ref();
    let mut player = ServerPlayer::new(
        Uuid::new(3, 4),
        "glide".into(),
        level.next_entity_id(),
        Vec3::ZERO,
        GameType::Survival,
    );
    player
        .inventory
        .set_selected_item(stack(0, EquipmentSlot::Mainhand));
    player
        .equipment
        .set(EquipmentSlot::Chest, stack(0, EquipmentSlot::Chest));
    player
        .equipment
        .set(EquipmentSlot::Head, stack(0, EquipmentSlot::Head));
    if kind == 1 {
        player.on_ground = true;
    }
    if kind == 2 {
        player.abilities.flying = true;
    }
    if kind == 3 {
        let effect = MobEffectInstance::new(MobEffect::LEVITATION, 100, 0, false, true, true, None);
        assert!(
            player
                .active_effects
                .load(Some(&Tag::List(ListTag(vec![
                    effect.to_component().to_nbt().unwrap()
                ]))))
                .is_empty()
        );
    }
    if kind == 4 || kind == 6 {
        player.inventory.set_selected_item(ItemStack::default());
        player.equipment.clear();
        if kind == 6 {
            player
                .equipment
                .set(EquipmentSlot::Chest, stack(4, EquipmentSlot::Chest));
        }
    }
    if kind == 5 {
        player.vehicle.set(Some(EntityRef(999)));
    }
    player.fall_fly_ticks = row[3].parse().unwrap();
    player.fall_distance = 10.0;
    player.velocity = Vec3::new(
        0.0,
        f64::from_bits(u64::from_str_radix(row[4], 16).unwrap()),
        0.0,
    );
    player.set_shared_flag(7, true);
    player.random.set_seed(row[5].parse().unwrap());
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    level.take_effects();
    (level, entity)
}
#[test]
fn native_glider_eligibility_uses_effective_components_and_next_break_threshold() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("glider-eligibility\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let slot = slot(row[2]);
        assert_eq!(
            can_glide_using(&stack(row[1].parse().unwrap(), slot), slot).to_string(),
            row[3],
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 96);
}
#[test]
fn native_glide_cadence_keeps_slot_order_rng_before_durability_and_event_after_it() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("glider-cadence\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let (mut level, entity) = fixture(&row);
        let behavior = level.entity_behavior(entity).unwrap();
        assert_eq!(
            behavior.can_glide(entity, &level).unwrap().to_string(),
            row[6]
        );
        level.set_unported_behavior(UnportedBehavior::Warn);
        update_fall_flying(behavior, entity, &mut level).unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(base.shared_flag(7).to_string(), row[7], "flag {row:?}");
        assert_eq!(
            format!("{:x}", base.fall_distance.to_bits()),
            row[8],
            "fall {row:?}"
        );
        let events = level
            .take_effects()
            .into_iter()
            .filter(|effect| {
                matches!(
                    effect,
                    LevelEffect::GameEvent {
                        event: GameEvent::ELYTRA_GLIDE,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(events.to_string(), row[9], "events {row:?}");
        assert_eq!(
            level
                .entity_mut::<EntityData>(entity)
                .unwrap()
                .random
                .next_long()
                .to_string(),
            row[11],
            "rng {row:?}"
        );
        let ticks: i32 = row[3].parse().unwrap();
        let durability = row[6] == "true" && ticks.wrapping_add(1) % 20 == 0;
        assert_eq!(
            level.unported_behavior_stats().skipped_calls,
            u64::from(durability),
            "warn {row:?}"
        );
        if durability {
            let (mut strict, entity) = fixture(&row);
            assert_eq!(
                update_fall_flying(behavior, entity, &mut strict),
                Err(LevelError::UnsupportedItemHurtAndBreak {
                    item: Item::ELYTRA,
                    amount: 1,
                    slot: slot(row[10])
                }),
                "strict {row:?}"
            );
            assert!(!strict.take_effects().into_iter().any(|effect| matches!(
                effect,
                LevelEffect::GameEvent {
                    event: GameEvent::ELYTRA_GLIDE,
                    ..
                }
            )));
            assert_eq!(
                strict
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long()
                    .to_string(),
                row[11]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 98);
}
