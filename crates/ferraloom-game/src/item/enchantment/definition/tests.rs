use super::*;
use ferraloom_data::datapack::{Pack, ResourceManager, TagSet, load_tags};
use std::{collections::BTreeMap, io::Read, path::Path};

struct ItemFacts(TagSet<Item>);
impl PredicateContext for ItemFacts {
    fn item_tags(&self) -> Result<&TagSet<Item>> {
        Ok(&self.0)
    }
}
fn slots(definition: &EnchantmentDefinition) -> u8 {
    [
        EquipmentSlot::Mainhand,
        EquipmentSlot::Offhand,
        EquipmentSlot::Feet,
        EquipmentSlot::Legs,
        EquipmentSlot::Chest,
        EquipmentSlot::Head,
        EquipmentSlot::Body,
        EquipmentSlot::Saddle,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |bits, (index, slot)| {
        bits | (u8::from(definition.matching_slot(slot)) << index)
    })
}

#[test]
fn native_definitions_match_registry_items_slots_codec_limits_and_overflow_costs() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native datapack");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let tags = load_tags(&resources, "item", |id, _| Item::from_name(&id.to_string()));
    assert!(tags.diagnostics.is_empty());
    let facts = ItemFacts(TagSet::from_tags(tags, ferraloom_data::ITEMS.len()).unwrap());
    let definitions: BTreeMap<_, _> = resources
        .registry("enchantment")
        .into_iter()
        .map(|(id, resource)| {
            (
                id.to_string(),
                EnchantmentDefinition::decode(&resource.json().unwrap()).unwrap(),
            )
        })
        .collect();
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/definitions.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut cases = [0; 6];
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "definition" => {
                let definition = &definitions[fields[1]];
                assert_eq!(
                    definition.weight,
                    fields[2].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    definition.max_level,
                    fields[3].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    definition.anvil_cost,
                    fields[4].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    slots(definition),
                    fields[5].parse::<u8>().unwrap(),
                    "{line}"
                );
                cases[0] += 1;
            }
            "cost" => {
                let definition = &definitions[fields[1]];
                let level = fields[2].parse().unwrap();
                assert_eq!(
                    definition.min_cost.calculate(level),
                    fields[3].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    definition.max_cost.calculate(level),
                    fields[4].parse::<i32>().unwrap(),
                    "{line}"
                );
                cases[1] += 1;
            }
            "item" => {
                let definition = &definitions[fields[1]];
                let item = Item::from_protocol_id(fields[2].parse().unwrap()).unwrap();
                let stack = ItemStack::new(item, fields[3].parse().unwrap(), Default::default());
                let supported = definition.is_supported_item(&stack, &facts).unwrap();
                assert_eq!(supported, fields[4].parse::<bool>().unwrap(), "{line}");
                assert_eq!(
                    definition.is_primary_item(&stack, &facts).unwrap(),
                    fields[5].parse::<bool>().unwrap(),
                    "{line}"
                );
                assert_eq!(supported, fields[6].parse::<bool>().unwrap(), "{line}");
                cases[2] += 1;
            }
            "codec" => {
                let value: Value = serde_json::from_str(fields[1]).unwrap();
                assert_eq!(
                    EnchantmentDefinition::decode(&value).is_ok(),
                    fields[2].parse::<bool>().unwrap(),
                    "{line}"
                );
                cases[3] += 1;
            }
            "slots" => {
                let value: Value = serde_json::from_str(fields[1]).unwrap();
                assert_eq!(
                    slots(&EnchantmentDefinition::decode(&value).unwrap()),
                    fields[2].parse::<u8>().unwrap(),
                    "{line}"
                );
                cases[4] += 1;
            }
            "overflow" => {
                let cost = EnchantmentCost {
                    base: fields[1].parse().unwrap(),
                    per_level_above_first: fields[2].parse().unwrap(),
                };
                assert_eq!(
                    cost.calculate(fields[3].parse().unwrap()),
                    fields[4].parse::<i32>().unwrap(),
                    "{line}"
                );
                cases[5] += 1;
            }
            _ => panic!("unexpected native case: {line}"),
        }
    }
    assert_eq!(cases, [43, 387, 1806, 40, 11, 175]);
}
