use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{EntityMethod, player::GameType},
    item::enchantment::helper::has_matching_effect,
    level::effects::LevelEffect,
    stats::Stat,
};
use ferraloom_data::{Item, registries::enchantment_effect_component_type::Id as Effect};
use ferraloom_protocol::{
    component::{DataComponentPatch, Enchantment, EquipmentSlot, ItemEnchantments, Unit},
    play::clientbound as cb,
};
use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
use std::io::Read;

fn rows() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/enchantments/hurt/vectors.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}
fn fixture(kind: usize, enchantment: i32) -> ItemStack {
    let item = match kind {
        5 => Item::DIAMOND_CHESTPLATE,
        7 => Item::STONE,
        _ => Item::IRON_AXE,
    };
    let mut patch = DataComponentPatch::default();
    if kind != 7 {
        patch.set(DataComponent::MaxDamage(VarInt(7)));
        patch.set(DataComponent::Damage(VarInt(6)));
    }
    match kind {
        1 => patch.set(DataComponent::Unbreakable(Unit)),
        2 => patch.remove(ComponentTypeId::DAMAGE),
        3 => patch.remove(ComponentTypeId::MAX_DAMAGE),
        4 => patch.set(DataComponent::MaxDamage(VarInt(0))),
        8 => patch.set(DataComponent::MaxDamage(VarInt(-3))),
        _ => {}
    }
    if enchantment > 0 {
        patch.set(DataComponent::Enchantments(ItemEnchantments(vec![(
            Enchantment("minecraft:unbreaking".parse().unwrap()),
            VarInt(enchantment),
        )])));
    }
    let mut stack = ItemStack::new(item, if kind == 9 { 2 } else { 1 }, patch);
    if kind == 6 {
        inventory::set_count(&mut stack, 0);
    }
    stack
}
fn snapshot(stack: &ItemStack) -> String {
    format!(
        "{},{},{},{},{},{}",
        stack.item().name(),
        inventory::count(stack),
        damage_value(stack),
        max_damage(stack),
        is_damageable_item(stack),
        is_broken(stack)
    )
}
fn slot(name: &str) -> EquipmentSlot {
    crate::entity::player::equipment::SLOTS
        .into_iter()
        .find(|slot| crate::entity::player::equipment::slot_name(*slot) == name)
        .unwrap()
}

