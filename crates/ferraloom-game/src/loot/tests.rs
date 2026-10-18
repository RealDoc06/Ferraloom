use super::*;
use ferraloom_data::Item;
use ferraloom_protocol::component::{DataComponentPatch, ItemStack};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use serde_json::json;

#[test]
fn context_sets_distinguish_missing_empty_and_extra() {
    let mut params = LootParams::default();
    let fishing = ContextKeySet::vanilla(&Identifier::parse("fishing").unwrap()).unwrap();
    assert!(params.validate(&fishing).is_err());
    params.origin = Some(ferraloom_util::Vec3::new(0.0, 0.0, 0.0));
    params.tool = Some(ItemStack::default());
    params.validate(&fishing).unwrap();
    params.enchantment_active = Some(false);
    assert!(params.validate(&fishing).is_err());
}
#[test]
fn provider_failure_boundary_preserves_nested_arithmetic() {
    let data = LootData::default();
    let params = LootParams::default();
    let mut random = LegacyRandomSource::new(1);
    let mut c = EvalContext {
        params: &params,
        random: &mut random,
        facts: &NoFacts,
        visited: Default::default(),
    };
    let p = ContextIntProvider::decode(
        &json!({"type":"add","inputs":[5,{"type":"div","left":1,"right":0}]}),
    )
    .unwrap();
    assert_eq!(p.get(&data, &mut c).unwrap(), 0);
    let p = ContextFloatProvider::decode(&json!({"type":"div","left":1,"right":0})).unwrap();
    assert_eq!(p.get(&data, &mut c).unwrap(), 0.0);
}
#[test]
fn single_entry_avoids_selection_draw_and_empty_range_avoids_input_draw() {
    let mut data = LootData::default();
    let id = Identifier::parse("test").unwrap();
    data.tables.insert(
        id.clone(),
        LootTable::decode(
            &json!({"pools":[{"rolls":1,"entries":[{"type":"item","name":"stone"}]}]}),
        )
        .unwrap(),
    );
    let params = LootParams::default();
    let mut actual = LegacyRandomSource::new(42);
    let mut expected = actual.clone();
    assert_eq!(
        data.get_random_items(&id, &params, &mut actual, &NoFacts)
            .unwrap(),
        vec![ItemStack::new(
            Item::STONE,
            1,
            DataComponentPatch::default()
        )]
    );
    let condition=Holder::direct(LootItemCondition::decode(&json!({"type":"int_value_check","value":{"type":"uniform","min":0,"max":20},"test":{}})).unwrap());
    assert!(
        data.test_predicate(&condition, &params, &mut actual, &NoFacts)
            .unwrap()
    );
    assert_eq!(actual.next_long(), expected.next_long());
}
#[test]
fn named_sequences_save_and_resume() {
    let id = Identifier::parse("entities/pig").unwrap();
    let mut streams = RandomSequences::default();
    streams.get(&id, -17).next_int_bound(33);
    streams.get(&id, -17).next_double();
    let mut restored = RandomSequences::from_nbt(&streams.to_nbt()).unwrap();
    assert_eq!(
        streams.get(&id, -17).next_long(),
        restored.get(&id, -17).next_long()
    );
}
#[test]
fn validation_reports_fatal_cycles_and_nonfatal_parameter_usage() {
    let mut data = LootData::default();
    let id = Identifier::parse("loop").unwrap();
    data.tables.insert(id.clone(),LootTable::decode(&json!({"type":"empty","pools":[{"rolls":1,"entries":[{"type":"loot_table","value":"loop","condition":{"type":"match_tool"}}]}]})).unwrap());
    let problems = data.validate();
    assert!(problems.iter().any(|p| p.fatal));
    assert!(problems.iter().any(|p| !p.fatal));
}
