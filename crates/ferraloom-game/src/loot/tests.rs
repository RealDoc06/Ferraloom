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

fn canonical(stack: &ItemStack) -> serde_json::Value {
    use ferraloom_protocol::component::Persistent;
    if stack.is_empty() {
        serde_json::Value::Null
    } else {
        stack.to_json().unwrap()
    }
}
fn decode_stack(value: &serde_json::Value) -> ItemStack {
    use ferraloom_protocol::component::Persistent;
    if value.is_null() {
        ItemStack::default()
    } else {
        ItemStack::from_json(value).unwrap()
    }
}
fn assert_stacks(actual: &[ItemStack], expected: &serde_json::Value, label: &str) {
    let actual = actual.iter().map(canonical).collect::<Vec<_>>();
    let expected = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| canonical(&decode_stack(v)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "{label}");
}
#[test]
fn synthetic_tables_and_providers_match_real_java() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["synthetic_tables"].as_array().unwrap() {
        let table = LootTable::decode(&record["definition"]).unwrap();
        let id = Identifier::parse("test").unwrap();
        let mut data = LootData::default();
        data.tables.insert(id.clone(), table);
        let params = LootParams {
            luck: 0.75,
            ..LootParams::default()
        };
        let seed = record["seed"].as_i64().unwrap();
        let mut random = LegacyRandomSource::new(seed);
        let items = data
            .get_random_items(&id, &params, &mut random, &NoFacts)
            .unwrap();
        assert_stacks(&items, &record["items"], &record.to_string());
        assert_eq!(random.next_long(), record["next"].as_i64().unwrap());
        let mut random = LegacyRandomSource::new(seed);
        let mut container = vec![ItemStack::default(); 9];
        container[3] = ItemStack::new(Item::APPLE, 1, DataComponentPatch::default());
        data.fill(&id, &mut container, &params, &mut random, &NoFacts)
            .unwrap();
        assert_stacks(&container, &record["slots"], &record.to_string());
        assert_eq!(random.next_long(), record["fill_next"].as_i64().unwrap());
    }
    let data = LootData::default();
    let params = LootParams::default();
    for record in vectors["int_providers"].as_array().unwrap() {
        let provider = ContextIntProvider::decode(&record["definition"]).unwrap();
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        let value = provider
            .get(
                &data,
                &mut EvalContext {
                    params: &params,
                    random: &mut random,
                    facts: &NoFacts,
                    visited: Default::default(),
                },
            )
            .unwrap();
        assert_eq!(
            value,
            i32::try_from(record["value"].as_i64().unwrap()).unwrap(),
            "{record}"
        );
        assert_eq!(random.next_long(), record["next"].as_i64().unwrap());
    }
}

