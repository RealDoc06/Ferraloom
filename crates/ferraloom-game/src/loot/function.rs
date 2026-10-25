use super::{
    ContextFloatProvider, ContextIntProvider, EvalContext, Holder, HolderSet, LootData,
    LootItemCondition, Result, codec,
};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::predicates::ItemPredicate;
use ferraloom_data::Item;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{codec::VarInt, component::*};
use ferraloom_util::random::RandomSource;

/// Port of LootItemConditionalFunction and LootItemFunctions codecs.
#[derive(Clone, Debug, PartialEq)]
pub struct LootItemFunction {
    pub condition: Option<Holder<LootItemCondition>>,
    pub kind: FunctionKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionKind {
    Sequence(HolderSet<LootItemFunction>),
    Component(super::ComponentFunction),
    World(super::WorldFunction),
    List(super::ListFunction),
    Container(super::ContainerFunction),
    Item(super::ItemFunction),
    CopyCustomData(super::CopyCustomData),
    SetCount {
        count: Holder<ContextIntProvider>,
        add: bool,
    },
    SetItem(Item),
    SetDamage {
        damage: Holder<ContextFloatProvider>,
        add: bool,
    },
    LimitCount {
        min: Option<Holder<ContextIntProvider>>,
        max: Option<Holder<ContextIntProvider>>,
    },
    SetComponents(DataComponentPatch),
    SetCustomData(CustomData),
    SetPotion(Potion),
    SetOminousBottleAmplifier(Holder<ContextIntProvider>),
    Filtered {
        filter: ItemPredicate,
        on_pass: Option<Holder<LootItemFunction>>,
        on_fail: Option<Holder<LootItemFunction>>,
    },
    ApplyBonus {
        enchantment: super::Identifier,
        formula: super::BonusFormula,
    },
    EnchantedCountIncrease {
        enchantment: super::Identifier,
        count: Holder<ContextFloatProvider>,
        limit: i32,
    },
    EnchantRandomly {
        options: Option<ferraloom_protocol::codec::HolderSet>,
        only_compatible: bool,
        include_additional_cost_component: bool,
    },
    EnchantWithLevels {
        levels: Holder<ContextIntProvider>,
        options: Option<ferraloom_protocol::codec::HolderSet>,
        include_additional_cost_component: bool,
    },
    ExplosionDecay,
    Discard,
}
impl LootItemFunction {
    /// Decode through the same schema using NbtOps scalar/component readers.
    pub fn decode_nbt(tag: &ferraloom_nbt::Tag) -> Result<Self> {
        Self::decode(&super::NbtOps::new(tag))
    }

    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        // SequenceFunction's inline codec accepts an array (and holder-set tags).
        if v.is_array() || v.is_string() {
            return Ok(Self {
                condition: None,
                kind: FunctionKind::Sequence(HolderSet::decode(v, Self::decode_typed)?),
            });
        }
        Self::decode_typed(v)
    }
    fn decode_typed<I: DecodeInput>(v: &I) -> Result<Self> {
        let condition = codec::optional(v, "condition", |v| {
            Holder::decode(v, LootItemCondition::decode)
        })?;
        let name = codec::kind(v)?;
        let kind = match name.as_str() {
            "minecraft:copy_custom_data" => {
                FunctionKind::CopyCustomData(super::CopyCustomData::decode(v)?)
            }
            "minecraft:sequence" => FunctionKind::Sequence(HolderSet::decode(
                codec::field(v, "functions")?,
                Self::decode_typed,
            )?),
            "minecraft:set_count" => FunctionKind::SetCount {
                count: Holder::decode(codec::field(v, "count")?, ContextIntProvider::decode)?,
                add: codec::optional(v, "add", codec::boolean)?.unwrap_or(false),
            },
            "minecraft:set_item" => {
                let id = codec::id(codec::field(v, "item")?)?;
                FunctionKind::SetItem(
                    Item::from_name(id.as_str())
                        .ok_or_else(|| codec::error(format!("unknown item {id}")))?,
                )
            }
            "minecraft:set_damage" => FunctionKind::SetDamage {
                damage: Holder::decode(codec::field(v, "damage")?, ContextFloatProvider::decode)?,
                add: codec::optional(v, "add", codec::boolean)?.unwrap_or(false),
            },
            "minecraft:limit_count" => {
                let limit = codec::field(v, "limit")?;
                FunctionKind::LimitCount {
                    min: codec::optional(limit, "min", |v| {
                        Holder::decode(v, ContextIntProvider::decode)
                    })?,
                    max: codec::optional(limit, "max", |v| {
                        Holder::decode(v, ContextIntProvider::decode)
                    })?,
                }
            }
            "minecraft:set_components" => {
                FunctionKind::SetComponents(codec::persistent(codec::field(v, "components")?)?)
            }
            "minecraft:set_custom_data" => {
                FunctionKind::SetCustomData(CustomData::from_input(codec::field(v, "tag")?)?)
            }
            "minecraft:set_potion" => {
                FunctionKind::SetPotion(Potion::from_input(codec::field(v, "id")?)?)
            }
            "minecraft:set_ominous_bottle_amplifier" => FunctionKind::SetOminousBottleAmplifier(
                Holder::decode(codec::field(v, "amplifier")?, ContextIntProvider::decode)?,
            ),
            "minecraft:filtered" => FunctionKind::Filtered {
                filter: ItemPredicate::decode(codec::field(v, "item_filter")?)?,
                on_pass: codec::optional(v, "on_pass", |v| Holder::decode(v, Self::decode))?,
                on_fail: codec::optional(v, "on_fail", |v| Holder::decode(v, Self::decode))?,
            },
            "minecraft:apply_bonus" => FunctionKind::ApplyBonus {
                enchantment: codec::id(codec::field(v, "enchantment")?)?,
                formula: super::BonusFormula::decode(v)?,
            },
            "minecraft:enchanted_count_increase" => FunctionKind::EnchantedCountIncrease {
                enchantment: codec::id(codec::field(v, "enchantment")?)?,
                count: Holder::decode(codec::field(v, "count")?, ContextFloatProvider::decode)?,
                limit: codec::optional(v, "limit", codec::int)?.unwrap_or(0),
            },
            "minecraft:enchant_randomly" => FunctionKind::EnchantRandomly {
                options: codec::optional(v, "options", |v| {
                    Ok(ferraloom_protocol::codec::HolderSet::from_input(v)?)
                })?,
                only_compatible: codec::optional(v, "only_compatible", codec::boolean)?
                    .unwrap_or(true),
                include_additional_cost_component: codec::optional(
                    v,
                    "include_additional_cost_component",
                    codec::boolean,
                )?
                .unwrap_or(false),
            },
            "minecraft:enchant_with_levels" => FunctionKind::EnchantWithLevels {
                levels: Holder::decode(codec::field(v, "levels")?, ContextIntProvider::decode)?,
                options: codec::optional(v, "options", |v| {
                    Ok(ferraloom_protocol::codec::HolderSet::from_input(v)?)
                })?,
                include_additional_cost_component: codec::optional(
                    v,
                    "include_additional_cost_component",
                    codec::boolean,
                )?
                .unwrap_or(false),
            },
            "minecraft:explosion_decay" => FunctionKind::ExplosionDecay,
            "minecraft:discard" => FunctionKind::Discard,
            _ => {
                if let Some(function) = super::ComponentFunction::decode(name.as_str(), v)? {
                    FunctionKind::Component(function)
                } else if let Some(function) = super::WorldFunction::decode(name.as_str(), v)? {
                    FunctionKind::World(function)
                } else if let Some(function) = super::ListFunction::decode(name.as_str(), v)? {
                    FunctionKind::List(function)
                } else if let Some(function) = super::ContainerFunction::decode(name.as_str(), v)? {
                    FunctionKind::Container(function)
                } else if let Some(function) = super::ItemFunction::decode(name.as_str(), v)? {
                    FunctionKind::Item(function)
                } else {
                    return Err(codec::error(format!("unknown loot function {name}")));
                }
            }
        };
        Ok(Self { condition, kind })
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        if let Some(condition) = &self.condition
            && !condition.test(data, c)?
        {
            return Ok(stack);
        }
        match &self.kind {
            FunctionKind::Item(function) => stack = function.apply(stack, data, c)?,
            FunctionKind::CopyCustomData(function) => stack = function.apply(stack, c)?,
            FunctionKind::Container(function) => stack = function.apply(stack, data, c)?,
            FunctionKind::List(function) => stack = function.apply(stack, data, c)?,
            FunctionKind::World(function) => stack = function.apply(stack, data, c)?,
            FunctionKind::Component(function) => stack = function.apply(stack, data, c)?,
            FunctionKind::Sequence(functions) => {
                for function in functions.resolve(data)? {
                    stack = function.apply(stack, data, c)?;
                }
            }
            FunctionKind::SetCount { count, add } => {
                let base = if *add { stack_count(&stack) } else { 0 };
                let count = count.get(data, c)?;
                if let Some(s) = &mut stack.0 {
                    s.count = base.wrapping_add(count);
                }
            }
            FunctionKind::SetItem(item) => {
                if !stack.is_empty() {
                    let s = stack.0.take().expect("nonempty");
                    stack = ItemStack::new(*item, s.count, s.components);
                }
            }
            FunctionKind::LimitCount { min, max } => {
                let mut count = stack_count(&stack);
                // Mth.clamp evaluates both bounds, even when the first clamps.
                let min = min.as_ref().map(|v| v.get(data, c)).transpose()?;
                let max = max.as_ref().map(|v| v.get(data, c)).transpose()?;
                if let Some(min) = min {
                    count = count.max(min);
                }
                if let Some(max) = max {
                    count = count.min(max);
                }
                if let Some(s) = &mut stack.0 {
                    s.count = count;
                }
            }
            FunctionKind::SetDamage { damage, add } => {
                let components = stack.components()?;
                if let Some(DataComponent::MaxDamage(max)) =
                    components.get(ComponentTypeId::MAX_DAMAGE)
                    && components.get(ComponentTypeId::DAMAGE).is_some()
                    && components.get(ComponentTypeId::UNBREAKABLE).is_none()
                {
                    let old = match components.get(ComponentTypeId::DAMAGE) {
                        Some(DataComponent::Damage(v)) => v.0.clamp(0, max.0),
                        _ => 0,
                    };
                    let base = if *add {
                        1.0 - old as f32 / max.0 as f32
                    } else {
                        0.0
                    };
                    let pct = 1.0 - (damage.get(data, c)? + base).clamp(0.0, 1.0);
                    let value =
                        ferraloom_util::mth::floor(f64::from(pct * max.0 as f32)).clamp(0, max.0);
                    set_component(&mut stack, DataComponent::Damage(VarInt(value)));
                }
            }
            FunctionKind::SetComponents(patch) => {
                let previous = stack.clone();
                if let Some(s) = &mut stack.0 {
                    for key in &patch.removed {
                        s.components.remove(*key);
                    }
                    for component in &patch.added {
                        s.components.set(component.clone());
                    }
                }
                if !valid_components(&stack, c.facts())? {
                    stack = previous;
                }
            }
            FunctionKind::SetCustomData(value) => {
                let mut tag = match stack.components()?.get(ComponentTypeId::CUSTOM_DATA) {
                    Some(DataComponent::CustomData(data)) => {
                        data.0.as_compound().cloned().unwrap_or_default()
                    }
                    _ => CompoundTag::new(),
                };
                tag.merge(value.0.as_compound().expect("custom data compound"));
                if tag.is_empty() {
                    if let Some(s) = &mut stack.0 {
                        s.components.remove(ComponentTypeId::CUSTOM_DATA);
                    }
                } else {
                    set_component(
                        &mut stack,
                        DataComponent::CustomData(CustomData(Tag::Compound(tag))),
                    );
                }
            }
            FunctionKind::SetPotion(potion) => {
                let mut contents = match stack.components()?.get(ComponentTypeId::POTION_CONTENTS) {
                    Some(DataComponent::PotionContents(v)) => v.clone(),
                    _ => PotionContents {
                        potion: None,
                        custom_color: None,
                        custom_effects: Vec::new(),
                        custom_name: None,
                    },
                };
                contents.potion = Some(potion.clone());
                set_component(&mut stack, DataComponent::PotionContents(contents));
            }
            FunctionKind::SetOminousBottleAmplifier(value) => {
                let value = value.get(data, c)?.clamp(0, 4);
                set_component(
                    &mut stack,
                    DataComponent::OminousBottleAmplifier(VarInt(value)),
                );
            }
            FunctionKind::Filtered {
                filter,
                on_pass,
                on_fail,
            } => {
                let function = if filter.matches(&stack, c.facts())? {
                    on_pass
                } else {
                    on_fail
                };
                if let Some(function) = function {
                    stack = function.apply(stack, data, c)?;
                }
            }
            FunctionKind::ApplyBonus {
                enchantment,
                formula,
            } => stack = formula.apply(stack, enchantment, c)?,
            FunctionKind::EnchantedCountIncrease {
                enchantment,
                count,
                limit,
            } => {
                stack = super::bonus::enchanted_count_increase(
                    stack,
                    enchantment,
                    count,
                    *limit,
                    data,
                    c,
                )?
            }
            FunctionKind::EnchantRandomly {
                options,
                only_compatible,
                include_additional_cost_component,
            } => {
                stack = c.runtime.enchant_item(
                    &super::EnchantmentOperation::Randomly {
                        options: options.clone(),
                        only_compatible: *only_compatible,
                        include_additional_cost_component: *include_additional_cost_component,
                    },
                    stack,
                    c.params,
                )?;
            }
            FunctionKind::EnchantWithLevels {
                levels,
                options,
                include_additional_cost_component,
            } => {
                let levels = levels.get(data, c)?;
                stack = c.runtime.enchant_item(
                    &super::EnchantmentOperation::WithLevels {
                        levels,
                        options: options.clone(),
                        include_additional_cost_component: *include_additional_cost_component,
                    },
                    stack,
                    c.params,
                )?;
            }
            FunctionKind::ExplosionDecay => {
                if let Some(radius) = c.params.explosion_radius {
                    let mut count = 0;
                    for _ in 0..stack_count(&stack) {
                        if c.random().next_float() <= 1.0 / radius {
                            count += 1;
                        }
                    }
                    if let Some(s) = &mut stack.0 {
                        s.count = count;
                    }
                }
            }
            FunctionKind::Discard => stack = ItemStack::default(),
        }
        // PatchedDataComponentMap removes additions equal to item defaults.
        if let Some(s) = &mut stack.0 {
            s.components
                .sanitize(DataComponentMap::item_defaults(s.item)?);
        }
        Ok(stack)
    }
}
pub(crate) fn stack_count(stack: &ItemStack) -> i32 {
    if stack.is_empty() {
        0
    } else {
        stack.0.as_ref().expect("nonempty").count
    }
}
pub(crate) fn max_stack_size(stack: &ItemStack) -> Result<i32> {
    Ok(
        match stack.components()?.get(ComponentTypeId::MAX_STACK_SIZE) {
            Some(DataComponent::MaxStackSize(value)) => value.0,
            _ => 1,
        },
    )
}
fn set_component(stack: &mut ItemStack, component: DataComponent) {
    if let Some(s) = &mut stack.0 {
        s.components.set(component);
    }
}
fn valid_components(stack: &ItemStack, facts: &dyn super::LootFacts) -> Result<bool> {
    let map = stack.components()?;
    let max = match map.get(ComponentTypeId::MAX_STACK_SIZE) {
        Some(DataComponent::MaxStackSize(value)) => value.0,
        _ => 1,
    };
    if stack_count(stack) > max || (map.get(ComponentTypeId::MAX_DAMAGE).is_some() && max > 1) {
        return Ok(false);
    }
    let contained = |item: &ItemStackTemplate| -> Result<bool> {
        let stack = ItemStack::new(item.item, item.count, item.components.clone());
        Ok(item.count <= max_stack_size(&stack)?)
    };
    if let Some(DataComponent::Container(items)) = map.get(ComponentTypeId::CONTAINER) {
        for item in items.0.iter().flatten() {
            if !contained(item)? {
                return Ok(false);
            }
        }
    }
    if let Some(DataComponent::BundleContents(items)) = map.get(ComponentTypeId::BUNDLE_CONTENTS) {
        for item in items {
            if !contained(item)? {
                return Ok(false);
            }
        }
        if !items.is_empty() && !facts.bundle_weight_valid(items)? {
            return Ok(false);
        }
    }
    if let Some(DataComponent::ChargedProjectiles(items)) =
        map.get(ComponentTypeId::CHARGED_PROJECTILES)
    {
        for item in &items.0 {
            if !contained(item)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
impl Holder<LootItemFunction> {
    pub(crate) fn apply(
        &self,
        stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        if let Self::Reference(id) = self {
            let key = ("item_modifier", id.clone());
            if !c.visited.insert(key.clone()) {
                return Ok(stack);
            }
            let result = self.resolve(data)?.apply(stack, data, c);
            c.visited.remove(&key);
            result
        } else {
            self.resolve(data)?.apply(stack, data, c)
        }
    }
}
