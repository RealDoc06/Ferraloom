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

#[derive(Clone, Copy)]
struct AwardContext<'a> {
    now: ObtainedTime,
    player_name: &'a ferraloom_protocol::chat::Component,
    announce_advancements: bool,
}
fn resolve_announcement(
    manager: &ServerAdvancementManager,
    id: &Identifier,
    award: AwardContext<'_>,
) -> Option<ferraloom_protocol::chat::Component> {
    award
        .announce_advancements
        .then(|| announcement(manager.get(id).unwrap(), award.player_name))
}

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
        Ok(match entity.serial() {
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
    let pig = create_context(
        origin,
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
    );
    let cow = create_context(
        origin,
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
    );
    let entity = if n == 1 { &cow } else { &pig };
    let item = match n {
        2 => ItemStack::default(),
        1 => ItemStack::from_json(&json!({"id":"diamond_pickaxe","components":{"damage":17}}))?,
        _ => ItemStack::new(Item::STONE, 5, Default::default()),
    };
    let state = if n == 1 { Block::OAK_LOG } else { Block::STONE }.default_state();
    let location = LootParams {
        origin: Some(origin),
        this_entity: Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 0)),
        tool: Some(item.clone()),
        block_state: Some(state),
        ..Default::default()
    };
    let source = DamageSource {
        damage_type: id("generic"),
        source: Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)),
        direct: Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)),
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
            player: EntityRef::fixture(crate::level::RegionId::FIXTURE, 0),
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
                        let params = create_context(
                            origin,
                            EntityRef::fixture(crate::level::RegionId::FIXTURE, 0),
                        );
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
                                player.award(manager, &key.advancement, &key.criterion, award.now);
                            effects.extend(self.effects(result, player, book, manager, award));
                        }
                    }
                }
                AdvancementEffect::AnnounceAdvancement(id) => {
                    if let Some(component) = resolve_announcement(manager, &id, award) {
                        effects.push(json!(component.to_json().unwrap()));
                    }
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
                let result = player.award(&manager, &key, parts[3], award.now);
                changed = result.changed;
                effects.extend(rewards.effects(result, &mut player, &mut book, &manager, award));
            } else {
                changed = player.revoke(&manager, &key, parts[3]);
            }
        } else if action == "trigger:tick" {
            let origin = Vec3::new(1.0, 64.0, 2.0);
            let params = create_context(
                origin,
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 0),
            );
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
                let result = player.award(&manager, &key.advancement, &key.criterion, award.now);
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
                &create_context(
                    Vec3::ZERO,
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 0)
                )
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
                                    &create_context(
                                        origin,
                                        EntityRef::fixture(crate::level::RegionId::FIXTURE, 0),
                                    ),
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
                                    self.award.now,
                                );
                                self.apply(player, nested);
                            }
                            if self.revoke {
                                assert!(player.revoke(self.manager, &advancement, "a"));
                                self.flush(player);
                            }
                        }
                    }
                    AdvancementEffect::AnnounceAdvancement(id) => {
                        if resolve_announcement(self.manager, &id, self.award).is_some() {
                            self.effects.push(json!("chat"));
                        }
                    }
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
        let result = player.award(&manager, &id("test:an"), "a", apply.award.now);
        apply.apply(&mut player, result);
        apply.flush(&mut player);
        assert_eq!(json!(apply.effects), row["effects"]);
        assert_eq!(json!(apply.packets), row["packets"]);
    }
}