struct ItemFacts {
    items: ferraloom_data::datapack::TagSet<Item>,
    blocks: ferraloom_data::datapack::TagSet<ferraloom_data::Block>,
}
impl crate::predicates::PredicateContext for ItemFacts {
    fn item_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Item>> {
        Ok(&self.items)
    }
    fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::Block>> {
        Ok(&self.blocks)
    }
    fn entity_flags(
        &self,
        _entity: crate::entity::EntityRef,
    ) -> Result<crate::predicates::EntityFlags> {
        Ok(crate::predicates::EntityFlags {
            on_ground: true,
            ..Default::default()
        })
    }
    fn entity_type(&self, _entity: crate::entity::EntityRef) -> Result<ferraloom_data::EntityType> {
        Ok(ferraloom_data::EntityType::PIG)
    }
    fn entity_position(&self, _entity: crate::entity::EntityRef) -> Result<ferraloom_util::Vec3> {
        Ok(ferraloom_util::Vec3::new(0.5, 64.0, 0.5))
    }
}
impl LootFacts for ItemFacts {
    fn dynamic_drops(&self, _id: &Identifier) -> Result<Vec<ItemStack>> {
        Ok(Vec::new())
    }
}
fn vanilla_resources() -> ferraloom_data::datapack::ResourceManager {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR")
        .expect("set FERRALOOM_VANILLA_JAR to the vanilla jar; the builder wrapper sets this");
    ResourceManager::new(vec![
        Pack::vanilla_jar(std::path::Path::new(&path)).unwrap(),
    ])
}
#[test]
fn item_predicate_acceptance_matches_real_java() {
    let resources = vanilla_resources();
    let facts = ItemFacts {
        items: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
        blocks: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    };
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["item_predicates"].as_array().unwrap() {
        let test = crate::predicates::ItemPredicate::decode(&record["predicate"]).unwrap();
        assert_eq!(
            test.matches(&decode_stack(&record["item"]), &facts)
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
}

#[test]
fn all_available_builtin_tables_match_real_java() {
    let resources = vanilla_resources();
    let mut data = LootData::default();
    let mut pending = std::collections::BTreeMap::new();
    // Only this test uses a partial reload: production load remains atomic and
    // rejects unsupported codecs. Each pending key retains its missing fact.
    macro_rules! decode {
        ($field:ident,$ty:ty,$directory:literal) => {
            for (id, resource) in resources.registry($directory) {
                let id = Identifier::parse(&id.to_string()).unwrap();
                match <$ty>::decode(&resource.json().unwrap()) {
                    Ok(value) => {
                        data.$field.insert(id, value);
                    }
                    Err(LootError::Unsupported(fact)) => {
                        pending.insert(($directory, id), fact);
                    }
                    Err(e) => panic!("{id}: {e}"),
                }
            }
        };
    }
    decode!(tables, LootTable, "loot_table");
    decode!(predicates, LootItemCondition, "predicate");
    decode!(modifiers, LootItemFunction, "item_modifier");
    decode!(int_providers, ContextIntProvider, "context_int_provider");
    decode!(
        float_providers,
        ContextFloatProvider,
        "context_float_provider"
    );
    let facts = ItemFacts {
        items: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
        blocks: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    };
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    let mut matched = 0;
    let mut pending_counts = std::collections::BTreeMap::<String, usize>::new();
    for record in vectors["vectors"].as_array().unwrap() {
        let id = Identifier::parse(record["table"].as_str().unwrap()).unwrap();
        let seed = record["seed"].as_i64().unwrap();
        let Some(table) = data.tables.get(&id) else {
            *pending_counts
                .entry(pending[&("loot_table", id.clone())].clone())
                .or_default() += 1;
            continue;
        };
        let allowed = ContextKeySet::vanilla(&table.param_set).unwrap().allowed;
        let mut params = LootParams::default();
        if allowed.contains(&ContextParam::Origin) {
            params.origin = Some(ferraloom_util::Vec3::new(0.5, 64.0, 0.5));
        }
        if allowed.contains(&ContextParam::ThisEntity) {
            params.this_entity = Some(crate::entity::EntityRef(1));
        }
        if allowed.contains(&ContextParam::Tool) {
            params.tool = Some(ItemStack::new(
                Item::DIAMOND_PICKAXE,
                1,
                DataComponentPatch::default(),
            ));
        }
        if allowed.contains(&ContextParam::BlockState) {
            let block = id
                .path()
                .strip_prefix("blocks/")
                .and_then(|name| ferraloom_data::Block::from_name(&format!("minecraft:{name}")))
                .unwrap_or(ferraloom_data::Block::STONE);
            params.block_state = Some(block.default_state());
        }
        let mut random = LegacyRandomSource::new(seed);
        let items = match data.get_random_items(&id, &params, &mut random, &facts) {
            Ok(items) => items,
            Err(LootError::Unsupported(fact)) => {
                *pending_counts.entry(fact).or_default() += 1;
                continue;
            }
            Err(LootError::MissingReference {
                registry,
                id: missing,
            }) if pending.contains_key(&(registry, missing.clone())) => {
                *pending_counts
                    .entry(pending[&(registry, missing)].clone())
                    .or_default() += 1;
                continue;
            }
            Err(e) => panic!("{id}, seed {seed}: {e}"),
        };
        assert_stacks(&items, &record["items"], &format!("{id} seed {seed}"));
        assert_eq!(
            random.next_long(),
            record["next"].as_i64().unwrap(),
            "{id} seed {seed} next RNG"
        );
        let mut random = LegacyRandomSource::new(seed);
        let mut container = vec![ItemStack::default(); 27];
        data.fill(&id, &mut container, &params, &mut random, &facts)
            .unwrap();
        assert_stacks(
            &container,
            &record["slots"],
            &format!("{id} seed {seed} fill"),
        );
        assert_eq!(
            random.next_long(),
            record["fill_next"].as_i64().unwrap(),
            "{id} seed {seed} fill RNG"
        );
        matched += 1;
    }
    assert!(matched > 0);
    eprintln!(
        "Built-in vanilla loot vectors matched: {matched}; pending by missing fact: {pending_counts:?}"
    );
}

#[test]
fn random_sequence_seeds_and_saved_words_match_java() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["random_sequences"].as_array().unwrap() {
        let id = Identifier::parse(record["key"].as_str().unwrap()).unwrap();
        let world_seed = record["world_seed"].as_i64().unwrap();
        let mut sequences = RandomSequences::default();
        sequences.salt = record["salt"].as_i64().unwrap() as i32;
        sequences.include_world_seed = record["include_world_seed"].as_bool().unwrap();
        sequences.include_sequence_id = record["include_sequence_id"].as_bool().unwrap();
        for value in record["draws"].as_array().unwrap() {
            assert_eq!(
                sequences.get(&id, world_seed).next_long(),
                value.as_i64().unwrap(),
                "{record}"
            );
        }
        let actual = sequences.get(&id, world_seed).seed_state();
        let words = &record["saved"]["sequences"][id.as_str()]["source"];
        assert_eq!(actual.seed_lo, words[0].as_i64().unwrap());
        assert_eq!(actual.seed_hi, words[1].as_i64().unwrap());
    }
}
#[test]
fn block_and_location_acceptance_matches_java() {
    use crate::predicates::*;
    let resources = vanilla_resources();
    let item_facts = ItemFacts {
        items: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
        blocks: ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    };
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["block_predicates"].as_array().unwrap() {
        let id = Identifier::parse(record["block"].as_str().unwrap()).unwrap();
        let state = ferraloom_data::Block::from_name(id.as_str())
            .unwrap()
            .default_state();
        assert_eq!(
            BlockPredicate::decode(&record["predicate"])
                .unwrap()
                .matches_state(state, &item_facts)
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
    struct LocationFacts;
    impl PredicateContext for LocationFacts {
        fn loaded(&self, _pos: ferraloom_util::BlockPos) -> Result<bool> {
            Ok(true)
        }
        fn dimension(&self) -> Result<Identifier> {
            Ok(Identifier::parse("overworld")?)
        }
        fn light(&self, _pos: ferraloom_util::BlockPos) -> Result<i32> {
            Ok(7)
        }
        fn can_see_sky(&self, _pos: ferraloom_util::BlockPos) -> Result<bool> {
            Ok(true)
        }
        fn block(&self, _pos: ferraloom_util::BlockPos) -> Result<ferraloom_data::BlockState> {
            Ok(ferraloom_data::Block::STONE.default_state())
        }
    }
    for record in vectors["location_predicates"].as_array().unwrap() {
        let point = ferraloom_util::Vec3::new(0.5, record["y"].as_f64().unwrap(), -0.5);
        assert_eq!(
            LocationPredicate::decode(&record["predicate"])
                .unwrap()
                .matches(point, &LocationFacts)
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
}
