use super::*;
use ferraloom_data::Item;
use ferraloom_protocol::component::{DataComponentPatch, ItemStack};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use serde_json::json;

// The Java harness explicitly enables FeatureFlags.VANILLA_SET.
struct VanillaFeatures;
impl crate::predicates::PredicateContext for VanillaFeatures {}
impl LootFacts for VanillaFeatures {
    fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(true)
    }
}

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
        runtime: &mut super::runtime::BorrowedRuntime::new(&mut random, &VanillaFeatures),
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
        data.get_random_items(&id, &params, &mut actual, &VanillaFeatures)
            .unwrap(),
        vec![ItemStack::new(
            Item::STONE,
            1,
            DataComponentPatch::default()
        )]
    );
    let condition=Holder::direct(LootItemCondition::decode(&json!({"type":"int_value_check","value":{"type":"uniform","min":0,"max":20},"test":{}})).unwrap());
    assert!(
        data.test_predicate(&condition, &params, &mut actual, &VanillaFeatures)
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
fn assert_stacks(
    actual: &[ItemStack],
    expected: &serde_json::Value,
    expected_nbt: &serde_json::Value,
    label: &str,
) {
    assert_eq!(
        actual.len(),
        expected_nbt.as_array().unwrap().len(),
        "{label}"
    );
    for (actual, expected) in actual.iter().zip(expected_nbt.as_array().unwrap()) {
        assert_native_stack(actual, expected, label);
    }
    let actual = actual.iter().map(canonical).collect::<Vec<_>>();
    let expected = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| canonical(&decode_stack(v)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "{label}");
}
fn assert_native_stack(actual: &ItemStack, expected: &serde_json::Value, label: &str) {
    use ferraloom_protocol::component::Persistent;
    if expected.is_null() {
        assert!(actual.is_empty(), "{label}");
    } else {
        assert_eq!(
            actual.to_nbt().unwrap(),
            ferraloom_nbt::parse(expected.as_str().unwrap()).unwrap(),
            "{label}"
        );
    }
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
            .get_random_items(&id, &params, &mut random, &VanillaFeatures)
            .unwrap();
        assert_stacks(
            &items,
            &record["items"],
            &record["items_nbt"],
            &record.to_string(),
        );
        assert_eq!(random.next_long(), record["next"].as_i64().unwrap());
        let mut random = LegacyRandomSource::new(seed);
        let mut container = vec![ItemStack::default(); 9];
        container[3] = ItemStack::new(Item::APPLE, 1, DataComponentPatch::default());
        data.fill(&id, &mut container, &params, &mut random, &VanillaFeatures)
            .unwrap();
        assert_stacks(
            &container,
            &record["slots"],
            &record["slots_nbt"],
            &record.to_string(),
        );
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
                    runtime: &mut super::runtime::BorrowedRuntime::new(
                        &mut random,
                        &VanillaFeatures,
                    ),
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

fn assert_placements(actual: &[(usize, ItemStack)], expected: &serde_json::Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for ((slot, stack), event) in actual.iter().zip(expected) {
        assert_eq!(*slot as u64, event["slot"].as_u64().unwrap());
        assert_native_stack(stack, &event["stack_nbt"], &event.to_string());
    }
}

#[test]
fn fill_placements_preserve_java_delivery_visibility_and_callback_rng() {
    use std::cell::Cell;
    struct GenerationFacts(Cell<bool>);
    impl crate::predicates::PredicateContext for GenerationFacts {}
    impl LootFacts for GenerationFacts {
        fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
            assert!(
                !self.0.get(),
                "feature checks precede the canonical slot scan"
            );
            Ok(true)
        }
    }
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["synthetic_tables"].as_array().unwrap() {
        let id = Identifier::parse("test").unwrap();
        let mut data = LootData::default();
        data.tables.insert(
            id.clone(),
            LootTable::decode(&record["definition"]).unwrap(),
        );
        let params = LootParams {
            luck: 0.75,
            ..Default::default()
        };
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        let mut sequences = RandomSequences::default();
        let mut container = vec![ItemStack::default(); 9];
        container[3] = ItemStack::new(Item::APPLE, 1, DataComponentPatch::default());
        let facts = GenerationFacts(Cell::new(false));
        let placements = data
            .fill_seeded_placements(
                &id,
                &params,
                0,
                &mut LootRandomSources {
                    world_seed: 0,
                    sequences: &mut sequences,
                    level_random: &mut random,
                },
                &facts,
                || {
                    assert!(!facts.0.replace(true));
                    Ok(container
                        .iter()
                        .enumerate()
                        .filter_map(|(slot, stack)| stack.is_empty().then_some(slot))
                        .collect())
                },
            )
            .unwrap();
        assert!(facts.0.get());
        assert_placements(&placements, &record["placements"]);
        assert_placements(&placements, &record["deliveries"]);
        for ((slot, stack), event) in placements
            .into_iter()
            .zip(record["deliveries"].as_array().unwrap())
        {
            container[slot] = stack;
            assert_eq!(
                random.next_long(),
                event["next"].as_i64().unwrap(),
                "{record}"
            );
            for (stack, expected) in container
                .iter()
                .zip(event["visible_nbt"].as_array().unwrap())
            {
                assert_native_stack(stack, expected, &event.to_string());
            }
        }
        assert_eq!(
            random.next_long(),
            record["delivery_next"].as_i64().unwrap(),
            "{record}"
        );
    }
}

struct ItemFacts {
    items: ferraloom_data::datapack::TagSet<Item>,
    blocks: ferraloom_data::datapack::TagSet<ferraloom_data::Block>,
    registry_tags: std::collections::BTreeMap<(&'static str, Identifier), Vec<Identifier>>,
}
impl ItemFacts {
    fn load(resources: &ferraloom_data::datapack::ResourceManager) -> Self {
        let mut registry_tags = std::collections::BTreeMap::new();
        for registry in ["instrument", "damage_type"] {
            let entries = resources.registry(registry);
            let values = ferraloom_data::datapack::load_tags(resources, registry, |id, _| {
                entries
                    .contains_key(id)
                    .then(|| Identifier::parse(&id.to_string()).unwrap())
            });
            for (id, values) in values.values {
                registry_tags.insert(
                    (registry, Identifier::parse(&id.to_string()).unwrap()),
                    values,
                );
            }
        }
        Self {
            items: ferraloom_data::datapack::TagSet::load(resources).unwrap(),
            blocks: ferraloom_data::datapack::TagSet::load(resources).unwrap(),
            registry_tags,
        }
    }
}
impl crate::predicates::PredicateContext for ItemFacts {
    fn registry_tag_values(
        &self,
        registry: crate::predicates::PredicateRegistry,
        tag: &Identifier,
    ) -> Result<Vec<Identifier>> {
        let key = match registry {
            crate::predicates::PredicateRegistry::Instrument => "instrument",
            crate::predicates::PredicateRegistry::DamageType => "damage_type",
            _ => {
                return Err(LootError::Unsupported(format!(
                    "registry.{registry:?}.tag_values"
                )));
            }
        };
        self.registry_tags
            .get(&(key, tag.clone()))
            .cloned()
            .ok_or_else(|| LootError::MissingReference {
                registry: key,
                id: tag.clone(),
            })
    }
    fn tag_contains(
        &self,
        registry: crate::predicates::PredicateRegistry,
        tag: &Identifier,
        value: &Identifier,
    ) -> Result<bool> {
        Ok(self.registry_tag_values(registry, tag)?.contains(value))
    }

    fn item_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Item>> {
        Ok(&self.items)
    }
    fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::Block>> {
        Ok(&self.blocks)
    }
    fn entity_equipment(
        &self,
        _entity: crate::entity::EntityRef,
    ) -> Result<Option<Vec<(ferraloom_protocol::component::EquipmentSlot, ItemStack)>>> {
        Ok(None)
    }
    fn entity_specific(
        &self,
        _entity: crate::entity::EntityRef,
    ) -> Result<crate::predicates::EntitySpecificFacts> {
        Ok(crate::predicates::EntitySpecificFacts::Other)
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
    fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
        Ok(true)
    }
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
    let facts = ItemFacts::load(&resources);
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["item_predicates"].as_array().unwrap() {
        let test = crate::predicates::ItemPredicate::decode(&record["predicate"]).unwrap();
        test.bind_registries(Some(&OracleRegistries(&resources)))
            .unwrap();
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
    let data = LootData::load(&resources, Some(&OracleRegistries(&resources))).unwrap();
    let facts = ItemFacts::load(&resources);
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    let mut matched = 0;
    let mut matched_tables = std::collections::BTreeSet::new();
    let mut pending_tables = std::collections::BTreeMap::<String, usize>::new();
    let mut pending_counts = std::collections::BTreeMap::<String, usize>::new();
    for record in vectors["vectors"].as_array().unwrap() {
        let id = Identifier::parse(record["table"].as_str().unwrap()).unwrap();
        let seed = record["seed"].as_i64().unwrap();
        let table = data.tables.get(&id).unwrap();
        let allowed = ContextKeySet::vanilla(&table.param_set).unwrap().allowed;
        let mut params = LootParams::default();
        if allowed.contains(&ContextParam::Origin) {
            params.origin = Some(ferraloom_util::Vec3::new(0.5, 64.0, 0.5));
        }
        if allowed.contains(&ContextParam::ThisEntity) {
            params.this_entity = Some(crate::entity::EntityRef::fixture(
                crate::level::RegionId::FIXTURE,
                1,
            ));
        }
        if allowed.contains(&ContextParam::DamageSource) {
            params.damage_source = Some(DamageSource {
                damage_type: Identifier::parse("generic").unwrap(),
                source: None,
                direct: None,
                position: None,
            });
        }
        if allowed.contains(&ContextParam::TargetEntity) {
            params.target_entity = Some(crate::entity::EntityRef::fixture(
                crate::level::RegionId::FIXTURE,
                2,
            ));
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
                *pending_tables.entry(id.to_string()).or_default() += 1;
                *pending_counts.entry(fact).or_default() += 1;
                continue;
            }
            Err(e) => panic!("{id}, seed {seed}: {e}"),
        };
        assert_stacks(
            &items,
            &record["items"],
            &record["items_nbt"],
            &format!("{id} seed {seed}"),
        );
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
            &record["slots_nbt"],
            &format!("{id} seed {seed} fill"),
        );
        assert_eq!(
            random.next_long(),
            record["fill_next"].as_i64().unwrap(),
            "{id} seed {seed} fill RNG"
        );
        let mut random = LegacyRandomSource::new(seed);
        let placements = data
            .fill_placements(&id, &params, &mut random, &facts, || Ok((0..27).collect()))
            .unwrap();
        assert_placements(&placements, &record["placements"]);
        assert_eq!(random.next_long(), record["fill_next"].as_i64().unwrap());
        matched_tables.insert(id);
        matched += 1;
    }
    assert!(matched > 0);
    eprintln!(
        "Built-in tables with matched seed cases: {}; pending seed counts by table: {}",
        matched_tables.len(),
        serde_json::to_string(&pending_tables).unwrap()
    );
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
fn named_sequence_dirty_marking_matches_java() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    let id = Identifier::parse("entities/pig").unwrap();
    for record in vectors["sequence_dirty"].as_array().unwrap() {
        let mut sequences = RandomSequences::default();
        if record["initial_dirty"].as_bool().unwrap() {
            sequences.get(&id, 42).next_int();
            // Restore the initial source without clearing vanilla's dirty flag.
            sequences.reset(id.clone(), 42, 0, true, true);
        } else {
            sequences.get(&id, 42);
        }
        match record["action"].as_str().unwrap() {
            "get" => {
                sequences.get(&id, 42);
            }
            "defaults" => sequences.set_seed_defaults(71, false, false),
            "clear" => {
                sequences.clear();
            }
            "reset" => sequences.reset(id.clone(), 42, -3, false, true),
            "consume_zero" => sequences.get(&id, 42).consume_count(0),
            "consume_negative" => sequences.get(&id, 42).consume_count(-1),
            "int" => {
                sequences.get(&id, 42).next_int();
            }
            "bound" => {
                sequences.get(&id, 42).next_int_bound(17);
            }
            "long" => {
                sequences.get(&id, 42).next_long();
            }
            "boolean" => {
                sequences.get(&id, 42).next_boolean();
            }
            "float" => {
                sequences.get(&id, 42).next_float();
            }
            "double" => {
                sequences.get(&id, 42).next_double();
            }
            "gaussian" => {
                sequences.get(&id, 42).next_gaussian();
            }
            "fork" => {
                sequences.get(&id, 42).fork();
            }
            "positional" => {
                sequences.get(&id, 42).fork_positional();
            }
            "seed" => sequences.get(&id, 42).set_seed(-17),
            "consume_one" => sequences.get(&id, 42).consume_count(1),
            action => panic!("unexpected action {action}"),
        }
        assert_eq!(
            sequences.is_dirty(),
            record["dirty"].as_bool().unwrap(),
            "{record}"
        );
        let saved = sequences.to_nbt();
        let expected = ferraloom_nbt::parse(record["saved_nbt"].as_str().unwrap()).unwrap();
        assert_eq!(saved, expected, "{record}");
        let restored = RandomSequences::from_nbt(&saved).unwrap();
        assert!(!restored.is_dirty());
        assert_eq!(
            sequences.get(&id, 42).next_long(),
            record["next"].as_i64().unwrap(),
            "{record}"
        );
    }
    for record in vectors["sequence_codecs"].as_array().unwrap() {
        let input = ferraloom_nbt::parse(record["input"].as_str().unwrap()).unwrap();
        let decoded = RandomSequences::from_nbt(&input);
        assert_eq!(
            decoded.is_ok(),
            record["accepted"].as_bool().unwrap(),
            "{record}"
        );
        if let Ok(decoded) = decoded {
            let expected = ferraloom_nbt::parse(record["output"].as_str().unwrap()).unwrap();
            assert_eq!(decoded.to_nbt(), expected, "{record}");
        }
    }
}
#[test]
fn block_and_location_acceptance_matches_java() {
    use crate::predicates::*;
    let resources = vanilla_resources();
    let item_facts = ItemFacts::load(&resources);
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

// The real Java harness binds against this same vanilla datapack plus static
// registries. Production supplies the validated server registry snapshot instead.
struct OracleRegistries<'a>(&'a ferraloom_data::datapack::ResourceManager);
impl RegistryLookup for OracleRegistries<'_> {
    fn lookup(
        &self,
        registry: RegistryKey,
        reference: &RegistryReference,
    ) -> Result<Option<RegistryHolder>> {
        use ferraloom_data::registries as r;
        let id = reference.id();
        let exists = match reference {
            RegistryReference::Tag(_) => self
                .0
                .get(
                    &format!(
                        "{}:tags/{}/{}.json",
                        id.namespace(),
                        registry.name(),
                        id.path()
                    )
                    .parse()
                    .unwrap(),
                )
                .is_some(),
            RegistryReference::Value(_) => match registry {
                RegistryKey::Item => Item::from_name(id.as_str()).is_some(),
                RegistryKey::Block => ferraloom_data::Block::from_name(id.as_str()).is_some(),
                RegistryKey::EntityType => {
                    ferraloom_data::EntityType::from_name(id.as_str()).is_some()
                }
                RegistryKey::Fluid => r::fluid::from_name(id.as_str()).is_some(),
                RegistryKey::Potion => r::potion::from_name(id.as_str()).is_some(),
                RegistryKey::MobEffect => r::mob_effect::from_name(id.as_str()).is_some(),
                RegistryKey::Attribute => r::attribute::from_name(id.as_str()).is_some(),
                RegistryKey::VillagerType => r::villager_type::from_name(id.as_str()).is_some(),
                RegistryKey::EnvironmentAttribute => {
                    r::environment_attribute::from_name(id.as_str()).is_some()
                }
                _ => self
                    .0
                    .get(
                        &format!("{}:{}/{}.json", id.namespace(), registry.name(), id.path())
                            .parse()
                            .unwrap(),
                    )
                    .is_some(),
            },
        };
        Ok(exists.then(|| RegistryHolder {
            registry,
            reference: reference.clone(),
        }))
    }
}
#[test]
fn dynamic_registry_binding_matches_java_and_requires_a_lookup() {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    let resources = vanilla_resources();
    let lookup = OracleRegistries(&resources);
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["registry_codecs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["kind"] == "dynamic_function")
    {
        let mut pack = Pack::new("modifiers");
        pack.insert(
            "test:item_modifier/test.json".parse().unwrap(),
            serde_json::to_vec(&record["definition"]).unwrap(),
        );
        let custom = ResourceManager::new(vec![pack]);
        assert!(
            matches!(LootData::load(&custom, None), Err(LootError::Unsupported(fact)) if fact == "registry.binding"),
            "{record}"
        );
        assert_eq!(
            LootData::load(&custom, Some(&lookup)).is_ok(),
            record["accepted"].as_bool().unwrap(),
            "{record}"
        );
    }
}
#[test]
fn fixed_registry_rejection_and_normalized_keys_match_java() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["registry_codecs"].as_array().unwrap() {
        let value = &record["definition"];
        let accepted = match record["kind"].as_str().unwrap() {
            "entry" => LootPoolEntryContainer::decode(value).is_ok(),
            "item" => crate::predicates::ItemPredicate::decode(value).is_ok(),
            "block" => crate::predicates::BlockPredicate::decode(value).is_ok(),
            "entity" => crate::predicates::EntityPredicate::decode(value).is_ok(),
            "fluid" => crate::predicates::FluidPredicate::decode(value).is_ok(),
            "function" => LootItemFunction::decode(value).is_ok(),
            "dynamic_function" => continue,
            kind => panic!("unexpected registry codec {kind}"),
        };
        assert_eq!(accepted, record["accepted"].as_bool().unwrap(), "{record}");
    }
}
#[test]
fn named_modifier_registry_and_tags_match_inline_java_function() {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["modifiers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["definition"]["type"] == "limit_count")
    {
        let mut pack = Pack::new("modifiers");
        pack.insert(
            "test:item_modifier/direct.json".parse().unwrap(),
            serde_json::to_vec(&record["definition"]).unwrap(),
        );
        pack.insert(
            "test:item_modifier/alias.json".parse().unwrap(),
            br##""#test:chain""##.to_vec(),
        );
        pack.insert(
            "test:tags/item_modifier/chain.json".parse().unwrap(),
            br#"{"values":["test:direct"]}"#.to_vec(),
        );
        let data = LootData::load(&ResourceManager::new(vec![pack]), None).unwrap();
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        let result = data
            .apply_modifier(
                &Holder::Reference(Identifier::parse("test:alias").unwrap()),
                decode_stack(&record["input"]),
                &LootParams::default(),
                &mut random,
                &VanillaFeatures,
            )
            .unwrap();
        assert_native_stack(&result, &record["result_nbt"], &record.to_string());
        assert_eq!(
            random.next_long(),
            record["next"].as_i64().unwrap(),
            "{record}"
        );
    }
}
#[test]
fn item_modifiers_and_float_arithmetic_match_java() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    let data = LootData::default();
    for record in vectors["book_page_codecs"].as_array().unwrap() {
        let text = record["character"]
            .as_str()
            .unwrap()
            .repeat(record["length"].as_u64().unwrap() as usize);
        let definition =
            json!({"type":"set_written_book_pages","mode":"replace_all","pages":[text]});
        assert_eq!(
            LootItemFunction::decode(&definition).is_ok(),
            record["accepts"].as_bool().unwrap(),
            "{record}"
        );
    }
    for record in vectors["modifiers"].as_array().unwrap() {
        let modifier = Holder::direct(LootItemFunction::decode(&record["definition"]).unwrap());
        let params = LootParams {
            tool: Some(decode_stack(&record["tool"])),
            block_entity: record
                .get("block_entity")
                .map(|v| ferraloom_nbt::parse(v.as_str().unwrap()).unwrap()),
            block_state: Some(ferraloom_data::Block::WHEAT.default_state()),
            explosion_radius: Some(3.0),
            ..Default::default()
        };
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        let result = data
            .apply_modifier(
                &modifier,
                decode_stack(&record["input"]),
                &params,
                &mut random,
                &VanillaFeatures,
            )
            .unwrap();
        assert_native_stack(&result, &record["result_nbt"], &record.to_string());
        assert_eq!(
            canonical(&result),
            canonical(&decode_stack(&record["result"])),
            "{record}"
        );
        assert_eq!(
            random.next_long(),
            record["next"].as_i64().unwrap(),
            "{record}"
        );
    }
    for record in vectors["float_providers"].as_array().unwrap() {
        let provider = Holder::direct(ContextFloatProvider::decode(&record["definition"]).unwrap());
        let params = LootParams::default();
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        let result = data
            .evaluate_float(&provider, &params, &mut random, &VanillaFeatures)
            .unwrap();
        assert_eq!(
            result.to_bits() as i32,
            record["bits"].as_i64().unwrap() as i32,
            "{record}"
        );
        assert_eq!(random.next_long(), record["next"].as_i64().unwrap());
    }
}