fn live_fixture(manager: ServerAdvancementManager) -> (crate::level::Level, EntityRef) {
    use crate::{
        Behaviors,
        crafting::{Recipe, RecipeHolder, RecipeManager, RecipeSynchronization},
        entity::{
            EntityState,
            player::{GameType, ServerPlayer},
        },
        level::{Level, Region},
    };
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let tags = ferraloom_data::datapack::TagSet::<Item>::load(
        &ferraloom_data::datapack::ResourceManager::new(Vec::new()),
    )
    .unwrap();
    let recipe = Recipe::decode(&json!({"type":"crafting_shapeless","ingredients":["stone"],"result":{"id":"crafting_table"}}), &tags).unwrap();
    let recipes = Arc::new(
        RecipeManager::new(vec![RecipeHolder {
            id: "crafting_table".parse().unwrap(),
            value: recipe,
        }])
        .unwrap(),
    );
    let sync = Arc::new(RecipeSynchronization::new(&recipes, |_| true));
    let mut level = {
        let mut fixture_level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    if level.shared().is_err() {
        level.install_shared(Box::default());
    }
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        ferraloom_util::Uuid::NIL,
        "Live".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    state.advancements = PlayerAdvancements::new(&manager);
    level.set_container_resources(
        Arc::new(crate::block::containers::runtime::ContainerResources {
            crafting: crate::inventory::RuntimeCraftingContext::fixture(),
            recipes,
            loot: Arc::new(LootData::default()),
            advancements: Arc::new(manager),
            recipe_tags: Default::default(),
            recipe_synchronization: sync,
        }),
        0,
    );
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    level.take_effects();
    (level, player)
}

#[test]
fn native_location_tick_tail_checks_interval_after_experience_packet() {
    use crate::{
        entity::player::{GameType, ServerPlayer, player_do_tick_with_tick_rate},
        level::{effects::LevelEffect, tick_rate::TickRateManager},
    };
    use ferraloom_protocol::play::clientbound as cb;

    let manager = ServerAdvancementManager::new(holders(&json!([{
        "id":"test:location_tick", "value":{
            "criteria":{"location":{"trigger":"location"}},
            "display":{"icon":{"id":"stone"},"title":"Tail","description":"Native interval","background":"test:background"},
            "rewards":{"experience":1}
        }
    }])));
    let (mut level, player) = live_fixture(manager);
    // An unloaded spectator skips the parent Player tick, leaving the native
    // tickCount fixed so the doTick tail can be checked at exact boundaries.
    let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
    owner.game_mode = GameType::Spectator;
    owner.tick_count = 19;
    player_do_tick_with_tick_rate(player, &mut level, &TickRateManager::default()).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .total_experience,
        0
    );
    level.take_effects();

    level.entity_mut::<ServerPlayer>(player).unwrap().tick_count = 20;
    crate::entity::player::give_experience_points(player, 1, &mut level).unwrap();
    player_do_tick_with_tick_rate(player, &mut level, &TickRateManager::default()).unwrap();
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => Some(*packet),
            _ => None,
        })
        .collect();
    assert!(
        matches!(packets.as_slice(), [cb::Packet::SetExperience(experience), cb::Packet::SystemChat(_)] if experience.total_experience.0 == 1)
    );
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    assert_eq!(owner.total_experience, 2);
    assert!(
        owner
            .advancements
            .get_progress(&id("test:location_tick"))
            .unwrap()
            .is_done()
    );

    // The reward's XP is sent at the next native XP slot, without re-awarding
    // the completed advancement at the same interval boundary.
    player_do_tick_with_tick_rate(player, &mut level, &TickRateManager::default()).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .total_experience,
        2
    );
    assert!(
        matches!(level.take_effects().as_slice(), [LevelEffect::PlayerPacket { packet, .. }] if matches!(packet.as_ref(), cb::Packet::SetExperience(experience) if experience.total_experience.0 == 2))
    );
}

#[test]
fn live_location_awards_native_leather_boots_predicate_from_canonical_facts() {
    use crate::{
        entity::{EntityData, player::ServerPlayer},
        level::chunk::LevelChunk,
    };
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    use ferraloom_protocol::component::EquipmentSlot;
    use ferraloom_util::{BlockPos, ChunkPos};
    use std::sync::Arc;

    let target = id("minecraft:adventure/walk_on_powder_snow_with_leather_boots");
    // Keep the captured built-in and its actual parent chain, including display
    // visibility, while isolating this trigger from unrelated advancements.
    let builtins = oracle()["builtins"].as_array().unwrap();
    let mut rows = Vec::new();
    let mut next = Some(target.as_str());
    while let Some(name) = next {
        let row = builtins.iter().find(|row| row["id"] == name).unwrap();
        rows.push(row.clone());
        next = row["value"]["parent"].as_str();
    }
    for (label, boots, count, on_ground, supporting, block, expected) in [
        (
            "fallback",
            Item::LEATHER_BOOTS,
            1,
            true,
            false,
            Block::POWDER_SNOW,
            true,
        ),
        (
            "supporting",
            Item::LEATHER_BOOTS,
            1,
            true,
            true,
            Block::POWDER_SNOW,
            true,
        ),
        (
            "wrong boots",
            Item::IRON_BOOTS,
            1,
            true,
            false,
            Block::POWDER_SNOW,
            false,
        ),
        (
            "empty boots",
            Item::LEATHER_BOOTS,
            0,
            true,
            false,
            Block::POWDER_SNOW,
            false,
        ),
        (
            "airborne",
            Item::LEATHER_BOOTS,
            1,
            false,
            false,
            Block::POWDER_SNOW,
            false,
        ),
        (
            "wrong block",
            Item::LEATHER_BOOTS,
            1,
            true,
            false,
            Block::STONE,
            false,
        ),
    ] {
        let manager = ServerAdvancementManager::new(holders(&json!(rows)));
        let (mut level, player) = live_fixture(manager);
        level
            .add_chunk(LevelChunk::new(
                ChunkPos::new(0, 0),
                -1..1,
                LevelChunkSection::new(
                    Strategy::block_states(),
                    Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                    BiomeId(0),
                ),
            ))
            .unwrap();
        let pos = BlockPos::new(if supporting { 1 } else { 0 }, -1, 0);
        level.set_block_state(pos, block.default_state()).unwrap();
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .parent
            .equipment
            .set(
                EquipmentSlot::Feet,
                ItemStack::new(boots, count, Default::default()),
            );
        let base = level.entity_mut::<EntityData>(player).unwrap();
        base.on_ground = on_ground;
        base.supporting_block_pos = supporting.then_some(pos);

        criteria::location(&mut level, player).unwrap();
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .advancements
                .get_progress(&target)
                .unwrap()
                .is_done(),
            expected,
            "{label}"
        );
    }
}

