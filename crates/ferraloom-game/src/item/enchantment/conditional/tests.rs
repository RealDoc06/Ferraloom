use super::*;
use crate::{
    item::enchantment::EnchantmentValueEffect,
    loot::{ContextKeySet, NoFacts},
};
use ferraloom_data::Item;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use std::io::Read;

#[test]
fn native_conditional_codecs_validate_contexts_and_keep_predicate_rng_order() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/conditional.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let data = LootData::default();
    let set = ContextKeySet::vanilla(&"enchanted_item".parse().unwrap()).unwrap();
    let mut cases = [0, 0];
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let value: Value = serde_json::from_str(fields[2]).unwrap();
        let simple = fields[1] == "conditional";
        if fields[0] == "codec" {
            let valid = if simple {
                ConditionalEffect::decode(&value, EnchantmentValueEffect::decode)
                    .and_then(|effect| effect.validate(&data, &set))
                    .is_ok()
            } else {
                TargetedConditionalEffect::decode(
                    &value,
                    fields[1] != "targeted",
                    EnchantmentValueEffect::decode,
                )
                .and_then(|effect| effect.validate(&data, &set))
                .is_ok()
            };
            assert_eq!(valid, fields[3].parse::<bool>().unwrap(), "{line}");
            cases[0] += 1;
        } else {
            let item = Item::from_protocol_id(fields[3].parse().unwrap()).unwrap();
            let params = LootParams {
                tool: Some(ItemStack::new(
                    item,
                    fields[4].parse().unwrap(),
                    Default::default(),
                )),
                enchantment_level: Some(3),
                ..Default::default()
            };
            let mut random = LegacyRandomSource::new(fields[5].parse().unwrap());
            let matches = if simple {
                ConditionalEffect::decode(&value, EnchantmentValueEffect::decode)
                    .unwrap()
                    .matches(&data, &params, &mut random, &NoFacts)
                    .unwrap()
            } else {
                TargetedConditionalEffect::decode(
                    &value,
                    fields[1] != "targeted",
                    EnchantmentValueEffect::decode,
                )
                .unwrap()
                .matches(&data, &params, &mut random, &NoFacts)
                .unwrap()
            };
            assert_eq!(matches, fields[6].parse::<bool>().unwrap(), "{line}");
            assert_eq!(
                random.next_long(),
                fields[7].parse::<i64>().unwrap(),
                "{line}"
            );
            cases[1] += 1;
        }
    }
    assert_eq!(cases, [24, 240]);
}
