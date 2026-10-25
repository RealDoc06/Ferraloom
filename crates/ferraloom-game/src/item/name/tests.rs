use super::*;
use ferraloom_protocol::component::{DataComponentPatch, Potion, PotionContents};

#[test]
fn item_name_overrides_match_native_component_and_registry_reads() {
    let mut count = 0;
    for row in include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("item-name\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let stack = if fields[1] == "0" {
            ItemStack::default()
        } else {
            ItemStack::from_json(&serde_json::from_str(fields[2]).unwrap()).unwrap()
        };
        assert_eq!(
            stack.get_item_name().unwrap(),
            Component::from_json(fields[3]).unwrap(),
            "case {}",
            fields[1]
        );
        count += 1;
    }
    assert_eq!(count, 15);
}

#[test]
fn unknown_potion_is_an_error_and_empty_payloads_use_air_holder_names() {
    let mut patch = DataComponentPatch::default();
    patch.set(DataComponent::PotionContents(PotionContents {
        potion: Some(Potion("missing:unbound".parse().unwrap())),
        custom_color: None,
        custom_effects: Vec::new(),
        custom_name: None,
    }));
    let mut stack = ItemStack::new(Item::POTION, 1, patch);
    assert!(get_item_name(&stack).is_err());
    stack.0.as_mut().unwrap().count = 0;
    assert_eq!(
        get_item_name(&stack).unwrap(),
        Component::translatable("block.minecraft.air", Vec::new())
    );
}

#[test]
fn hover_and_display_names_match_native_custom_names_rarity_and_item_templates() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../commands/vanilla/item-display-name-vectors.json"
    ))
    .unwrap();
    for row in &rows {
        let mut stack = if row["empty"] == true {
            ItemStack::default()
        } else {
            ItemStack::from_json(&row["input"]).unwrap()
        };
        if row["depleted"] == true {
            stack.0.as_mut().unwrap().count = 0;
        }
        for (method, actual) in [
            ("hover", stack.get_hover_name().unwrap()),
            ("display", stack.get_display_name().unwrap()),
        ] {
            // Item hovers contain a generic NBT patch. Compare the native NBT
            // representation used on the wire, preserving byte/boolean kinds.
            let expected =
                ferraloom_nbt::parse(row[format!("{method}_nbt")].as_str().unwrap()).unwrap();
            assert_eq!(
                actual.to_nbt().unwrap(),
                expected,
                "case {} {method}",
                row["case"]
            );
        }
    }
    assert_eq!(rows.len(), 32);
}
