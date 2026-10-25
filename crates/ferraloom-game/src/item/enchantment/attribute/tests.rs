use super::*;
use std::io::Read;

#[test]
fn native_attribute_effect_modifiers_keep_slot_ids_float_widening_and_operations() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/attributes.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut cases = [0, 0];
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let value: Value = serde_json::from_str(fields[1]).unwrap();
        if fields[0] == "codec" {
            assert_eq!(
                EnchantmentAttributeEffect::decode(&value).is_ok(),
                fields[2].parse::<bool>().unwrap(),
                "{line}"
            );
            cases[0] += 1;
        } else {
            let effect = EnchantmentAttributeEffect::decode(&value).unwrap();
            let slot = EquipmentSlot::from_json(&Value::String(fields[3].into())).unwrap();
            let modifier = effect.modifier(fields[2].parse().unwrap(), slot).unwrap();
            assert_eq!(modifier.id.as_str(), fields[4], "{line}");
            assert_eq!(
                modifier.amount.0.to_bits(),
                fields[5].parse::<i64>().unwrap() as u64,
                "{line}"
            );
            assert_eq!(
                modifier.operation.to_json().unwrap(),
                Value::String(fields[6].into()),
                "{line}"
            );
            cases[1] += 1;
        }
    }
    assert_eq!(cases, [16, 1680]);
}
