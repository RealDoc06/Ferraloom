use super::{
    Advancement, Identifier, Result,
    triggers::{ContextAwarePredicate, TriggerConditions, TriggerType},
};
use crate::{
    crafting::RecipeManager,
    loot::{
        ContextKeySet, Holder, HolderSet, LootData, LootError, LootItemCondition, RegistryBindable,
        RegistryHolder, RegistryLookup, ValidationProblem,
    },
    predicates::{EntityPredicate, EntitySubPredicate, ItemPredicate},
};
use ferraloom_protocol::component::{ComponentPredicate, DataComponentMatchers, LockCode};
use std::collections::BTreeMap;

/// Registry snapshot for advancement loading. Level state is never consulted.
pub struct AdvancementRegistryContext<'a> {
    pub loot: &'a LootData,
    pub recipes: &'a RecipeManager,
    pub recipe_tags: &'a BTreeMap<Identifier, Vec<Identifier>>,
    pub lookup: Option<&'a dyn RegistryLookup>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancementValidationProblem {
    pub advancement: Identifier,
    pub criterion: String,
    pub problem: ValidationProblem,
}
fn predicate<'a>(
    value: &'a ContextAwarePredicate,
    context: &'a AdvancementRegistryContext<'_>,
) -> Result<&'a LootItemCondition> {
    match value {
        Holder::Inline(value) => Ok(value),
        Holder::Reference(id) => {
            context
                .loot
                .predicates
                .get(id)
                .ok_or_else(|| LootError::MissingReference {
                    registry: "predicate",
                    id: id.clone(),
                })
        }
    }
}
fn validate_set<T>(
    set: &HolderSet<T>,
    registry: &'static str,
    tag: impl Fn(&Identifier) -> bool,
    value: impl Fn(&Identifier) -> bool,
) -> Result<()> {
    match set {
        HolderSet::Tag(id) if !tag(id) => {
            return Err(LootError::MissingReference {
                registry,
                id: id.clone(),
            });
        }
        HolderSet::Direct(values) => {
            for entry in values {
                if let Holder::Reference(id) = entry {
                    if !value(id) {
                        return Err(LootError::MissingReference {
                            registry,
                            id: id.clone(),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn partial_item(predicate: ComponentPredicate) -> ItemPredicate {
    ItemPredicate(LockCode {
        items: None,
        count: Default::default(),
        components: DataComponentMatchers {
            exact: Vec::new(),
            partial: ferraloom_protocol::component::BoundedList(vec![predicate]),
        },
    })
}
impl Advancement {
    /// Port of Advancement#validate and RegistryOps binding. Codec-invalid
    /// references fail publication; context validation problems remain warnings.
    pub fn bind_and_validate(
        &self,
        id: &Identifier,
        context: &AdvancementRegistryContext<'_>,
    ) -> Result<(Vec<RegistryHolder>, Vec<AdvancementValidationProblem>)> {
        let mut bindings = Vec::new();
        let mut problems = Vec::new();
        if let Some(display) = &self.display {
            let test = ItemPredicate(LockCode {
                items: None,
                count: Default::default(),
                components: DataComponentMatchers {
                    exact: display.icon.components.added.clone(),
                    partial: ferraloom_protocol::component::BoundedList(Vec::new()),
                },
            });
            bindings.extend(test.bind_registries(context.lookup)?);
        }
        validate_set(
            &self.rewards.loot,
            "loot_table",
            |tag| context.loot.tags.contains_key(&("loot_table", tag.clone())),
            |key| context.loot.tables.contains_key(key),
        )?;
        if let HolderSet::Direct(tables) = &self.rewards.loot {
            for table in tables {
                if let Holder::Inline(table) = table {
                    bindings.extend(table.bind_registries(context.lookup)?);
                }
            }
        }
        for (criterion_name, criterion) in &self.criteria {
            let mut conditions: Vec<(String, &ContextAwarePredicate, &str)> = Vec::new();
            let mut bindable: Vec<&dyn RegistryBindable> = Vec::new();
            if let Some(player) = &criterion.instance.player {
                conditions.push(("player".into(), player, "advancement_entity"));
            }
            fn add<'a>(
                conditions: &mut Vec<(String, &'a ContextAwarePredicate, &'a str)>,
                name: &'a str,
                predicate: &'a Option<ContextAwarePredicate>,
            ) {
                if let Some(predicate) = predicate {
                    conditions.push((name.into(), predicate, "advancement_entity"));
                }
            }
            fn item<'a>(
                values: &mut Vec<&'a dyn RegistryBindable>,
                predicate: &'a Option<ItemPredicate>,
            ) {
                if let Some(predicate) = predicate {
                    values.push(predicate);
                }
            }
            use TriggerConditions as C;
            match &criterion.instance.conditions {
                C::Impossible
                | C::Player
                | C::Beacon(_)
                | C::EnderEye(_)
                | C::SpearMobs(_)
                | C::Levitation { .. }
                | C::Dimension { .. } => {}
                C::Item(value) => item(&mut bindable, value),
                C::Entity(value) => add(&mut conditions, "entity", value),
                C::Block(value) => bindable.push(value),
                C::Location(value) => {
                    if let Some(value) = value {
                        conditions.push((
                            "location".into(),
                            value,
                            if criterion.trigger == TriggerType::DefaultBlockUse {
                                "block_use"
                            } else {
                                "advancement_location"
                            },
                        ));
                    }
                }
                C::Killed {
                    entity,
                    killing_blow,
                } => {
                    add(&mut conditions, "entity", entity);
                    if let Some(value) = killing_blow {
                        bindable.push(value);
                    }
                }
                C::Inventory { items, .. } => {
                    for value in items {
                        bindable.push(value);
                    }
                }
                C::Recipe {
                    recipes,
                    ingredients,
                } => {
                    validate_set(
                        recipes,
                        "recipe",
                        |tag| context.recipe_tags.contains_key(tag),
                        |id| {
                            context
                                .recipes
                                .by_key(&id.as_str().parse().expect("validated id"))
                                .is_some()
                        },
                    )?;
                    for value in ingredients {
                        bindable.push(value);
                    }
                }
                C::Hurt { damage, entity } => {
                    if let Some(value) = damage {
                        bindable.push(value);
                    }
                    add(&mut conditions, "entity", entity);
                }
                C::EnchantedItem { item: value, .. } | C::Durability { item: value, .. } => {
                    item(&mut bindable, value)
                }
                C::Potion(value) => {
                    if let Some(value) = value {
                        bindings.extend(
                            partial_item(ComponentPredicate::Potions(value.clone()))
                                .bind_registries(context.lookup)?,
                        );
                    }
                }
                C::Bred {
                    parent,
                    partner,
                    child,
                } => {
                    add(&mut conditions, "parent", parent);
                    add(&mut conditions, "partner", partner);
                    add(&mut conditions, "child", child);
                }
                C::Cured { zombie, villager } => {
                    add(&mut conditions, "zombie", zombie);
                    add(&mut conditions, "villager", villager);
                }
                C::Trade {
                    villager,
                    item: value,
                } => {
                    add(&mut conditions, "villager", villager);
                    item(&mut bindable, value);
                }
                C::Effects { effects, source } => {
                    if let Some(value) = effects {
                        bindings.extend(
                            EntityPredicate {
                                parts: vec![EntitySubPredicate::Effects(value.clone())],
                            }
                            .bind_registries(context.lookup)?,
                        );
                    }
                    add(&mut conditions, "source", source);
                }
                C::Distance {
                    start_position,
                    cause,
                    ..
                } => {
                    if let Some(value) = start_position {
                        bindable.push(value);
                    }
                    add(&mut conditions, "cause", cause);
                }
                C::Fishing {
                    rod,
                    entity,
                    item: value,
                } => {
                    item(&mut bindable, rod);
                    item(&mut bindable, value);
                    add(&mut conditions, "entity", entity);
                }
                C::ChanneledLightning(victims) | C::KilledByArrow { victims, .. } => {
                    for (index, value) in victims.iter().enumerate() {
                        conditions.push((format!("victims[{index}]"), value, "advancement_entity"));
                    }
                    if let C::KilledByArrow {
                        fired_from_weapon, ..
                    } = &criterion.instance.conditions
                    {
                        item(&mut bindable, fired_from_weapon);
                    }
                }
                C::BeeNest {
                    block, item: value, ..
                } => {
                    bindable.push(block);
                    item(&mut bindable, value);
                }
                C::Target { projectile, .. } => add(&mut conditions, "projectile", projectile),
                C::Loot(tables) => {
                    validate_set(
                        tables,
                        "loot_table",
                        |tag| context.loot.tags.contains_key(&("loot_table", tag.clone())),
                        |id| context.loot.tables.contains_key(id),
                    )?;
                    if let HolderSet::Direct(tables) = tables {
                        for value in tables {
                            if let Holder::Inline(value) = value {
                                bindable.push(value.as_ref());
                            }
                        }
                    }
                }
                C::ItemEntity {
                    item: value,
                    entity,
                } => {
                    item(&mut bindable, value);
                    add(&mut conditions, "entity", entity);
                }
                C::Lightning {
                    lightning,
                    bystander,
                } => {
                    add(&mut conditions, "lightning", lightning);
                    add(&mut conditions, "bystander", bystander);
                }
            }
            for value in bindable {
                bindings.extend(value.bind_registries(context.lookup)?);
            }
            for (name, value, param_set) in conditions {
                bindings.extend(predicate(value, context)?.bind_registries(context.lookup)?);
                let params = ContextKeySet::vanilla(&Identifier::parse(param_set)?)
                    .expect("vanilla parameter set");
                for mut problem in context.loot.validate_condition(value, &params) {
                    problem.path = format!(
                        "{criterion_name}.{name}{}",
                        problem
                            .path
                            .strip_prefix(".condition")
                            .unwrap_or(&problem.path)
                    );
                    problems.push(AdvancementValidationProblem {
                        advancement: id.clone(),
                        criterion: criterion_name.clone(),
                        problem,
                    });
                }
            }
        }
        bindings.sort();
        bindings.dedup();
        Ok((bindings, problems))
    }
}