#[test]
fn native_hurt_and_break_overloads_preserve_abilities_unbreaking_rng_boundaries_copies_events_and_stats()
 {
    let (mut level, player) = crate::item::stack::tests::level();
    crate::level::drop_fixtures::install(&mut level);
    let mut count = 0;
    for row in rows().lines().filter(|row| row.starts_with("hurt-break\t")) {
        let fields = row.split('\t').collect::<Vec<_>>();
        let kind = fields[1].parse::<usize>().unwrap();
        let mode = fields[2];
        let stack = fixture(kind, fields[4].parse().unwrap());
        assert_eq!(snapshot(&stack), fields[6], "{row}");
        let item = if kind == 5 {
            Item::DIAMOND_CHESTPLATE
        } else if kind == 7 {
            Item::STONE
        } else {
            Item::IRON_AXE
        };
        {
            let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
            owner.abilities.instabuild = mode == "infinite" || mode == "creative";
            owner.game_mode = if mode == "creative" {
                GameType::Creative
            } else {
                GameType::Survival
            };
            owner.stats = Default::default();
        }
        level.take_effects();
        *level.random() = Random::Legacy(LegacyRandomSource::new(fields[5].parse().unwrap()));
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation.own(player, stack).unwrap();
        let mut callbacks = Vec::new();
        let amount = fields[3].parse().unwrap();
        if mode == "slot" {
            crate::item::stack::hurt_and_break(
                receiver,
                amount,
                player,
                if kind == 5 {
                    EquipmentSlot::Chest
                } else {
                    EquipmentSlot::Mainhand
                },
                &mut operation,
            )
            .unwrap();
        } else {
            hurt_and_break(
                receiver,
                amount,
                (mode != "none").then_some(player),
                &mut operation,
                |broken, operation| {
                    let live = operation.level.stack(receiver).unwrap();
                    callbacks.push(format!(
                        "{}#{}#{}",
                        snapshot(&broken),
                        snapshot(live),
                        std::ptr::eq(&broken, live)
                    ));
                    Ok(())
                },
            )
            .unwrap();
        }
        assert_eq!(
            snapshot(operation.level.stack(receiver).unwrap()),
            fields[7],
            "{row}"
        );
        assert_eq!(callbacks.join(";"), fields[8], "{row}");
        let mut events = Vec::new();
        for effect in operation.level.take_effects() {
            if let LevelEffect::PlayerPacket { recipients, packet } = effect
                && let cb::Packet::EntityEvent(event) = *packet
            {
                assert_eq!(recipients, [player]);
                events.push(format!("event:{}", event.event_id));
            } else {
                panic!("unexpected durability output: {row}");
            }
        }
        let stat = Stat::Broken(item);
        let broken = operation
            .level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .stats
            .get_value(stat);
        if broken != 0 {
            events.push(format!("stat:{}:{broken}", stat.objective_name()));
        }
        assert_eq!(events.join(";"), fields[9], "{row}");
        assert_eq!(
            operation.level.random().next_long(),
            fields[10].parse::<i64>().unwrap(),
            "{row}"
        );
        assert_eq!(operation.level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 2250);
}

#[test]
fn native_registry_proves_unbreaking_cleanup_noop_and_preserves_required_attribute_and_location_effects()
 {
    let (mut level, player) = crate::item::stack::tests::level();
    level.set_unported_behavior(UnportedBehavior::Strict);
    let mut count = 0;
    for row in rows()
        .lines()
        .filter(|row| row.starts_with("break-cleanup\t"))
    {
        let fields = row.split('\t').collect::<Vec<_>>();
        let slot = slot(fields[2]);
        let mut broken = fixture(0, 0);
        broken
            .0
            .as_mut()
            .unwrap()
            .components
            .set(DataComponent::Enchantments(ItemEnchantments(vec![(
                Enchantment(fields[1].parse().unwrap()),
                VarInt(1),
            )])));
        assert_eq!(
            has_matching_effect(&broken, slot, Effect::ATTRIBUTES, &level).unwrap(),
            fields[3] == "true" && fields[4] == "true"
        );
        assert_eq!(
            has_matching_effect(&broken, slot, Effect::LOCATION_CHANGED, &level).unwrap(),
            fields[3] == "true" && fields[5] == "true"
        );
        level.entity_mut::<ServerPlayer>(player).unwrap().stats = Default::default();
        level.take_effects();
        let result =
            crate::entity::living::on_equipped_item_broken(player, broken, slot, &mut level);
        let method = if fields[4] == "true" {
            Some(EntityMethod::EnchantmentAttributeModifiers)
        } else if fields[5] == "true" {
            Some(EntityMethod::StopLocationBasedEffects)
        } else {
            None
        };
        assert_eq!(
            result,
            method.map_or(Ok(()), |method| Err(LevelError::UnsupportedEntityMethod(
                ferraloom_data::EntityType::PLAYER,
                method
            )))
        );
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .stats
                .get_value(Stat::Broken(Item::IRON_AXE)),
            i32::from(method.is_none())
        );
        assert_eq!(level.take_effects().into_iter().filter(|effect| matches!(effect,LevelEffect::PlayerPacket {packet,..} if matches!(packet.as_ref(),cb::Packet::EntityEvent(_)))).count(),1);
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn native_equipment_wear_preserves_damage_division_eligibility_unbreaking_and_break_output() {
    let (mut level, player) = crate::item::stack::tests::level();
    crate::level::drop_fixtures::install(&mut level);
    let source = level.damage_sources().generic();
    let behavior = level.entity_behavior(player).unwrap();
    let mut count = 0;
    for row in rows()
        .lines()
        .filter(|row| row.starts_with("equipment-wear\t"))
    {
        let fields = row.split('\t').collect::<Vec<_>>();
        {
            let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
            owner.abilities.instabuild = fields[4] == "true";
            owner.stats = Default::default();
            owner
                .equipment
                .set(EquipmentSlot::Chest, fixture(5, fields[3].parse().unwrap()));
        }
        level.take_effects();
        *level.random() = Random::Legacy(LegacyRandomSource::new(fields[2].parse().unwrap()));
        crate::entity::living::hurt::hurt_equipment(
            behavior,
            player,
            &source,
            f32::from_bits(fields[1].parse::<i32>().unwrap() as u32),
            &[EquipmentSlot::Chest],
            &mut level,
        )
        .unwrap();
        assert_eq!(
            snapshot(
                level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .equipment
                    .get(EquipmentSlot::Chest)
            ),
            fields[5],
            "{row}"
        );
        let mut events = Vec::new();
        for effect in level.take_effects() {
            if let LevelEffect::PlayerPacket { packet, .. } = effect
                && let cb::Packet::EntityEvent(event) = *packet
            {
                events.push(format!("event:{}", event.event_id));
            } else {
                panic!("unexpected equipment wear output");
            }
        }
        let stat = Stat::Broken(Item::DIAMOND_CHESTPLATE);
        let broken = level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .stats
            .get_value(stat);
        if broken != 0 {
            events.push(format!("stat:{}:{broken}", stat.objective_name()));
        }
        assert_eq!(events.join(";"), fields[6], "{row}");
        assert_eq!(
            level.random().next_long(),
            fields[7].parse::<i64>().unwrap(),
            "{row}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 72);
}

#[test]
fn native_glider_wear_draws_entity_slot_rng_before_shared_durability_and_glide_event() {
    use crate::entity::{EntityData, LivingEntity, player::equipment::SLOTS};
    use crate::level::game_event::test_support::{self as game_events, Observation};
    let (mut level, player) = crate::item::stack::tests::level();
    game_events::observe(&mut level);
    let behavior = level.entity_behavior(player).unwrap();
    let mut count = 0;
    for row in rows()
        .lines()
        .filter(|row| row.starts_with("glider-wear\t"))
    {
        let fields = row.split('\t').collect::<Vec<_>>();
        {
            let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
            owner.inventory.set_selected_item(ItemStack::default());
            owner.equipment.clear();
            for slot in [
                EquipmentSlot::Mainhand,
                EquipmentSlot::Chest,
                EquipmentSlot::Head,
            ] {
                let mut stack = ItemStack::new(Item::ELYTRA, 1, Default::default());
                let Some(DataComponent::Equippable(mut equippable)) =
                    inventory::component(&stack, ComponentTypeId::EQUIPPABLE).cloned()
                else {
                    panic!("elytra");
                };
                equippable.slot = slot;
                stack
                    .0
                    .as_mut()
                    .unwrap()
                    .components
                    .set(DataComponent::Equippable(equippable));
                if slot == EquipmentSlot::Mainhand {
                    owner.inventory.set_selected_item(stack);
                } else {
                    owner.equipment.set(slot, stack);
                }
            }
            owner.on_ground = false;
            owner.abilities.flying = false;
            owner.set_shared_flag(7, true);
            owner.fall_fly_ticks = 19;
            owner.random.set_seed(fields[1].parse().unwrap());
        }
        *level.random() = Random::Legacy(LegacyRandomSource::new(123));
        game_events::take(&mut level);
        crate::entity::living::gliding::update_fall_flying(behavior, player, &mut level).unwrap();
        let damage = SLOTS
            .into_iter()
            .map(|slot| {
                damage_value(
                    behavior
                        .item_by_slot(player, slot, &level)
                        .unwrap()
                        .unwrap(),
                )
                .to_string()
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(damage, fields[2], "{row}");
        assert_eq!(
            game_events::take(&mut level)
                .into_iter()
                .filter(|observation| matches!(
                    observation,
                    Observation::GameEvent {
                        event: ferraloom_data::registries::game_event::Id::ELYTRA_GLIDE,
                        ..
                    }
                ))
                .count(),
            fields[3].parse::<usize>().unwrap()
        );
        assert_eq!(
            level
                .entity_mut::<EntityData>(player)
                .unwrap()
                .random
                .next_long(),
            fields[4].parse::<i64>().unwrap()
        );
        assert_eq!(
            level.random().next_long(),
            fields[5].parse::<i64>().unwrap()
        );
        assert_eq!(
            level.entity::<LivingEntity>(player).unwrap().fall_fly_ticks,
            19
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 3);
}
