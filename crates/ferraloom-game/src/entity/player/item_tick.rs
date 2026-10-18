//! Bounded Item#inventoryTick dispatch for the player aiStep predecessors.
use super::{ServerPlayer, equipment::SLOTS};
use crate::{
    entity::{EntityRef, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_data::Item;
use ferraloom_protocol::component::EquipmentSlot;

/// Native declaring class for the only two Item#inventoryTick overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InventoryTickOverride {
    Compass,
    Map,
}
impl InventoryTickOverride {
    pub(crate) fn method(self) -> &'static str {
        match self {
            Self::Compass => "CompassItem#inventoryTick",
            Self::Map => "MapItem#inventoryTick",
        }
    }
}

/// Native Items registrations by the method's declaring class. The oracle
/// reflects every registered Item so explorer-map additions cannot be omitted.
pub(crate) fn inventory_tick_override(item: Item) -> Option<InventoryTickOverride> {
    match item {
        Item::COMPASS => Some(InventoryTickOverride::Compass),
        Item::FILLED_MAP
        | Item::OCEAN_MONUMENT_MAP
        | Item::WOODLAND_MANSION_MAP
        | Item::BURIED_TRIAL_CHAMBERS_MAP
        | Item::JUNGLE_PYRAMID_MAP
        | Item::SWAMP_HUT_MAP
        | Item::DESERT_VILLAGE_MAP
        | Item::PLAINS_VILLAGE_MAP
        | Item::SAVANNA_VILLAGE_MAP
        | Item::SNOWY_VILLAGE_MAP
        | Item::TAIGA_VILLAGE_MAP
        | Item::BURIED_TREASURE_MAP
        | Item::BURIED_ANCIENT_CITY_MAP
        | Item::BURIED_MINESHAFT_MAP
        | Item::DESERT_PYRAMID_MAP
        | Item::ABANDONED_CAMP_MAP
        | Item::WARM_OCEAN_RUINS_MAP => Some(InventoryTickOverride::Map),
        _ => None,
    }
}

/// Server ItemStack#inventoryTick's observable body. popTime is never read by
/// server logic, serialized or sent, so its decrement and state are omitted.
fn tick_item(
    item: Item,
    _slot: Option<EquipmentSlot>,
    level: &mut Level,
) -> Result<(), LevelError> {
    if inventory_tick_override(item).is_some() {
        level.unported_operation(crate::block::UnportedOperation::ItemInventoryTick(item))?;
    }
    Ok(())
}

/// Port of Inventory#tick. Read the selected slot at each native invocation;
/// end canonical stack borrows before the item method's policy dispatch.
pub(crate) fn tick_inventory(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let len = level.entity::<ServerPlayer>(player)?.inventory.items.len();
    for index in 0..len {
        let tick = {
            let inventory = &level.entity::<ServerPlayer>(player)?.inventory;
            let stack = &inventory.items[index];
            (!stack.is_empty()).then(|| {
                (
                    stack.item(),
                    (index == inventory.selected_slot()).then_some(EquipmentSlot::Mainhand),
                )
            })
        };
        if let Some((item, slot)) = tick {
            tick_item(item, slot, level)?;
        }
    }
    Ok(())
}

/// Port of EntityEquipment#tick. This reads the backing equipment map, including
/// its mainhand entry, rather than PlayerEquipment's selected-hand override.
pub(crate) fn tick_equipment(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    for slot in SLOTS {
        let item = {
            let stack = level.entity::<LivingEntity>(entity)?.equipment.get(slot);
            (!stack.is_empty()).then(|| stack.item())
        };
        if let Some(item) = item {
            tick_item(item, Some(slot), level)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_inventory_tick_override_set_matches_every_registered_item() {
        let actual: Vec<_> = (0..ferraloom_data::ITEMS.len())
            .filter_map(|id| {
                let item = Item::from_protocol_id(id as u16).unwrap();
                inventory_tick_override(item).map(|class| {
                    format!(
                        "item-inventory-tick-override\t{}\t{}",
                        item.name(),
                        class.method()
                    )
                })
            })
            .collect();
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("item-inventory-tick-override\t"))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 18);
        assert_eq!(inventory_tick_override(Item::MAP), None);
    }

    #[test]
    fn inventory_and_equipment_ticks_skip_empty_payloads_and_use_native_scan_order() {
        use crate::{
            block::UnportedBehavior,
            entity::{EntityState, player::GameType},
        };
        use ferraloom_protocol::component::{DataComponentPatch, ItemStack};
        use ferraloom_util::{Uuid, Vec3};
        let stack = |item, count| {
            let mut value = ItemStack::new(item, 1, DataComponentPatch::default());
            value.0.as_mut().unwrap().count = count;
            value
        };
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(UnportedBehavior::Strict);
        let entity = level.next_entity_ref();
        let mut player = ServerPlayer::new(
            Uuid::new(3, 4),
            "ticks".into(),
            level.next_entity_id(),
            Vec3::ZERO,
            GameType::Survival,
        );
        player.inventory.set_selected_slot(8);
        player
            .inventory
            .set_item(0, stack(Item::COMPASS, 0), &mut player.parent.equipment);
        player.inventory.set_item(
            1,
            stack(Item::OCEAN_MONUMENT_MAP, -1),
            &mut player.parent.equipment,
        );
        player
            .inventory
            .set_item(2, stack(Item::MAP, 1), &mut player.parent.equipment);
        player
            .inventory
            .set_item(3, stack(Item::STONE, 1), &mut player.parent.equipment);
        level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .unwrap();
        tick_inventory(entity, &mut level).unwrap();
        {
            let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
            player.inventory.set_item(
                7,
                stack(Item::OCEAN_MONUMENT_MAP, 1),
                &mut player.parent.equipment,
            );
            player.inventory.set_selected_item(stack(Item::COMPASS, 1));
        }
        assert_eq!(
            tick_inventory(entity, &mut level),
            Err(LevelError::UnsupportedItemInventoryTick(
                Item::OCEAN_MONUMENT_MAP
            ))
        );
        {
            let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
            player
                .equipment
                .set(EquipmentSlot::Mainhand, stack(Item::FILLED_MAP, 0));
            player
                .equipment
                .set(EquipmentSlot::Offhand, stack(Item::COMPASS, 1));
            player
                .equipment
                .set(EquipmentSlot::Head, stack(Item::OCEAN_MONUMENT_MAP, 1));
        }
        assert_eq!(
            tick_equipment(entity, &mut level),
            Err(LevelError::UnsupportedItemInventoryTick(Item::COMPASS))
        );
        level.set_unported_behavior(UnportedBehavior::Warn);
        tick_inventory(entity, &mut level).unwrap();
        tick_equipment(entity, &mut level).unwrap();
        assert_eq!(level.unported_behavior_stats().skipped_calls, 4);
        assert_eq!(level.unported_behavior_stats().distinct_calls, 2);
    }
}