#[test]
fn live_rewards_nest_recipe_awards_before_outer_announcement_and_visibility() {
    use crate::{entity::player::ServerPlayer, level::effects::LevelEffect};
    use ferraloom_protocol::play::clientbound as cb;
    let row = &oracle()["nested_visibility"][0];
    let mut manager = ServerAdvancementManager::new(holders(&row["inputs"]));
    for holder in &mut manager.advancements {
        holder.value.display.as_mut().unwrap().title =
            ferraloom_protocol::chat::Component::literal(holder.id.to_string());
    }
    let (mut level, player) = live_fixture(manager);
    criteria::tick(&mut level, player).unwrap();
    let resources = level.container_resources().unwrap();
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    assert!(
        owner
            .recipe_book
            .contains(&"crafting_table".parse().unwrap())
    );
    for id in [id("test:an"), id("test:c0")] {
        assert!(owner.advancements.get_progress(&id).unwrap().is_done());
    }
    let name = owner.display_name();
    let expected = ["test:c0", "test:an"]
        .map(|id_text| announcement(resources.advancements.get(&id(id_text)).unwrap(), &name));
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => Some(*packet),
            _ => None,
        })
        .collect();
    let announcements: Vec<_> = packets
        .iter()
        .filter_map(|packet| match packet {
            cb::Packet::SystemChat(packet) => Some(packet.content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(announcements, expected);
    assert!(matches!(
        packets.as_slice(),
        [cb::Packet::SystemChat(_), cb::Packet::RecipeBookAdd(packet), cb::Packet::SystemChat(_)]
            if !packet.replace && packet.entries.len() == 1
    ));
    flush_dirty(&mut level, player).unwrap();
    let packet = level
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => match *packet {
                cb::Packet::UpdateAdvancements(packet) => Some(packet),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(
        json!(
            packet
                .added
                .iter()
                .map(|value| value.advancement.id.as_str())
                .collect::<Vec<_>>()
        ),
        row["root_order"]
    );
}

#[test]
fn vanilla_live_item_triggers_preserve_receivers_and_pre_damage_values() {
    use crate::{
        entity::player::{ServerPlayer, inventory},
        inventory::{ContainerHandle, ContainerSlot, StackValue},
        level::chunk::LevelChunk,
    };
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    use ferraloom_protocol::component::{ComponentTypeId, DataComponent};
    use ferraloom_util::{BlockPos, ChunkPos};
    use std::sync::Arc;

    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("item-triggers.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 9);
    for row in rows.as_array().unwrap() {
        let stack = ItemStack::from_json(&row["stack"]).unwrap();
        let block = Block::from_name(row["block"].as_str().unwrap()).unwrap();
        let new_damage = row["new_damage"].as_i64().unwrap() as i32;
        let criterion = Criterion::decode(&row["input"]["criteria"]["a"]).unwrap();
        let params = LootParams {
            origin: Some(Vec3::new(0.5, 0.5, 0.5)),
            block_state: Some(block.default_state()),
            tool: Some(stack.clone()),
            ..Default::default()
        };
        let event = match criterion.trigger {
            TriggerType::PlacedBlock => TriggerEvent::Location(&params),
            TriggerType::ItemDurabilityChanged => TriggerEvent::Durability {
                item: &stack,
                new_damage,
            },
            TriggerType::ConsumeItem => TriggerEvent::Item(&stack),
            _ => unreachable!(),
        };
        let mut random = LegacyRandomSource::new(42);
        let data = LootData::default();
        let mut context = MatchContext::borrowed(&data, &Facts, &mut random, Vec3::ZERO);
        assert_eq!(
            criterion.instance.matches(event, &mut context).unwrap(),
            row["done"].as_bool().unwrap(),
            "{}",
            row["input"]
        );

        for detached in [false, true] {
            let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
                id: id("test:item"),
                value: Advancement::decode(&row["input"]).unwrap(),
            }]);
            let (mut level, player) = live_fixture(manager);
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(0, 0),
                    0..1,
                    LevelChunkSection::new(
                        Strategy::block_states(),
                        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                        BiomeId(0),
                    ),
                ))
                .unwrap();
            let pos = BlockPos::new(0, 0, 0);
            level.set_block_state(pos, block.default_state()).unwrap();
            level
                .entity_mut::<ServerPlayer>(player)
                .unwrap()
                .inventory
                .items[0] = stack.clone();
            let address = ContainerSlot {
                container: ContainerHandle::PlayerInventory(player),
                slot: 0,
            };
            let receiver = level.stack_ref(address).unwrap();
            level.retain_stack(receiver).unwrap();
            if detached {
                level
                    .replace_stack_object(address, StackValue::New(ItemStack::default()))
                    .unwrap();
            }
            match criterion.trigger {
                TriggerType::PlacedBlock => {
                    criteria::placed_block(&mut level, player, pos, receiver)
                }
                TriggerType::ItemDurabilityChanged => {
                    criteria::item_durability_changed(&mut level, player, receiver, new_damage)
                }
                TriggerType::ConsumeItem => criteria::consume_item(&mut level, player, receiver),
                _ => unreachable!(),
            }
            .unwrap();
            let owner = level.entity::<ServerPlayer>(player).unwrap();
            assert_eq!(
                owner
                    .advancements
                    .get_progress(&id("test:item"))
                    .unwrap()
                    .is_done(),
                row["done"].as_bool().unwrap(),
                "detached={detached}: {}",
                row["input"]
            );
            assert_eq!(
                owner.total_experience as usize,
                row["reward_reads"].as_array().unwrap().len()
            );
            let after = level.stack(receiver).unwrap();
            assert_eq!(after, &ItemStack::from_json(&row["after_trigger"]).unwrap());
            if let Some(read) = row["reward_reads"].as_array().unwrap().first() {
                let components = after.components().unwrap();
                let damage = match components.get(ComponentTypeId::DAMAGE) {
                    Some(DataComponent::Damage(value)) => value.0,
                    _ => 0,
                };
                assert_eq!(
                    json!({"count":inventory::count(after),"damage":damage}),
                    *read
                );
            }
            level.release_stack(receiver);
            assert!(!level.stack_is_retained(address).unwrap());
            if detached {
                assert!(level.stack(receiver).is_err());
            }
        }
    }
}

