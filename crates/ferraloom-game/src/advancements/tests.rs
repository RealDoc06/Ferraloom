use super::triggers::*;
use super::*;
use crate::{entity::EntityRef, loot::*, predicates::PredicateContext};
use ferraloom_data::{Block, EntityType, Item};
use ferraloom_protocol::{
    Wire, Writer,
    component::{ItemStack, Persistent, PotionContents},
};
use ferraloom_util::{
    Vec3,
    random::{LegacyRandomSource, RandomSource},
};
use serde_json::{Value, json};
use std::{io::Read, sync::OnceLock};

fn oracle() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("vanilla.json.gz")[..])
            .read_to_end(&mut bytes)
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    })
}
fn id(value: &str) -> Identifier {
    Identifier::parse(value).unwrap()
}
fn wire(value: &impl Wire) -> String {
    let mut writer = Writer::new();
    value.encode(&mut writer).unwrap();
    writer
        .as_slice()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn holders(rows: &Value) -> Vec<AdvancementHolder> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| AdvancementHolder {
            id: id(row["id"].as_str().unwrap()),
            value: Advancement::decode(&row["value"])
                .unwrap_or_else(|e| panic!("{}: {e}", row["id"])),
        })
        .collect()
}

#[test]
fn vanilla_builtin_advancements_tree_and_positions() {
    let rows = &oracle()["builtins"];
    let mut manager = ServerAdvancementManager::new(holders(rows));
    assert_eq!(manager.advancements.len(), 1866);
    assert_eq!(manager.tree.nodes().count(), 1866);
    for row in rows.as_array().unwrap() {
        let children = row["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| id(v.as_str().unwrap()))
            .collect();
        assert!(
            manager
                .tree
                .set_child_order(&id(row["id"].as_str().unwrap()), children)
        );
    }
    manager.tree.reposition_nodes();
    for row in rows.as_array().unwrap() {
        let node = manager.tree.get(&id(row["id"].as_str().unwrap())).unwrap();
        assert_eq!(
            (node.x, node.y),
            (
                row["x"].as_f64().unwrap() as f32,
                row["y"].as_f64().unwrap() as f32
            ),
            "{}",
            row["id"]
        );
        assert_eq!(
            node.parent.as_ref().map(Identifier::as_str),
            row["value"]["parent"].as_str()
        );
    }
}
#[test]
fn vanilla_trigger_codecs_and_synthetic_matches() {
    let data = LootData::default();
    let facts = Facts;
    for row in oracle()["trigger_decode"].as_array().unwrap() {
        assert_eq!(
            Criterion::decode(&row["input"]).is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["input"]
        );
    }
    struct Runtime<'a> {
        facts: &'a Facts,
        random: LegacyRandomSource,
    }
    impl LootRuntime for Runtime<'_> {
        fn facts(&self) -> &dyn LootFacts {
            self.facts
        }
        fn with_random(&mut self, operation: &mut dyn FnMut(&mut dyn RandomSource)) {
            operation(&mut self.random);
        }
    }
    impl TriggerRuntime for Runtime<'_> {
        fn trigger_facts(&self) -> &dyn TriggerFacts {
            self.facts
        }
    }
    let mut covered = std::collections::BTreeSet::new();
    for row in oracle()["trigger_matches"].as_array().unwrap() {
        let criterion = Criterion::decode(&row["input"]).unwrap();
        covered.insert(criterion.trigger);
        let n = row["scenario"].as_i64().unwrap() as i32;
        let mut random = LegacyRandomSource::new(42);
        let mut context =
            MatchContext::borrowed(&data, &facts, &mut random, Vec3::new(1.0, 64.0, 2.0));
        let mut runtime = Runtime {
            facts: &facts,
            random: LegacyRandomSource::new(42),
        };
        let mut live_context = MatchContext::live(&data, &mut runtime, context.origin);
        let runtime_matched = scenario(&criterion.instance, n, &mut live_context).unwrap();
        assert_eq!(
            runtime_matched,
            row["matches"].as_bool().unwrap(),
            "live {} scenario {n}",
            row["input"]
        );
        let matched = scenario(&criterion.instance, n, &mut context);
        assert_eq!(
            matched.unwrap_or_else(|e| panic!("{} scenario {n}: {e}", row["input"])),
            row["matches"].as_bool().unwrap(),
            "{} scenario {n}",
            row["input"]
        );
    }
    assert_eq!(covered.len(), TriggerType::ALL.len());
}
struct Facts;
impl PredicateContext for Facts {
    fn entity_type(&self, entity: EntityRef) -> Result<EntityType> {
        Ok(match entity.0 {
            0 => EntityType::PLAYER,
            1 => EntityType::PIG,
            _ => EntityType::COW,
        })
    }
    fn entity_position(&self, _entity: EntityRef) -> Result<Vec3> {
        Ok(Vec3::new(1.0, 64.0, 2.0))
    }
    fn loaded(&self, _pos: ferraloom_util::BlockPos) -> Result<bool> {
        Ok(true)
    }
    fn entity_effects(
        &self,
        _entity: EntityRef,
    ) -> Result<Option<Vec<ferraloom_protocol::component::MobEffectInstance>>> {
        Ok(Some(Vec::new()))
    }
}
impl LootFacts for Facts {
    fn item_enabled(&self, _: &ItemStack) -> Result<bool> {
        Ok(true)
    }
}
impl TriggerFacts for Facts {
    fn hooked_item(&self, _: EntityRef) -> Result<Option<ItemStack>> {
        Ok(None)
    }
}
fn scenario(instance: &TriggerInstance, n: i32, context: &mut MatchContext<'_>) -> Result<bool> {
    use TriggerConditions as C;
    use TriggerEvent as E;
    let origin = context.origin;
    let start = Vec3::new(-3.0, 60.0, 2.0);
    let pig = create_context(origin, EntityRef(1));
    let cow = create_context(origin, EntityRef(2));
    let entity = if n == 1 { &cow } else { &pig };
    let item = match n {
        2 => ItemStack::default(),
        1 => ItemStack::from_json(&json!({"id":"diamond_pickaxe","components":{"damage":17}}))?,
        _ => ItemStack::new(Item::STONE, 5, Default::default()),
    };
    let state = if n == 1 { Block::OAK_LOG } else { Block::STONE }.default_state();
    let location = LootParams {
        origin: Some(origin),
        this_entity: Some(EntityRef(0)),
        tool: Some(item.clone()),
        block_state: Some(state),
        ..Default::default()
    };
    let source = DamageSource {
        damage_type: id("generic"),
        source: Some(EntityRef(1)),
        direct: Some(EntityRef(1)),
        position: None,
    };
    let inventory = [
        ItemStack::new(Item::STONE, 5, Default::default()),
        ItemStack::new(Item::APPLE, 1, Default::default()),
        ItemStack::default(),
        ItemStack::new(Item::STONE, 64, Default::default()),
    ];
    let recipe = id(if n == 1 { "furnace" } else { "crafting_table" });
    let ingredients = if n == 1 {
        vec![item.clone()]
    } else {
        vec![item.clone(), item.clone()]
    };
    let potion = PotionContents::from_json(&json!({"potion":if n==0 {"healing"}else{"water"}}))?;
    let from = id("overworld");
    let to = id(if n == 1 { "the_nether" } else { "overworld" });
    let caught = if n == 2 {
        Vec::new()
    } else {
        vec![ItemStack::new(Item::APPLE, 1, Default::default())]
    };
    let victims = if n == 2 {
        Vec::new()
    } else {
        vec![pig.clone(), cow.clone()]
    };
    let table = id(if n == 1 {
        "chests/spawn_bonus_chest"
    } else {
        "chests/simple_dungeon"
    });
    let event = match &instance.conditions {
        C::Impossible | C::Player => E::Player,
        C::Item(_) => E::Item(&item),
        C::Entity(_) => E::Entity(entity),
        C::Block(_) => E::Block(state),
        C::Location(_) => E::Location(&location),
        C::Killed { .. } => E::Killed {
            entity,
            source: &source,
        },
        C::Inventory { .. } => E::Inventory {
            inventory: &inventory,
            changed: &item,
            full: 1,
            empty: 1,
            occupied: 3,
        },
        C::Recipe { .. } => E::Recipe {
            id: &recipe,
            ingredients: &ingredients,
        },
        C::Hurt { .. } => E::Hurt {
            entity: Some(entity),
            source: &source,
            original: 5.0,
            actual: 3.0,
            blocked: n == 1,
        },
        C::EnchantedItem { .. } => E::EnchantedItem {
            item: &item,
            levels: n + 1,
        },
        C::Potion(_) => E::Potion(&potion),
        C::Beacon(_) => E::Beacon(n + 1),
        C::EnderEye(_) => E::EnderEye(match n {
            0 => 16.0,
            1 => 25.0,
            _ => 26.0,
        }),
        C::Bred { .. } => E::Bred {
            parent: entity,
            partner: &cow,
            child: (n != 2).then_some(&pig),
        },
        C::Cured { .. } => E::Cured {
            zombie: entity,
            villager: &cow,
        },
        C::Trade { .. } => E::Trade {
            villager: entity,
            item: &item,
        },
        C::Durability { .. } => E::Durability {
            item: &item,
            new_damage: 10,
        },
        C::Levitation { .. } => E::Levitation {
            start,
            duration: n * 20,
        },
        C::Dimension { .. } => E::Dimension {
            from: &from,
            to: &to,
        },
        C::Effects { .. } => E::Effects {
            player: EntityRef(0),
            source: (n != 2).then_some(entity),
        },
        C::Distance { .. } => E::Distance {
            start,
            end: origin,
            cause: (n != 2).then_some(entity),
        },
        C::Fishing { .. } => E::Fishing {
            rod: &item,
            hooked: entity,
            items: &caught,
        },
        C::ChanneledLightning(_) => E::ChanneledLightning(&victims),
        C::SpearMobs(_) => E::SpearMobs(n),
        C::KilledByArrow { .. } => E::KilledByArrow {
            victims: &victims,
            unique_entity_types: if n == 2 { 0 } else { 2 },
            fired_from_weapon: (n != 2).then_some(&item),
        },
        C::BeeNest { .. } => E::BeeNest {
            state,
            item: &item,
            bees: n,
        },
        C::Target { .. } => E::Target {
            signal_strength: n,
            projectile: entity,
        },
        C::Loot(_) => E::Loot(&table),
        C::ItemEntity { .. } => E::ItemEntity {
            item: &item,
            entity,
        },
        C::Lightning { .. } => E::Lightning {
            lightning: entity,
            bystanders: &victims,
        },
    };
    instance.matches(event, context)
}
#[test]
fn vanilla_progress_award_revoke_dates_and_wire() {
    for row in oracle()["progress"].as_array().unwrap() {
        let progress = AdvancementProgress::decode(&row["input"]);
        assert_eq!(
            progress.is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["input"]
        );
        let Ok(mut progress) = progress else {
            continue;
        };
        progress.update(&AdvancementRequirements(vec![
            vec!["a".into(), "b".into()],
            vec!["c".into()],
        ]));
        for step in row["steps"].as_array().unwrap() {
            let action = step["action"].as_str().unwrap();
            let changed = if let Some(name) = action.strip_prefix("grant:") {
                progress.grant(name, ObtainedTime::from_millis(123456789))
            } else if let Some(name) = action.strip_prefix("revoke:") {
                progress.revoke(name)
            } else {
                false
            };
            assert_eq!(changed, step["changed"].as_bool().unwrap());
            assert_eq!(progress.is_done(), step["done"].as_bool().unwrap());
            assert_eq!(progress.percent(), step["percent"].as_f64().unwrap() as f32);
            assert_eq!(progress.to_json(|_| 0).unwrap(), step["json"]);
            let network = progress.network(id("test"));
            let mut writer = Writer::new();
            writer.var_int(network.criteria.len() as i32);
            for criterion in network.criteria {
                criterion.encode(&mut writer).unwrap();
            }
            let actual: String = writer
                .as_slice()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(
                actual,
                step["wire"].as_str().unwrap(),
                "{} {action}",
                row["input"]
            );
        }
    }
}
#[test]
fn vanilla_saved_progress_rebuilds_hashmap_wire_order() {
    for row in oracle()["loaded_progress"].as_array().unwrap() {
        let mut progress = AdvancementProgress::decode(&row["input"]).unwrap();
        progress.update(&AdvancementRequirements::decode(&row["requirements"]).unwrap());
        let network = progress.network(id("test"));
        let mut writer = Writer::new();
        writer.var_int(network.criteria.len() as i32);
        for criterion in network.criteria {
            criterion.encode(&mut writer).unwrap();
        }
        let actual: String = writer
            .as_slice()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(actual, row["wire"].as_str().unwrap(), "{}", row["input"]);
    }
}
fn synthetic_manager() -> ServerAdvancementManager {
    let fixture = &oracle()["player"];
    let mut manager = ServerAdvancementManager::new(holders(&fixture["inputs"]));
    for (key, children) in fixture["children"].as_object().unwrap() {
        assert!(
            manager.tree.set_child_order(
                &id(key),
                children
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| id(v.as_str().unwrap()))
                    .collect()
            )
        );
    }
    manager.tree.reposition_nodes();
    manager
}
struct RewardHarness {
    recipes: crate::crafting::RecipeManager,
    sync: crate::crafting::RecipeSynchronization,
}
impl RewardHarness {
    fn new() -> Self {
        let tags = ferraloom_data::datapack::TagSet::<Item>::load(
            &ferraloom_data::datapack::ResourceManager::new(Vec::new()),
        )
        .unwrap();
        let recipe=crate::crafting::Recipe::decode(&json!({"type":"crafting_shapeless","ingredients":["stone"],"result":{"id":"crafting_table"}}),&tags).unwrap();
        let recipes = crate::crafting::RecipeManager::new(vec![crate::crafting::RecipeHolder {
            id: "minecraft:crafting_table".parse().unwrap(),
            value: recipe,
        }])
        .unwrap();
        let mut sync = crate::crafting::RecipeSynchronization::new(&recipes, |_| true);
        sync.displays.clear();
        Self { recipes, sync }
    }
    fn effects(
        &self,
        result: AwardResult,
        player: &mut PlayerAdvancements,
        book: &mut crate::crafting::ServerRecipeBook,
        manager: &ServerAdvancementManager,
        award: AwardContext<'_>,
    ) -> Vec<Value> {
        let mut effects = Vec::new();
        for effect in result.effects {
            match effect {
                AdvancementEffect::Rewards { rewards, .. } => {
                    effects.push(json!(format!("xp:{}", rewards.experience)));
                    let addition = rewards.grant_recipes(book, &self.recipes, &self.sync);
                    for recipe in addition.unlocked {
                        let recipe = id(&recipe.to_string());
                        let origin = Vec3::new(1.0, 64.0, 2.0);
                        let params = create_context(origin, EntityRef(0));
                        let data = LootData::default();
                        let mut random = LegacyRandomSource::new(42);
                        let mut context =
                            MatchContext::borrowed(&data, &Facts, &mut random, origin);
                        let matched = player
                            .matching_listeners(
                                TriggerType::RecipeUnlocked,
                                &params,
                                &mut context,
                                |instance, context| {
                                    instance.matches(
                                        TriggerEvent::Recipe {
                                            id: &recipe,
                                            ingredients: &[],
                                        },
                                        context,
                                    )
                                },
                            )
                            .unwrap();
                        for key in matched {
                            let result =
                                player.award(manager, &key.advancement, &key.criterion, award);
                            effects.extend(self.effects(result, player, book, manager, award));
                        }
                    }
                }
                AdvancementEffect::Announcement(component) => {
                    effects.push(json!(component.as_ref().to_json().unwrap()))
                }
                AdvancementEffect::MarkVisibility(id) => {
                    player.apply_mark_visibility(manager, &id);
                }
            }
        }
        effects
    }
}
#[test]
fn vanilla_player_listeners_visibility_packets_rewards_and_persistence() {
    let manager = synthetic_manager();
    let mut player = PlayerAdvancements::new(&manager);
    let fixture = &oracle()["player"];
    let rewards = RewardHarness::new();
    let mut book = crate::crafting::ServerRecipeBook::default();
    let name = ferraloom_protocol::chat::Component::literal("Doc");
    let award = AwardContext {
        now: ObtainedTime::from_millis(123456789),
        player_name: &name,
        announce_advancements: true,
    };
    for step in fixture["steps"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        let mut packets = Vec::new();
        let mut effects = Vec::new();
        let mut changed = false;
        if action.starts_with("award:") || action.starts_with("revoke:") {
            let parts: Vec<_> = action.split(':').collect();
            let key = id(&format!("{}:{}", parts[1], parts[2]));
            if parts[0] == "award" {
                let result = player.award(&manager, &key, parts[3], award);
                changed = result.changed;
                effects.extend(rewards.effects(result, &mut player, &mut book, &manager, award));
            } else {
                changed = player.revoke(&manager, &key, parts[3]);
            }
        } else if action == "trigger:tick" {
            let origin = Vec3::new(1.0, 64.0, 2.0);
            let params = create_context(origin, EntityRef(0));
            let data = LootData::default();
            let mut random = LegacyRandomSource::new(42);
            let mut context = MatchContext::borrowed(&data, &Facts, &mut random, origin);
            let matched = player
                .matching_listeners(
                    TriggerType::Tick,
                    &params,
                    &mut context,
                    |instance, context| instance.matches(TriggerEvent::Player, context),
                )
                .unwrap();
            for key in matched {
                let result = player.award(&manager, &key.advancement, &key.criterion, award);
                effects.extend(rewards.effects(result, &mut player, &mut book, &manager, award));
            }
        } else if let Some(key) = action.strip_prefix("tab:")
            && let Some(packet) = player.set_selected_tab(if key == "null" {
                None
            } else {
                manager.get(&id(key))
            })
        {
            packets.push(Value::String(wire(&packet)));
        }
        if let Some(packet) = player.flush_dirty(&manager, true) {
            packets.push(Value::String(wire(&packet)));
        }
        assert_eq!(changed, step["changed"].as_bool().unwrap(), "{action}");
        assert_eq!(Value::Array(packets), step["packets"], "{action}");
        let expected: Vec<Value> = step["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                if value.as_str().unwrap().starts_with("xp:") {
                    value.clone()
                } else {
                    json!(serde_json::from_str::<Value>(value.as_str().unwrap()).unwrap())
                }
            })
            .collect();
        let actual: Vec<Value> = effects
            .into_iter()
            .map(|value| {
                if value.as_str().unwrap().starts_with("xp:") {
                    value
                } else {
                    json!(serde_json::from_str::<Value>(value.as_str().unwrap()).unwrap())
                }
            })
            .collect();
        assert_eq!(actual, expected, "{action}");
        assert_eq!(player.to_json(|_| 0).unwrap(), step["saved"], "{action}");
        let mut registered: Vec<_> = TriggerType::ALL
            .iter()
            .flat_map(|&trigger| {
                player.listeners(trigger).map(move |(key, _)| {
                    format!("{}/{}/{}", trigger.name(), key.advancement, key.criterion)
                })
            })
            .collect();
        registered.sort();
        assert_eq!(json!(registered), step["listeners"], "{action}");
        let mut known: Vec<_> = book.known().iter().map(ToString::to_string).collect();
        known.sort();
        assert_eq!(json!(known), step["known_recipes"], "{action}");
        for holder in &manager.advancements {
            let progress = player.get_progress(&holder.id).unwrap();
            for (criterion, definition) in &holder.value.criteria {
                let registered = player
                    .listeners(definition.trigger)
                    .any(|(key, _)| key.advancement == holder.id && key.criterion == *criterion);
                assert_eq!(
                    registered,
                    !progress.is_done() && !progress.criterion_done(criterion),
                    "{action} {} {criterion}",
                    holder.id
                );
            }
        }
    }
    let unknown = player.load_json(&manager, &fixture["load_input"]).unwrap();
    assert_eq!(unknown, vec![id("test:missing")]);
    assert_eq!(player.to_json(|_| 0).unwrap(), fixture["loaded_saved"]);
    assert_eq!(
        Value::Array(
            player
                .flush_dirty(&manager, true)
                .into_iter()
                .map(|p| json!(wire(&p)))
                .collect()
        ),
        fixture["loaded_packets"]
    );
}
#[test]
fn unavailable_trigger_facts_are_errors_and_impossible_never_awards() {
    let criterion=Criterion::decode(&json!({"trigger":"location","conditions":{"player":{"type":"entity_properties","entity":"this","predicate":{"location":{"biomes":"plains"}}}}})).unwrap();
    let data = LootData::default();
    let mut random = LegacyRandomSource::new(0);
    let mut context = MatchContext::borrowed(&data, &NoFacts, &mut random, Vec3::ZERO);
    assert!(
        context
            .predicate(
                criterion.instance.player.as_ref().unwrap(),
                &create_context(Vec3::ZERO, EntityRef(0))
            )
            .is_err()
    );
    let manager = synthetic_manager();
    let mut player = PlayerAdvancements::new(&manager);
    let impossible = Criterion::decode(&json!({"trigger":"impossible"})).unwrap();
    player.add_player_listener(
        &impossible,
        TriggerInstanceKey {
            advancement: id("test:root"),
            criterion: "a".into(),
        },
    );
    assert!(
        player
            .matching_listeners(
                TriggerType::Impossible,
                &LootParams::default(),
                &mut context,
                |_, _| Ok(true)
            )
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        player.load_json(&manager, &json!({"DataVersion":1343})),
        Err(LootError::Unsupported(_))
    ));
}

