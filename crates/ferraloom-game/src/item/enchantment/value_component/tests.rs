use super::*;
use std::io::Read;

#[test]
fn native_value_components_validate_their_exact_context_and_payload_kind() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/value-components.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let data = LootData::default();
    let mut cases = 0;
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let component = ComponentType::from_name(fields[1]).unwrap();
        let value: Value = serde_json::from_str(fields[2]).unwrap();
        let decoded =
            EnchantmentValueEffectComponent::decode(component, &value).and_then(|value| {
                let value = value.expect("native case names a value component");
                value.validate(&data)?;
                Ok(value)
            });
        assert_eq!(
            decoded.is_ok(),
            fields[3].parse::<bool>().unwrap(),
            "{line}"
        );
        if let Ok(value) = decoded {
            assert_eq!(value.component_type(), component, "{line}");
            assert_eq!(
                value
                    .conditional_effects()
                    .map_or(-1, |effects| effects.len() as i32),
                fields[4].parse::<i32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                value.unconditional_effect().is_some(),
                matches!(
                    component,
                    ComponentType::CROSSBOW_CHARGE_TIME
                        | ComponentType::TRIDENT_SPIN_ATTACK_STRENGTH
                ),
                "{line}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 252);
}