#[test]
fn vanilla_sculk_triggers_use_canonical_player_victim_and_damage_source() {
    use crate::entity::{
        EntityData, EntityState, animal::Cow, damage::DamageSource, player::ServerPlayer,
    };

    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("sculk-triggers.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 9);
    for row in rows.as_array().unwrap() {
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: id("test:sculk"),
            value: Advancement::decode(&row["input"]).unwrap(),
        }]);
        let (mut level, player) = live_fixture(manager);
        level
            .entity_mut::<EntityData>(player)
            .unwrap()
            .set_pos(Vec3::new(1.0, 64.0, 2.0));
        let victim = level.next_entity_ref();
        let mut victim_data = EntityData::new(EntityType::COW, 2, 0);
        victim_data.set_pos(Vec3::new(4.0, 64.0, 2.0));
        level
            .add_entity(
                victim,
                EntityState::Cow(Box::new(Cow::with_parent(victim_data))),
            )
            .unwrap();
        let source = if row["indirect"].as_bool().unwrap() {
            let holder = level
                .context()
                .damage_types()
                .get(&"arrow".parse().unwrap())
                .unwrap()
                .clone();
            DamageSource::with_entities(holder, Some(victim), Some(player))
        } else {
            level.context().damage_sources().player_attack(player)
        };
        assert_eq!(source.is_direct(), row["is_direct"].as_bool().unwrap());
        assert_eq!(
            source.to_loot_source().damage_type.as_str(),
            row["damage_type"].as_str().unwrap()
        );
        match row["input"]["criteria"]["a"]["trigger"].as_str().unwrap() {
            "avoid_vibration" => criteria::avoid_vibration(&mut level, player),
            "kill_mob_near_sculk_catalyst" => {
                criteria::kill_mob_near_sculk_catalyst(&mut level, player, victim, &source)
            }
            _ => unreachable!(),
        }
        .unwrap();
        let owner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(
            owner
                .advancements
                .get_progress(&id("test:sculk"))
                .unwrap()
                .is_done(),
            row["done"].as_bool().unwrap(),
            "{}",
            row["input"]
        );
        assert_eq!(
            owner.total_experience,
            row["experience"].as_i64().unwrap() as i32
        );
    }
}

#[test]
fn vanilla_fall_from_height_uses_start_and_captured_current_position() {
    use crate::entity::{EntityData, player::ServerPlayer};

    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("fall-height.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 9);
    for row in rows.as_array().unwrap() {
        let vector = |value: &Value| {
            let values = value.as_array().unwrap();
            Vec3::new(
                values[0].as_f64().unwrap(),
                values[1].as_f64().unwrap(),
                values[2].as_f64().unwrap(),
            )
        };
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: id("test:fall"),
            value: Advancement::decode(&row["input"]).unwrap(),
        }]);
        let (mut level, player) = live_fixture(manager);
        level
            .entity_mut::<EntityData>(player)
            .unwrap()
            .set_pos(vector(&row["end"]));
        criteria::fall_from_height(&mut level, player, vector(&row["start"])).unwrap();
        let owner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(
            owner
                .advancements
                .get_progress(&id("test:fall"))
                .unwrap()
                .is_done(),
            row["done"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(
            owner.total_experience,
            row["experience"].as_i64().unwrap() as i32
        );
    }
}

