use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{
        EntityState,
        ai::attributes::{AttributeMap, AttributeModifier, AttributeOperation},
        animal::Cow,
        player::{
            GameType, ServerPlayer,
            equipment::{SLOTS, slot_name},
            inventory::set_count,
        },
    },
    stats::Stat,
};
use ferraloom_data::{EntityType, Item};
use ferraloom_protocol::{
    codec::VarInt,
    component::{
        Attribute as AttributeHolder, AttributeDisplay, AttributeEntry, Enchantment,
        EquipmentSlotGroup, ItemEnchantments,
    },
};
use ferraloom_util::{Uuid, Vec3};
use std::sync::Arc;

fn modifier() -> AttributeModifier {
    AttributeModifier {
        id: "minecraft:broken".parse().unwrap(),
        amount: 0.5.into(),
        operation: AttributeOperation::AddValue,
    }
}

fn receiver(installed: bool) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    let owner = level.next_entity_ref();
    let mut player = ServerPlayer::new(
        Uuid::new(1, 2),
        "break".into(),
        level.next_entity_id().unwrap(),
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    player.attributes = AttributeMap::new(EntityType::PLAYER);
    if installed {
        player
            .attributes
            .get_instance(Attribute::SUBMERGED_MINING_SPEED)
            .unwrap()
            .add_transient_modifier(Arc::new(modifier()))
            .unwrap();
    }
    player.attributes.clear_attributes_to_sync();
    player.attributes.clear_attributes_to_update();
    level
        .add_entity(owner, EntityState::Player(Box::new(player)))
        .unwrap();
    level.take_effects();
    (level, owner)
}

fn broken_stack(slot: EquipmentSlot, scenario: &str, attribute: Attribute) -> ItemStack {
    let group = if scenario == "other-slot" {
        if slot == EquipmentSlot::Head {
            EquipmentSlotGroup::Chest
        } else {
            EquipmentSlotGroup::Head
        }
    } else {
        match slot {
            EquipmentSlot::Mainhand => EquipmentSlotGroup::Mainhand,
            EquipmentSlot::Offhand => EquipmentSlotGroup::Offhand,
            EquipmentSlot::Feet => EquipmentSlotGroup::Feet,
            EquipmentSlot::Legs => EquipmentSlotGroup::Legs,
            EquipmentSlot::Chest => EquipmentSlotGroup::Chest,
            EquipmentSlot::Head => EquipmentSlotGroup::Head,
            EquipmentSlot::Body => EquipmentSlotGroup::Body,
            EquipmentSlot::Saddle => EquipmentSlotGroup::Saddle,
        }
    };
    let mut broken = ItemStack::new(Item::STONE, 1, Default::default());
    broken
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::AttributeModifiers(vec![AttributeEntry {
            attribute: AttributeHolder(attribute.name().parse().unwrap()),
            modifier: modifier(),
            slot: group,
            display: AttributeDisplay::Default,
        }]));
    if scenario == "empty" {
        set_count(&mut broken, 0);
    }
    broken
}

#[test]
fn native_equipment_break_ids_cleanup_and_stats_deliver_the_captured_event() {
    let mut cases = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("equipment-break\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let slot = SLOTS
            .into_iter()
            .find(|slot| slot_name(*slot) == fields[1])
            .unwrap();
        let installed = fields[2] == "installed";
        for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
            let (mut level, owner) = receiver(installed);
            level.set_unported_behavior(policy);
            let broken = broken_stack(slot, fields[2], Attribute::SUBMERGED_MINING_SPEED);
            let kind = broken.item();
            let result = on_equipped_item_broken(owner, broken, slot, &mut level);
            result.unwrap();
            let player = level.entity::<ServerPlayer>(owner).unwrap();
            let stat = player.stats.get_value(Stat::Broken(kind));
            // Native event selection precedes the component cleanup and broken-item stat.
            assert_eq!(
                format!(
                    "event:{}:{installed};stat:{}:{stat}",
                    entity_event_for_equipment_break(slot),
                    kind.name(),
                ),
                fields[3],
                "{row}",
            );
            assert_eq!(
                player.attributes.save_nbt(),
                ferraloom_nbt::parse(fields[4]).unwrap(),
                "{row}",
            );
            assert_eq!(
                player.attributes.attributes_to_sync().count(),
                fields[5].parse::<usize>().unwrap(),
                "{row}",
            );
            assert_eq!(
                player.attributes.attributes_to_update().count(),
                fields[6].parse::<usize>().unwrap(),
                "{row}",
            );
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0, "{row}");
            let effects = level.take_effects();
            assert!(
                matches!(effects.as_slice(),
                    [crate::level::effects::LevelEffect::PlayerPacket { recipients, packet }]
                    if recipients == &[owner] && matches!(packet.as_ref(),
                        ferraloom_protocol::play::clientbound::Packet::EntityEvent(packet)
                        if packet.event_id == entity_event_for_equipment_break(slot))
                ),
                "{row}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 32);
}

#[test]
fn living_equipment_break_parent_does_not_award_player_stats() {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Warn);
    let owner = level.next_entity_ref();
    let mut cow = Cow::with_parent(level.create_entity_data(EntityType::COW).unwrap());
    cow.as_living_mut()
        .attributes
        .get_instance(Attribute::MAX_HEALTH)
        .unwrap()
        .add_transient_modifier(Arc::new(modifier()))
        .unwrap();
    level
        .add_entity(owner, EntityState::Cow(Box::new(cow)))
        .unwrap();
    on_equipped_item_broken(
        owner,
        broken_stack(EquipmentSlot::Feet, "installed", Attribute::MAX_HEALTH),
        EquipmentSlot::Feet,
        &mut level,
    )
    .unwrap();
    assert!(
        !level
            .entity::<LivingEntity>(owner)
            .unwrap()
            .attributes
            .has_modifier(Attribute::MAX_HEALTH, &modifier().id)
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    assert!(level.take_effects().is_empty()); // Unobserved cow has no tracking audience.
}

#[test]
fn required_enchantment_iteration_preserves_completed_component_cleanup_prefix() {
    let (mut level, owner) = receiver(true);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let mut broken = broken_stack(
        EquipmentSlot::Mainhand,
        "installed",
        Attribute::SUBMERGED_MINING_SPEED,
    );
    broken
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::Enchantments(ItemEnchantments(vec![(
            Enchantment("minecraft:unbreaking".parse().unwrap()),
            VarInt(1),
        )])));
    assert_eq!(
        stop_location_based_effects(owner, &broken, EquipmentSlot::Mainhand, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::PLAYER,
            EntityMethod::EnchantmentAttributeModifiers,
        )),
    );
    assert!(
        !level
            .entity::<LivingEntity>(owner)
            .unwrap()
            .attributes
            .has_modifier(Attribute::SUBMERGED_MINING_SPEED, &modifier().id)
    );
    level.set_unported_behavior(UnportedBehavior::Warn);
    on_equipped_item_broken(owner, broken, EquipmentSlot::Mainhand, &mut level).unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
    assert_eq!(
        level
            .entity::<ServerPlayer>(owner)
            .unwrap()
            .stats
            .get_value(Stat::Broken(Item::STONE)),
        1,
    );
}
