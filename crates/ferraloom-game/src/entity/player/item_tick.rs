//! Bounded Item#inventoryTick dispatch for the player aiStep predecessors.
use ferraloom_data::Item;

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
}