#[test]
fn vanilla_hurt_triggers_preserve_damage_blocking_and_entity_roles() {
    use crate::entity::{
        EntityData, EntityState, animal::Cow, damage::DamageSource, player::ServerPlayer,
    };

    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("hurt-triggers.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 13);
    for row in rows.as_array().unwrap() {
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: id("test:hurt"),
            value: Advancement::decode(&row["input"]).unwrap(),
        }]);
        let (mut level, player) = live_fixture(manager);
        level
            .entity_mut::<EntityData>(player)
            .unwrap()
            .set_pos(Vec3::new(1.0, 64.0, 2.0));
        let other = level.next_entity_ref();
        let mut other_data = EntityData::new(EntityType::COW, 2, 0);
        other_data.set_pos(Vec3::new(4.0, 64.0, 2.0));
        level
            .add_entity(
                other,
                EntityState::Cow(Box::new(Cow::with_parent(other_data))),
            )
            .unwrap();
        let source = match row["source"].as_str().unwrap() {
            "player" => level.context().damage_sources().player_attack(player),
            "mob" => level.context().damage_sources().mob_attack(other),
            "generic" => level.context().damage_sources().generic(),
            "indirect" => {
                let holder = level
                    .context()
                    .damage_types()
                    .get(&"arrow".parse().unwrap())
                    .unwrap()
                    .clone();
                DamageSource::with_entities(holder, Some(other), Some(player))
            }
            _ => unreachable!(),
        };
        assert_eq!(
            source.to_loot_source().damage_type.as_str(),
            row["damage_type"].as_str().unwrap()
        );
        let original = row["original"].as_f64().unwrap() as f32;
        let damage = row["damage"].as_f64().unwrap() as f32;
        let blocked = row["blocked"].as_bool().unwrap();
        let health = level.entity::<ServerPlayer>(player).unwrap().health();
        match row["input"]["criteria"]["a"]["trigger"].as_str().unwrap() {
            "entity_hurt_player" => {
                criteria::entity_hurt_player(&mut level, player, &source, original, damage, blocked)
            }
            "player_hurt_entity" => criteria::player_hurt_entity(
                &mut level, player, other, &source, original, damage, blocked,
            ),
            _ => unreachable!(),
        }
        .unwrap();
        let owner = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(
            owner
                .advancements
                .get_progress(&id("test:hurt"))
                .unwrap()
                .is_done(),
            row["done"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(
            owner.total_experience,
            row["experience"].as_i64().unwrap() as i32
        );
        assert_eq!(owner.health(), health);
    }
}

#[test]
fn live_fall_from_height_reads_canonical_player_nbt_at_the_trigger_point() {
    use crate::entity::{EntityData, player::ServerPlayer};

    for fall_distance in [5.25, 0.0] {
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: id("test:fall_nbt"),
            value: Advancement::decode(&json!({
                "criteria": {"a": {"trigger": "fall_from_height", "conditions": {
                    "player": {"type": "entity_properties", "entity": "this",
                        "predicate": {"nbt": "{fall_distance:5.25d}"}},
                    "distance": {"y": 10}
                }}}
            }))
            .unwrap(),
        }]);
        let (mut level, player) = live_fixture(manager);
        crate::entity::tests::install_test_environment(&mut level, false, "minecraft:overworld");
        let base = level.entity_mut::<EntityData>(player).unwrap();
        base.set_pos(Vec3::new(1.0, 64.0, 2.0));
        base.fall_distance = fall_distance;
        criteria::fall_from_height(&mut level, player, Vec3::new(1.0, 74.0, 2.0)).unwrap();
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .advancements
                .get_progress(&id("test:fall_nbt"))
                .unwrap()
                .is_done(),
            fall_distance == 5.25
        );
    }
}