#[test]
fn vanilla_inline_reward_tables_select_their_own_random_sequence() {
    let fixture = oracle();
    let data = LootData::default();
    let key = id("test:advancement_reward");
    for row in fixture["inline_loot"].as_array().unwrap() {
        let table = Holder::direct(LootTable::decode(&row["input"]).unwrap());
        let mut local = LegacyRandomSource::new(42);
        let mut sequences = RandomSequences::default();
        let mut sources = LootRandomSources {
            world_seed: 123,
            sequences: &mut sequences,
            level_random: &mut local,
        };
        let items = data
            .get_random_items_from_holder(&table, &LootParams::default(), &mut sources, &Facts)
            .unwrap();
        assert_eq!(items.len(), row["count"].as_u64().unwrap() as usize);
        assert_eq!(local.next_long(), row["level_next"].as_i64().unwrap());
        assert_eq!(
            sequences.get(&key, 123).next_long(),
            row["sequence_next"].as_i64().unwrap()
        );
    }
}

#[test]
fn vanilla_advancement_validation_and_empty_requirement_edges() {
    for row in oracle()["advancement_decode"].as_array().unwrap() {
        assert_eq!(
            Advancement::decode(&row["input"]).is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["input"]
        );
    }
}

#[test]
fn vanilla_validation_reports_context_paths_and_retains_advancements() {
    let data = LootData::default();
    let recipes = crate::crafting::RecipeManager::new(Vec::new()).unwrap();
    let tags = std::collections::BTreeMap::new();
    let context = AdvancementRegistryContext {
        loot: &data,
        recipes: &recipes,
        recipe_tags: &tags,
        lookup: None,
    };
    for row in oracle()["validation"].as_array().unwrap() {
        let mut pack = ferraloom_data::datapack::Pack::new("validation");
        pack.insert(
            "test:advancement/warning.json".parse().unwrap(),
            row["input"].to_string().into_bytes(),
        );
        let resources = ferraloom_data::datapack::ResourceManager::new(vec![pack]);
        let manager = ServerAdvancementManager::load(&resources, &context).unwrap();
        assert_eq!(
            manager.get(&id("test:warning")).is_some(),
            row["retained"].as_bool().unwrap()
        );
        let actual: Vec<_> = manager
            .validation_problems
            .iter()
            .map(|p| json!({"path":p.problem.path,"fatal":p.problem.fatal}))
            .collect();
        let expected: Vec<_> = row["problems"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| json!({"path":p["path"],"fatal":p["fatal"]}))
            .collect();
        assert_eq!(actual, expected, "{}", row["input"]);
    }
}

