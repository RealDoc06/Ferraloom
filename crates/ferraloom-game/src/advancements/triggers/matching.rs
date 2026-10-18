use super::{
    ContextAwarePredicate, InventorySlots, TriggerConditions, TriggerFacts, TriggerInstance,
};
use crate::{
    entity::EntityRef,
    inventory::StackRef,
    loot::{
        DamageSource, Holder, HolderSet, Identifier, LootData, LootError, LootParams, LootRuntime,
        Result,
    },
    predicates::{EntityPredicate, EntitySubPredicate, ItemPredicate, match_components},
};
use ferraloom_data::BlockState;
use ferraloom_protocol::component::{
    ComponentPredicate, ComponentTypeId, DataComponent, DataComponentMap, DataComponentMatchers,
    ItemStack, PotionContents,
};
use ferraloom_util::{Vec3, random::RandomSource};

/// Captured arguments to vanilla TriggerInstance#matches. Caller identity remains
/// in TriggerType so related triggers can share a typed argument shape.
#[derive(Clone, Copy, Debug)]
pub enum TriggerEvent<'a> {
    Player,
    Item(&'a ItemStack),
    Entity(&'a LootParams),
    Block(BlockState),
    Location(&'a LootParams),
    Killed {
        entity: &'a LootParams,
        source: &'a DamageSource,
    },
    Inventory {
        inventory: &'a [ItemStack],
        changed: &'a ItemStack,
        full: i32,
        empty: i32,
        occupied: i32,
    },
    InventoryLive {
        player: EntityRef,
        changed: StackRef,
        full: i32,
        empty: i32,
        occupied: i32,
    },
    Recipe {
        id: &'a Identifier,
        ingredients: &'a [ItemStack],
    },
    RecipeLive {
        id: &'a Identifier,
        ingredients: &'a [StackRef],
    },
    Hurt {
        entity: Option<&'a LootParams>,
        source: &'a DamageSource,
        original: f32,
        actual: f32,
        blocked: bool,
    },
    EnchantedItem {
        item: &'a ItemStack,
        levels: i32,
    },
    Potion(&'a PotionContents),
    Beacon(i32),
    EnderEye(f64),
    Bred {
        parent: &'a LootParams,
        partner: &'a LootParams,
        child: Option<&'a LootParams>,
    },
    Cured {
        zombie: &'a LootParams,
        villager: &'a LootParams,
    },
    Trade {
        villager: &'a LootParams,
        item: &'a ItemStack,
    },
    Durability {
        item: &'a ItemStack,
        new_damage: i32,
    },
    Levitation {
        start: Vec3,
        duration: i32,
    },
    Dimension {
        from: &'a Identifier,
        to: &'a Identifier,
    },
    Effects {
        player: EntityRef,
        source: Option<&'a LootParams>,
    },
    Distance {
        start: Vec3,
        end: Vec3,
        cause: Option<&'a LootParams>,
    },
    Fishing {
        rod: &'a ItemStack,
        hooked: &'a LootParams,
        items: &'a [ItemStack],
    },
    ChanneledLightning(&'a [LootParams]),
    SpearMobs(i32),
    KilledByArrow {
        victims: &'a [LootParams],
        unique_entity_types: i32,
        fired_from_weapon: Option<&'a ItemStack>,
    },
    BeeNest {
        state: BlockState,
        item: &'a ItemStack,
        bees: i32,
    },
    Target {
        signal_strength: i32,
        projectile: &'a LootParams,
    },
    Loot(&'a Identifier),
    ItemEntity {
        item: &'a ItemStack,
        entity: &'a LootParams,
    },
    Lightning {
        lightning: &'a LootParams,
        bystanders: &'a [LootParams],
    },
}
/// A single evaluation uses the caller's RNG and canonical facts, shared by all
/// listeners exactly as SimpleCriterionTrigger does. No world state is invented.
/// Live adapters expose canonical facts through the same loot runtime used by
/// conditions. No world borrow survives an RNG operation or an award callback.
pub trait TriggerRuntime: LootRuntime {
    fn trigger_facts(&self) -> &dyn TriggerFacts;
    fn stack(&self, _stack: StackRef) -> Result<&ItemStack> {
        Err(LootError::Unsupported("advancement.retained_stack".into()))
    }
    fn inventory_item(&self, _player: EntityRef, _slot: usize) -> Result<&ItemStack> {
        Err(LootError::Unsupported(
            "advancement.player_inventory".into(),
        ))
    }
}
enum MatchAccess<'a> {
    Borrowed {
        facts: &'a dyn TriggerFacts,
        random: &'a mut dyn RandomSource,
    },
    Runtime(&'a mut dyn TriggerRuntime),
}
pub struct MatchContext<'a> {
    pub data: &'a LootData,
    pub origin: Vec3,
    access: MatchAccess<'a>,
}
impl<'a> MatchContext<'a> {
    pub fn borrowed(
        data: &'a LootData,
        facts: &'a dyn TriggerFacts,
        random: &'a mut dyn RandomSource,
        origin: Vec3,
    ) -> Self {
        Self {
            data,
            origin,
            access: MatchAccess::Borrowed { facts, random },
        }
    }
    pub fn live(data: &'a LootData, runtime: &'a mut dyn TriggerRuntime, origin: Vec3) -> Self {
        Self {
            data,
            origin,
            access: MatchAccess::Runtime(runtime),
        }
    }
    pub fn facts(&self) -> &dyn TriggerFacts {
        match &self.access {
            MatchAccess::Borrowed { facts, .. } => *facts,
            MatchAccess::Runtime(runtime) => runtime.trigger_facts(),
        }
    }
    pub fn predicate(
        &mut self,
        predicate: &ContextAwarePredicate,
        params: &LootParams,
    ) -> Result<bool> {
        match &mut self.access {
            MatchAccess::Borrowed { facts, random } => {
                self.data.test_predicate(predicate, params, *random, *facts)
            }
            MatchAccess::Runtime(runtime) => self
                .data
                .test_predicate_with_runtime(predicate, params, *runtime),
        }
    }
    fn optional(
        &mut self,
        predicate: &Option<ContextAwarePredicate>,
        params: Option<&LootParams>,
    ) -> Result<bool> {
        match predicate {
            None => Ok(true),
            Some(test) => match params {
                None => Ok(false),
                Some(params) => self.predicate(test, params),
            },
        }
    }
    fn any(&mut self, predicate: &ContextAwarePredicate, params: &[LootParams]) -> Result<bool> {
        for params in params {
            if self.predicate(predicate, params)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn item_matches(
    test: &Option<ItemPredicate>,
    item: &ItemStack,
    context: &MatchContext<'_>,
) -> Result<bool> {
    test.as_ref()
        .map_or(Ok(true), |test| test.matches(item, context.facts()))
}
fn holder_contains<T>(
    set: &HolderSet<T>,
    id: &Identifier,
    tag: impl FnOnce(&Identifier) -> Result<Vec<Identifier>>,
) -> Result<bool> {
    Ok(match set {
        HolderSet::Tag(key) => tag(key)?.contains(id),
        HolderSet::Direct(entries) => entries
            .iter()
            .any(|entry| matches!(entry,Holder::Reference(key) if key==id)),
    })
}
impl TriggerInstance {
    /// Port of the trigger-specific matches methods. Player predicates run only
    /// after this succeeds, in PlayerAdvancements#trigger.
    pub fn matches(&self, event: TriggerEvent<'_>, context: &mut MatchContext<'_>) -> Result<bool> {
        use TriggerConditions as C;
        use TriggerEvent as E;
        Ok(match (&self.conditions, event) {
            (C::Impossible, _) => false,
            (C::Player, E::Player) => true,
            (C::Item(test), E::Item(item)) => item_matches(test, item, context)?,
            (C::Entity(test), E::Entity(params)) => context.optional(test, Some(params))?,
            (C::Location(test), E::Location(params)) => context.optional(test, Some(params))?,
            (C::Block(test), E::Block(state)) => test.matches_state(state, context.facts())?,
            (
                C::Killed {
                    entity,
                    killing_blow,
                },
                E::Killed {
                    entity: params,
                    source,
                },
            ) => {
                let damage = killing_blow.as_ref().map_or(Ok(true), |test| {
                    test.matches(source, context.origin, context.facts())
                })?;
                damage && context.optional(entity, Some(params))?
            }
            (
                C::Inventory { slots, items },
                E::Inventory {
                    inventory,
                    changed,
                    full,
                    empty,
                    occupied,
                },
            ) => {
                slots.matches(full, empty, occupied)
                    && inventory_matches(items, changed, inventory.iter().map(Ok), context.facts())?
            }
            (
                C::Inventory { slots, items },
                E::InventoryLive {
                    player,
                    changed,
                    full,
                    empty,
                    occupied,
                },
            ) => {
                if !slots.matches(full, empty, occupied) {
                    return Ok(false);
                }
                let MatchAccess::Runtime(runtime) = &context.access else {
                    return Err(LootError::Unsupported("advancement.retained_stack".into()));
                };
                inventory_matches(
                    items,
                    runtime.stack(changed)?,
                    (0..43).map(|slot| runtime.inventory_item(player, slot)),
                    runtime.trigger_facts(),
                )?
            }
            (
                C::Recipe {
                    recipes,
                    ingredients,
                },
                E::Recipe {
                    id,
                    ingredients: used,
                },
            ) => recipe_matches(
                recipes,
                ingredients,
                id,
                used.iter(),
                |stack| Ok(*stack),
                context.facts(),
            )?,
            (
                C::Recipe {
                    recipes,
                    ingredients,
                },
                E::RecipeLive {
                    id,
                    ingredients: used,
                },
            ) => {
                let MatchAccess::Runtime(runtime) = &context.access else {
                    return Err(LootError::Unsupported("advancement.retained_stack".into()));
                };
                recipe_matches(
                    recipes,
                    ingredients,
                    id,
                    used.iter().copied(),
                    |stack| runtime.stack(*stack),
                    runtime.trigger_facts(),
                )?
            }
            (
                C::Hurt { damage, entity },
                E::Hurt {
                    entity: params,
                    source,
                    original,
                    actual,
                    blocked,
                },
            ) => {
                let damage = damage.as_ref().map_or(Ok(true), |test| {
                    test.matches(
                        context.origin,
                        source,
                        original,
                        actual,
                        blocked,
                        context.facts(),
                    )
                })?;
                damage && context.optional(entity, params)?
            }
            (
                C::EnchantedItem { item, levels },
                E::EnchantedItem {
                    item: stack,
                    levels: count,
                },
            ) => item_matches(item, stack, context)? && levels.matches(count),
            (C::Potion(test), E::Potion(potion)) => {
                if let Some(test) = test {
                    let components = DataComponentMap {
                        entries: vec![DataComponent::PotionContents(potion.clone())],
                    };
                    let matcher = DataComponentMatchers {
                        exact: Vec::new(),
                        partial: ferraloom_protocol::component::BoundedList(vec![
                            ComponentPredicate::Potions(test.clone()),
                        ]),
                    };
                    match_components(&matcher, &components, context.facts())?
                } else {
                    true
                }
            }
            (C::Beacon(level), E::Beacon(value)) => level.matches(value),
            (C::EnderEye(distance), E::EnderEye(squared)) => distance.matches_sqr(squared),
            (
                C::Bred {
                    parent,
                    partner,
                    child,
                },
                E::Bred {
                    parent: p,
                    partner: q,
                    child: c,
                },
            ) => {
                context.optional(child, c)?
                    && (context.optional(parent, Some(p))? && context.optional(partner, Some(q))?
                        || context.optional(parent, Some(q))?
                            && context.optional(partner, Some(p))?)
            }
            (
                C::Cured { zombie, villager },
                E::Cured {
                    zombie: z,
                    villager: v,
                },
            ) => context.optional(zombie, Some(z))? && context.optional(villager, Some(v))?,
            (
                C::Trade { villager, item },
                E::Trade {
                    villager: v,
                    item: stack,
                },
            ) => context.optional(villager, Some(v))? && item_matches(item, stack, context)?,
            (
                C::Durability {
                    item,
                    durability,
                    delta,
                },
                E::Durability {
                    item: stack,
                    new_damage,
                },
            ) => {
                if !item_matches(item, stack, context)? {
                    return Ok(false);
                }
                let components = stack.components()?;
                let max = match components.get(ComponentTypeId::MAX_DAMAGE) {
                    Some(DataComponent::MaxDamage(v)) => v.0,
                    _ => 0,
                };
                let old = match components.get(ComponentTypeId::DAMAGE) {
                    Some(DataComponent::Damage(v)) => v.0,
                    _ => 0,
                };
                durability.matches(max.wrapping_sub(new_damage))
                    && delta.matches(old.wrapping_sub(new_damage))
            }
            (C::Levitation { distance, duration }, E::Levitation { start, duration: t }) => {
                distance.is_none_or(|test| test.matches(start, context.origin))
                    && duration.matches(t)
            }
            (C::Dimension { from, to }, E::Dimension { from: f, to: t }) => {
                from.as_ref().is_none_or(|v| v == f) && to.as_ref().is_none_or(|v| v == t)
            }
            (
                C::Effects { effects, source },
                E::Effects {
                    player,
                    source: params,
                },
            ) => {
                if let Some(effects) = effects {
                    let test = EntityPredicate {
                        parts: vec![EntitySubPredicate::Effects(effects.clone())],
                    };
                    if !test.matches(Some(player), Some(context.origin), context.facts())? {
                        return Ok(false);
                    }
                }
                context.optional(source, params)?
            }
            (
                C::Distance {
                    start_position,
                    distance,
                    cause,
                },
                E::Distance {
                    start,
                    end,
                    cause: params,
                },
            ) => {
                if let Some(test) = start_position
                    && !test.matches(start, context.facts())?
                {
                    return Ok(false);
                }
                distance.is_none_or(|test| test.matches(start, end))
                    && context.optional(cause, params)?
            }
            (
                C::Fishing { rod, entity, item },
                E::Fishing {
                    rod: stack,
                    hooked,
                    items,
                },
            ) => {
                if !item_matches(rod, stack, context)? || !context.optional(entity, Some(hooked))? {
                    return Ok(false);
                }
                if let Some(test) = item {
                    let mut found = false;
                    if let Some(entity) = hooked.this_entity
                        && let Some(stack) = context.facts().hooked_item(entity)?
                    {
                        found = test.matches(&stack, context.facts())?;
                    }
                    // Vanilla still checks the caught collection after a hooked-item match.
                    for item in items {
                        if test.matches(item, context.facts())? {
                            found = true;
                            break;
                        }
                    }
                    found
                } else {
                    true
                }
            }
            (C::ChanneledLightning(tests), E::ChanneledLightning(victims)) => {
                for test in tests {
                    if !context.any(test, victims)? {
                        return Ok(false);
                    }
                }
                true
            }
            (C::SpearMobs(count), E::SpearMobs(value)) => count.is_none_or(|min| value >= min),
            (
                C::KilledByArrow {
                    victims,
                    unique_entity_types,
                    fired_from_weapon,
                },
                E::KilledByArrow {
                    victims: params,
                    unique_entity_types: count,
                    fired_from_weapon: weapon,
                },
            ) => {
                if let Some(test) = fired_from_weapon {
                    let Some(weapon) = weapon else {
                        return Ok(false);
                    };
                    if !test.matches(weapon, context.facts())? {
                        return Ok(false);
                    }
                }
                let mut remaining: Vec<_> = params.iter().collect();
                for test in victims {
                    let mut found = None;
                    for (index, params) in remaining.iter().enumerate() {
                        if context.predicate(test, params)? {
                            found = Some(index);
                            break;
                        }
                    }
                    let Some(index) = found else {
                        return Ok(false);
                    };
                    remaining.remove(index);
                }
                unique_entity_types.matches(count)
            }
            (
                C::BeeNest { block, item, bees },
                E::BeeNest {
                    state,
                    item: stack,
                    bees: count,
                },
            ) => {
                block.matches_state(state, context.facts())?
                    && item_matches(item, stack, context)?
                    && bees.matches(count)
            }
            (
                C::Target {
                    signal_strength,
                    projectile,
                },
                E::Target {
                    signal_strength: value,
                    projectile: params,
                },
            ) => signal_strength.matches(value) && context.optional(projectile, Some(params))?,
            (C::Loot(tables), E::Loot(id)) => {
                holder_contains(tables, id, |tag| context.facts().loot_table_tag(tag))?
            }
            (
                C::ItemEntity { item, entity },
                E::ItemEntity {
                    item: stack,
                    entity: params,
                },
            ) => item_matches(item, stack, context)? && context.optional(entity, Some(params))?,
            (
                C::Lightning {
                    lightning,
                    bystander,
                },
                E::Lightning {
                    lightning: params,
                    bystanders,
                },
            ) => {
                if !context.optional(lightning, Some(params))? {
                    return Ok(false);
                }
                match bystander {
                    None => true,
                    Some(test) => context.any(test, bystanders)?,
                }
            }
            _ => {
                return Err(LootError::Decode(
                    "trigger event does not match its instance type".into(),
                ));
            }
        })
    }
}
impl InventorySlots {
    pub fn matches(&self, full: i32, empty: i32, occupied: i32) -> bool {
        self.full.matches(full) && self.empty.matches(empty) && self.occupied.matches(occupied)
    }
}

/// Port of InventoryChangeTrigger.TriggerInstance#matches. The live iterator
/// reads canonical slots lazily and retains the native changed-stack receiver.
fn inventory_matches<'a>(
    items: &[ItemPredicate],
    changed: &ItemStack,
    inventory: impl IntoIterator<Item = Result<&'a ItemStack>>,
    facts: &dyn TriggerFacts,
) -> Result<bool> {
    if items.len() == 1 {
        return Ok(!changed.is_empty() && items[0].matches(changed, facts)?);
    }
    let mut remaining: Vec<_> = items.iter().collect();
    for item in inventory {
        if remaining.is_empty() {
            break;
        }
        let item = item?;
        if !item.is_empty() {
            let mut keep = Vec::new();
            for test in remaining {
                if !test.matches(item, facts)? {
                    keep.push(test);
                }
            }
            remaining = keep;
        }
    }
    Ok(remaining.is_empty())
}

/// Port of RecipeCraftedTrigger.TriggerInstance#matches. The working list holds
/// the captured receiver identities; each predicate resolves its current value.
fn recipe_matches<'a, T>(
    recipes: &HolderSet<crate::crafting::Recipe>,
    ingredients: &[ItemPredicate],
    id: &Identifier,
    used: impl IntoIterator<Item = T>,
    resolve: impl Fn(&T) -> Result<&'a ItemStack>,
    facts: &dyn TriggerFacts,
) -> Result<bool> {
    if !holder_contains(recipes, id, |tag| facts.recipe_tag(tag))? {
        return Ok(false);
    }
    let mut remaining: Vec<_> = used.into_iter().collect();
    for test in ingredients {
        let mut found = None;
        for (index, item) in remaining.iter().enumerate() {
            if test.matches(resolve(item)?, facts)? {
                found = Some(index);
                break;
            }
        }
        let Some(index) = found else {
            return Ok(false);
        };
        remaining.remove(index);
    }
    Ok(true)
}