#[test]
fn retained_inventory_trigger_and_stats_request_read_canonical_player() {
    use crate::{
        entity::player::{ServerPlayer, inventory},
        inventory::{ContainerHandle, ContainerSlot},
        level::effects::LevelEffect,
        predicates::PredicateContext,
        stats::{Stat, custom},
    };
    use ferraloom_protocol::play::clientbound as cb;
    let manager = ServerAdvancementManager::new(holders(
        &json!([{ "id":"test:inventory", "value": {
            "criteria":{"stone":{"trigger":"inventory_changed","conditions":{"items":[{"items":"stone"}],"slots":{"occupied":1,"full":1,"empty":42}}}}
        }}]),
    ));
    let (mut level, player) = live_fixture(manager);
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .inventory
        .items[0] = ItemStack::new(Item::STONE, 64, Default::default());
    let receiver = level
        .stack_ref(ContainerSlot {
            container: ContainerHandle::PlayerInventory(player),
            slot: 0,
        })
        .unwrap();
    criteria::inventory_changed(&mut level, player, receiver).unwrap();
    assert!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .advancements
            .get_progress(&id("test:inventory"))
            .unwrap()
            .is_done()
    );
    assert!(
        PredicateContext::player_advancement(&level, player, &id("test:inventory"))
            .unwrap()
            .unwrap()
            .done
    );
    let owner = level.entity_mut::<ServerPlayer>(player).unwrap();
    inventory::set_count(&mut owner.inventory.items[0], 1);
    owner
        .stats
        .increment(Stat::Custom(custom::Id::OPEN_CHEST), 2);
    assert_eq!(
        PredicateContext::player_stat(&level, player, &id("custom"), &id("open_chest")).unwrap(),
        2
    );
    crate::stats::request_stats(&mut level, player).unwrap();
    crate::stats::request_stats(&mut level, player).unwrap();
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => match *packet {
                cb::Packet::AwardStats(packet) => Some(packet),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(packets[0].stats.len(), 1);
    assert_eq!(packets[0].stats[0].count.0, 2);
    assert!(packets[1].stats.is_empty());
}

#[test]
fn announcement_descriptor_reads_rule_and_team_at_application_time() {
    use crate::{entity::player::ServerPlayer, level::effects::LevelEffect};
    use ferraloom_protocol::{chat::Component, play::clientbound as cb};
    let manager =
        ServerAdvancementManager::new(holders(&oracle()["nested_visibility"][0]["inputs"]));
    let (mut level, player) = live_fixture(manager);
    let resources = level.container_resources().unwrap();
    let result = level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .advancements
        .award(
            &resources.advancements,
            &id("test:an"),
            "a",
            ObtainedTime::from_millis(1),
        );
    let descriptor = result
        .effects
        .into_iter()
        .find(|effect| matches!(effect, AdvancementEffect::AnnounceAdvancement(_)))
        .unwrap();
    level.rules_mut().show_advancement_messages = false;
    super::live::apply_effect(
        &mut level,
        player,
        &resources.advancements,
        descriptor.clone(),
    )
    .unwrap();
    assert!(level.take_effects().is_empty());
    let scoreboard = &mut level.shared_mut().unwrap().scoreboard;
    scoreboard.add_player_team("after_rewards");
    scoreboard
        .update_team("after_rewards", |team| {
            team.prefix = Component::literal("after ")
        })
        .unwrap();
    scoreboard
        .add_player_to_team("Live", "after_rewards")
        .unwrap();
    level.rules_mut().show_advancement_messages = true;
    let expected = announcement(
        resources.advancements.get(&id("test:an")).unwrap(),
        &level.entity_display_name(player).unwrap(),
    );
    super::live::apply_effect(&mut level, player, &resources.advancements, descriptor).unwrap();
    let content = level
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => match *packet {
                cb::Packet::SystemChat(chat) => Some(chat.content),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(content, expected);
    assert!(content.to_json().unwrap().contains("after "));
}

#[test]
fn vanilla_rewards_change_announcement_rule_and_name_before_chat() {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("announcement.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let cases: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 4);
    for row in cases.as_array().unwrap() {
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: id("test:announcement"),
            value: Advancement::decode(&row["input"]).unwrap(),
        }]);
        let mut player = PlayerAdvancements::new(&manager);
        let effects = player
            .award(
                &manager,
                &id("test:announcement"),
                "a",
                ObtainedTime::from_millis(1),
            )
            .effects;
        let mut rule = row["before"].as_bool().unwrap();
        let mut name = ferraloom_protocol::chat::Component::literal("Before");
        let mut chats = Vec::new();
        for effect in effects {
            match effect {
                AdvancementEffect::Rewards { .. } => {
                    rule = row["after"].as_bool().unwrap();
                    name = ferraloom_protocol::chat::Component::literal("After");
                }
                AdvancementEffect::AnnounceAdvancement(id) => {
                    if let Some(component) = resolve_announcement(
                        &manager,
                        &id,
                        AwardContext {
                            now: ObtainedTime::from_millis(1),
                            player_name: &name,
                            announce_advancements: rule,
                        },
                    ) {
                        chats.push(
                            serde_json::from_str::<Value>(&component.to_json().unwrap()).unwrap(),
                        );
                    }
                }
                AdvancementEffect::MarkVisibility(id) => {
                    player.apply_mark_visibility(&manager, &id)
                }
            }
        }
        assert_eq!(
            json!(chats),
            row["chats"],
            "before={} after={}",
            row["before"],
            row["after"]
        );
    }
}