#[test]
fn vanilla_tree_parent_resolution_removal_and_clear() {
    let fixture = &oracle()["tree"];
    let mut tree = AdvancementTree::default();
    let unresolved = tree.add_all(&holders(&fixture["inputs"]));
    assert_eq!(
        unresolved,
        vec![id("test:orphan"), id("test:cycle_a"), id("test:cycle_b")]
    );
    for row in fixture["steps"].as_array().unwrap() {
        let action = row["action"].as_str().unwrap();
        if action == "replace_root" {
            let value = Advancement::decode(
                &json!({"parent":"test:child","criteria":{"a":{"trigger":"tick"}}}),
            )
            .unwrap();
            assert!(
                tree.add_all(&[AdvancementHolder {
                    id: id("test:root"),
                    value
                }])
                .is_empty()
            );
        } else if let Some(key) = action.strip_prefix("remove:") {
            tree.remove(&[id(key)]);
        } else if action == "clear" {
            tree.clear();
        }
        let nodes: Vec<_> = tree
            .nodes()
            .map(|node| json!(node.holder.id.to_string()))
            .collect();
        assert_eq!(Value::Array(nodes), row["nodes"]);
        let children: serde_json::Map<_, _> = tree
            .nodes()
            .map(|node| {
                (
                    node.holder.id.to_string(),
                    Value::Array(
                        node.children
                            .iter()
                            .map(|id| json!(id.to_string()))
                            .collect(),
                    ),
                )
            })
            .collect();
        assert_eq!(Value::Object(children), row["children"]);
        let roots: serde_json::Map<_, _> = tree
            .nodes()
            .map(|node| {
                (
                    node.holder.id.to_string(),
                    json!(tree.root(&node.holder.id).unwrap().holder.id.to_string()),
                )
            })
            .collect();
        assert_eq!(Value::Object(roots), row["roots"]);
    }
}