#[test]
fn movement_distance_fluid_and_environment_codecs_match_java() {
    use crate::predicates::{
        DistancePredicate, FluidPredicate, FluidState, MovementPredicate, PredicateContext,
    };
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    for record in vectors["movement_predicates"].as_array().unwrap() {
        assert_eq!(
            MovementPredicate::decode(&record["predicate"])
                .unwrap()
                .matches(
                    ferraloom_util::Vec3::new(record["x"].as_f64().unwrap(), -0.1, 0.05),
                    0.75
                ),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
    for record in vectors["distance_predicates"].as_array().unwrap() {
        assert_eq!(
            DistancePredicate::decode(&record["predicate"])
                .unwrap()
                .matches(
                    ferraloom_util::Vec3::new(0.0, 0.0, 0.0),
                    ferraloom_util::Vec3::new(record["x"].as_f64().unwrap(), 0.5, 0.0)
                ),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
    for record in vectors["double_matches"].as_array().unwrap() {
        let bounds = crate::predicates::DoubleBounds {
            min: record
                .get("min")
                .map(|v| v.as_str().unwrap().parse().unwrap()),
            max: record
                .get("max")
                .map(|v| v.as_str().unwrap().parse().unwrap()),
        };
        let value = record["value"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            bounds.matches(value),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
        assert_eq!(
            bounds.matches_sqr(value),
            record["matches_sqr"].as_bool().unwrap(),
            "{record}"
        );
    }
    struct FluidFacts(FluidState);
    impl PredicateContext for FluidFacts {
        fn loaded(&self, _pos: ferraloom_util::BlockPos) -> Result<bool> {
            Ok(true)
        }
        fn fluid(&self, _pos: ferraloom_util::BlockPos) -> Result<FluidState> {
            Ok(self.0.clone())
        }
    }
    for record in vectors["fluid_predicates"].as_array().unwrap() {
        let facts = FluidFacts(FluidState {
            fluid: Identifier::parse(record["fluid"].as_str().unwrap()).unwrap(),
            properties: record["properties"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect(),
        });
        assert_eq!(
            FluidPredicate::decode(&record["predicate"])
                .unwrap()
                .matches(ferraloom_util::BlockPos::new(0, 0, 0), &facts)
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
    for record in vectors["environment_codecs"].as_array().unwrap() {
        assert_eq!(
            LootItemCondition::decode(&record["definition"]).is_ok(),
            record["accepts"].as_bool().unwrap(),
            "{record}"
        );
    }
    let source = DamageSource {
        damage_type: Identifier::parse("generic").unwrap(),
        source: None,
        direct: None,
        position: None,
    };
    for record in vectors["damage_predicates"].as_array().unwrap() {
        assert_eq!(
            crate::predicates::DamagePredicate::decode(&record["predicate"])
                .unwrap()
                .matches(
                    ferraloom_util::Vec3::new(0.0, 64.0, 0.0),
                    &source,
                    record["original"].as_f64().unwrap() as f32,
                    1.5,
                    record["blocked"].as_bool().unwrap(),
                    &VanillaFeatures
                )
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
    }
}

#[test]
fn conditions_match_real_java() {
    struct ConditionFacts;
    impl crate::predicates::PredicateContext for ConditionFacts {
        fn loaded(&self, _pos: ferraloom_util::BlockPos) -> Result<bool> {
            Ok(true)
        }
        fn light(&self, _pos: ferraloom_util::BlockPos) -> Result<i32> {
            Ok(7)
        }
    }
    impl LootFacts for ConditionFacts {
        fn weather(&self) -> Result<(bool, bool)> {
            Ok((true, false))
        }
        fn clock_ticks(&self, _clock: &Identifier) -> Result<i64> {
            Ok(23567)
        }
        fn environment_value(
            &self,
            _attribute: &Identifier,
            _params: &LootParams,
        ) -> Result<EnvironmentValue> {
            Ok(EnvironmentValue::Float(7.0))
        }
    }
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/java/loot/vectors.json")).unwrap();
    let data = LootData::default();
    for record in vectors["conditions"].as_array().unwrap() {
        let condition = Holder::direct(LootItemCondition::decode(&record["definition"]).unwrap());
        let params = LootParams {
            origin: Some(ferraloom_util::Vec3::new(0.5, 64.0, 0.5)),
            tool: Some(decode_stack(&record["tool"])),
            block_state: Some(ferraloom_data::Block::WHEAT.default_state()),
            explosion_radius: Some(3.0),
            enchantment_active: Some(true),
            damage_source: Some(DamageSource {
                damage_type: Identifier::parse("generic").unwrap(),
                source: None,
                direct: None,
                position: None,
            }),
            ..Default::default()
        };
        let mut random = LegacyRandomSource::new(record["seed"].as_i64().unwrap());
        assert_eq!(
            data.test_predicate(&condition, &params, &mut random, &ConditionFacts)
                .unwrap(),
            record["matches"].as_bool().unwrap(),
            "{record}"
        );
        assert_eq!(
            random.next_long(),
            record["next"].as_i64().unwrap(),
            "{record}"
        );
    }
}