#[test]
fn vanilla_recipe_crafted_retained_receivers_preserve_order_and_duplicates() {
    use crate::{
        crafting::RecipeHolder,
        entity::player::ServerPlayer,
        inventory::{ContainerHandle, ContainerSlot},
    };
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("recipe-crafted.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 6);
    for row in rows.as_array().unwrap() {
        let objects: Vec<_> = row["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|object| ItemStack::from_json(object).unwrap())
            .collect();
        let used: Vec<_> = row["used"]
            .as_array()
            .unwrap()
            .iter()
            .map(|index| objects[index.as_u64().unwrap() as usize].clone())
            .collect();
        let recipe_id = id(row["recipe"].as_str().unwrap());
        let data = LootData::default();
        let mut random = LegacyRandomSource::new(42);
        let mut context = MatchContext::borrowed(&data, &Facts, &mut random, Vec3::ZERO);
        assert_eq!(
            Criterion::decode(&row["input"])
                .unwrap()
                .instance
                .matches(
                    TriggerEvent::Recipe {
                        id: &recipe_id,
                        ingredients: &used,
                    },
                    &mut context,
                )
                .unwrap(),
            row["matches"].as_bool().unwrap(),
            "pure {row}"
        );
        let manager = ServerAdvancementManager::new(holders(
            &json!([{"id":"test:crafted","value":{"criteria":{"used":row["input"]}}}]),
        ));
        let (mut level, player) = live_fixture(manager);
        for (slot, object) in row["objects"].as_array().unwrap().iter().enumerate() {
            level
                .entity_mut::<ServerPlayer>(player)
                .unwrap()
                .inventory
                .items[slot] = ItemStack::from_json(object).unwrap();
        }
        let inputs: Vec<_> = row["used"]
            .as_array()
            .unwrap()
            .iter()
            .map(|index| {
                level
                    .stack_ref(ContainerSlot {
                        container: ContainerHandle::PlayerInventory(player),
                        slot: index.as_u64().unwrap() as usize,
                    })
                    .unwrap()
            })
            .collect();
        let resources = level.container_resources().unwrap();
        let recipe = RecipeHolder {
            id: row["recipe"].as_str().unwrap().parse().unwrap(),
            value: resources.recipes.recipes()[0].value.clone(),
        };
        criteria::recipe_crafted(&mut level, player, &recipe, &inputs).unwrap();
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .advancements
                .get_progress(&id("test:crafted"))
                .unwrap()
                .is_done(),
            row["matches"].as_bool().unwrap(),
            "{row}"
        );
        for input in inputs {
            assert!(!level.stack_is_retained(input.address()).unwrap());
        }
    }
}

#[test]
fn recipe_crafted_families_use_distinct_listeners_and_shared_retained_inputs() {
    use crate::{
        entity::player::ServerPlayer,
        inventory::{ContainerHandle, ContainerSlot},
    };
    let rows: Vec<_> = [("ordinary", "recipe_crafted"), ("crafter", "crafter_recipe_crafted")]
        .into_iter()
        .map(|(name, trigger)| json!({"id":format!("test:{name}"),"value":{"criteria":{"used":{
            "trigger":trigger,"conditions":{"recipes":"crafting_table","ingredients":[{"items":"stone"},{"items":"stone"}]}
        }}}}))
        .collect();
    let manager = ServerAdvancementManager::new(holders(&json!(rows)));
    let (mut level, player) = live_fixture(manager);
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .inventory
        .items[0] = ItemStack::new(Item::STONE, 1, Default::default());
    let address = ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot: 0,
    };
    let input = level.stack_ref(address).unwrap();
    let resources = level.container_resources().unwrap();
    let recipe = &resources.recipes.recipes()[0];
    let done = |level: &crate::level::Level| {
        ["test:ordinary", "test:crafter"].map(|name| {
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .advancements
                .get_progress(&id(name))
                .unwrap()
                .is_done()
        })
    };
    criteria::crafter_recipe_crafted(&mut level, player, recipe, &[input, input]).unwrap();
    assert_eq!(done(&level), [false, true]);
    assert!(!level.stack_is_retained(address).unwrap());
    criteria::recipe_crafted(&mut level, player, recipe, &[input, input]).unwrap();
    assert_eq!(done(&level), [true, true]);
    assert!(!level.stack_is_retained(address).unwrap());
}