#[test]
fn vanilla_obtained_time_offsets_years_and_smart_midnight() {
    for row in oracle()["obtained_times"].as_array().unwrap() {
        let time = ObtainedTime::from_millis(row["millis"].as_i64().unwrap());
        assert_eq!(
            time.format(row["offset_seconds"].as_i64().unwrap() as i32)
                .unwrap(),
            row["expected"].as_str().unwrap()
        );
    }
}

#[test]
fn vanilla_nested_reward_visibility_order_and_post_reward_revoke() {
    struct Apply<'a> {
        manager: &'a ServerAdvancementManager,
        award: AwardContext<'a>,
        packets: Vec<Value>,
        effects: Vec<Value>,
        revoke: bool,
    }
    impl Apply<'_> {
        fn flush(&mut self, player: &mut PlayerAdvancements) {
            if let Some(packet) = player.flush_dirty(self.manager, true) {
                self.packets.push(json!(wire(&packet)));
            }
        }
        fn apply(&mut self, player: &mut PlayerAdvancements, result: AwardResult) {
            for effect in result.effects {
                match effect {
                    AdvancementEffect::Rewards {
                        advancement,
                        rewards,
                    } => {
                        self.effects.push(json!("xp"));
                        for recipe in rewards.recipes {
                            let origin = Vec3::new(1.0, 64.0, 2.0);
                            let data = LootData::default();
                            let mut random = LegacyRandomSource::new(42);
                            let mut context =
                                MatchContext::borrowed(&data, &Facts, &mut random, origin);
                            let matched = player
                                .matching_listeners(
                                    TriggerType::RecipeUnlocked,
                                    &create_context(origin, EntityRef(0)),
                                    &mut context,
                                    |instance, context| {
                                        instance.matches(
                                            TriggerEvent::Recipe {
                                                id: &recipe,
                                                ingredients: &[],
                                            },
                                            context,
                                        )
                                    },
                                )
                                .unwrap();
                            for key in matched {
                                let nested = player.award(
                                    self.manager,
                                    &key.advancement,
                                    &key.criterion,
                                    self.award,
                                );
                                self.apply(player, nested);
                            }
                            if self.revoke {
                                assert!(player.revoke(self.manager, &advancement, "a"));
                                self.flush(player);
                            }
                        }
                    }
                    AdvancementEffect::Announcement(_) => self.effects.push(json!("chat")),
                    AdvancementEffect::MarkVisibility(id) => {
                        player.apply_mark_visibility(self.manager, &id);
                    }
                }
            }
        }
    }
    for row in oracle()["nested_visibility"].as_array().unwrap() {
        let manager = ServerAdvancementManager::new(holders(&row["inputs"]));
        let mut player = PlayerAdvancements::new(&manager);
        let name = ferraloom_protocol::chat::Component::literal("Doc");
        let mut apply = Apply {
            manager: &manager,
            award: AwardContext {
                now: ObtainedTime::from_millis(123456789),
                player_name: &name,
                announce_advancements: true,
            },
            packets: Vec::new(),
            effects: Vec::new(),
            revoke: row["revoke"].as_bool().unwrap(),
        };
        let result = player.award(&manager, &id("test:an"), "a", apply.award);
        apply.apply(&mut player, result);
        apply.flush(&mut player);
        assert_eq!(json!(apply.effects), row["effects"]);
        assert_eq!(json!(apply.packets), row["packets"]);
    }
}