#[test]
fn recipe_crafted_releases_roots_on_capture_and_predicate_errors() {
    use crate::{
        entity::player::ServerPlayer,
        inventory::{ContainerHandle, ContainerSlot, StackRef},
    };
    let manager = ServerAdvancementManager::new(holders(
        &json!([{"id":"test:crafted","value":{"criteria":{"used":{
            "trigger":"recipe_crafted","conditions":{"recipes":"crafting_table","ingredients":[{"items":"stone"}],
            "player":{"type":"entity_properties","entity":"this","predicate":{"minecraft:type_specific/player":{"food":{"level":20}}}}}
        }}}}]),
    ));
    let (mut level, player) = live_fixture(manager);
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .inventory
        .items[0] = ItemStack::new(Item::STONE, 1, Default::default());
    let address = ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot: 0,
    };
    let input = level.stack_ref(address).unwrap();
    let invalid = StackRef {
        generation: input.generation + 1,
        ..input
    };
    let resources = level.container_resources().unwrap();
    let recipe = &resources.recipes.recipes()[0];
    level.retain_stack(input).unwrap();
    assert!(criteria::recipe_crafted(&mut level, player, recipe, &[input, invalid]).is_err());
    assert!(level.stack_is_retained(address).unwrap());
    level.release_stack(input);
    assert!(!level.stack_is_retained(address).unwrap());
    assert!(matches!(
        criteria::recipe_crafted(&mut level, player, recipe, &[input]),
        Err(crate::level::LevelError::UnsupportedAdvancementPredicate {
            trigger: criteria::Trigger::Native(TriggerType::RecipeCrafted),
            ..
        })
    ));
    assert!(!level.stack_is_retained(address).unwrap());
    assert!(
        !level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .advancements
            .get_progress(&id("test:crafted"))
            .unwrap()
            .is_done()
    );
}

#[test]
fn award_recipes_counts_displays_after_synchronous_nested_unlocks() {
    use crate::{
        block::containers::runtime::ContainerResources,
        crafting::{Recipe, RecipeHolder, RecipeManager, RecipeSynchronization},
        entity::player::ServerPlayer,
        level::effects::LevelEffect,
    };
    use ferraloom_protocol::play::clientbound as cb;
    use std::sync::Arc;

    let manager = ServerAdvancementManager::new(holders(&json!([
        {"id":"test:outer","value":{
            "criteria":{"unlocked":{"trigger":"recipe_unlocked","conditions":{"recipes":"test:multi"}}},
            "rewards":{"recipes":["crafting_table"]}
        }},
        {"id":"test:nested","value":{
            "criteria":{"unlocked":{"trigger":"recipe_unlocked","conditions":{"recipes":"crafting_table"}}}
        }}
    ])));
    let (mut level, player) = live_fixture(manager);
    let previous = level.container_resources().unwrap();
    let tags = ferraloom_data::datapack::TagSet::<Item>::load(
        &ferraloom_data::datapack::ResourceManager::new(Vec::new()),
    )
    .unwrap();
    let recipes = Arc::new(
        RecipeManager::new(vec![
            RecipeHolder {
                id: "test:multi".parse().unwrap(),
                value: Recipe::decode(
                    &json!({"type":"crafting_transmute","input":"stone","material":"dirt","material_count":{"min":1,"max":3},"result":"diamond","show_notification":false}),
                    &tags,
                ).unwrap(),
            },
            previous.recipes.recipes()[0].clone(),
            RecipeHolder {
                id: "test:special".parse().unwrap(),
                value: Recipe::decode(&json!({"type":"crafting_special_repairitem"}), &tags).unwrap(),
            },
        ]).unwrap(),
    );
    let sync = Arc::new(RecipeSynchronization::new(&recipes, |_| true));
    level.set_container_resources(
        Arc::new(ContainerResources {
            crafting: Arc::clone(&previous.crafting),
            recipes: Arc::clone(&recipes),
            loot: Arc::clone(&previous.loot),
            advancements: Arc::clone(&previous.advancements),
            recipe_tags: previous.recipe_tags.clone(),
            recipe_synchronization: Arc::clone(&sync),
        }),
        0,
    );
    let ordered = [
        &recipes.recipes()[0],
        &recipes.recipes()[1],
        &recipes.recipes()[2],
        &recipes.recipes()[0],
    ];
    // The nested award unlocks crafting_table before the outer loop reaches it.
    assert_eq!(award_recipes(&mut level, player, ordered).unwrap(), 3);
    let owner = level.entity::<ServerPlayer>(player).unwrap();
    assert_eq!(
        owner.recipe_book.known(),
        &[
            recipes.recipes()[0].id.clone(),
            recipes.recipes()[1].id.clone()
        ]
    );
    for advancement in ["test:outer", "test:nested"] {
        assert!(
            owner
                .advancements
                .get_progress(&id(advancement))
                .unwrap()
                .is_done()
        );
    }
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, .. } => Some(*packet),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 2);
    for (packet, recipe) in packets
        .iter()
        .zip([&recipes.recipes()[1], &recipes.recipes()[0]])
    {
        let cb::Packet::RecipeBookAdd(packet) = packet else {
            panic!("expected recipe book packet")
        };
        assert!(!packet.replace);
        let expected: Vec<_> = sync
            .displays_for_recipe(&recipe.id)
            .map(|entry| entry.id)
            .collect();
        assert_eq!(
            packet
                .entries
                .iter()
                .map(|entry| entry.contents.id)
                .collect::<Vec<_>>(),
            expected
        );
    }
    assert_eq!(award_recipes(&mut level, player, ordered).unwrap(), 0);
    assert!(level.take_effects().is_empty());
}
