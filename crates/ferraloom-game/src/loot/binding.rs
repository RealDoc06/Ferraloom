//! Cold-path RegistryOps boundary. Registry owners supply the active snapshot;
//! evaluation uses the validated identities and never calls this lookup.
use super::{Identifier, LootError, Result, codec};
use ferraloom_protocol::{codec::HolderSet, component::*};
use std::collections::BTreeSet;

/// Registry identities needed by loot and advancement predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegistryKey {
    Item,
    Block,
    EntityType,
    Fluid,
    Enchantment,
    Potion,
    MobEffect,
    Attribute,
    Instrument,
    JukeboxSong,
    TrimMaterial,
    TrimPattern,
    BannerPattern,
    VillagerType,
    Biome,
    Structure,
    DamageType,
    WorldClock,
    EnvironmentAttribute,
}
impl RegistryKey {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Block => "block",
            Self::EntityType => "entity_type",
            Self::Fluid => "fluid",
            Self::Enchantment => "enchantment",
            Self::Potion => "potion",
            Self::MobEffect => "mob_effect",
            Self::Attribute => "attribute",
            Self::Instrument => "instrument",
            Self::JukeboxSong => "jukebox_song",
            Self::TrimMaterial => "trim_material",
            Self::TrimPattern => "trim_pattern",
            Self::BannerPattern => "banner_pattern",
            Self::VillagerType => "villager_type",
            Self::Biome => "worldgen/biome",
            Self::Structure => "worldgen/structure",
            Self::DamageType => "damage_type",
            Self::WorldClock => "world_clock",
            Self::EnvironmentAttribute => "environment_attribute",
        }
    }
}
/// A value holder and a named holder set have different binding rules.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegistryReference {
    Value(Identifier),
    Tag(Identifier),
}
impl RegistryReference {
    pub fn id(&self) -> &Identifier {
        match self {
            Self::Value(id) | Self::Tag(id) => id,
        }
    }
}
/// A registry-qualified identity returned by the owner of a validated snapshot.
/// The owning registry retains the value; loot keeps no duplicate registry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegistryHolder {
    pub registry: RegistryKey,
    pub reference: RegistryReference,
}
/// Minimal shared RegistryOps integration point. Called only while loading or
/// binding a standalone predicate, including named holder-set existence checks.
pub trait RegistryLookup {
    fn lookup(
        &self,
        registry: RegistryKey,
        reference: &RegistryReference,
    ) -> Result<Option<RegistryHolder>>;
}

/// Bind a decoded predicate to a registry snapshot before publishing it.
/// Decoding alone represents the symbolic intermediate form, like RegistryOps'
/// loading phase. An absent lookup rejects external references explicitly.
pub trait RegistryBindable {
    fn bind_registries(&self, lookup: Option<&dyn RegistryLookup>) -> Result<Vec<RegistryHolder>>;
}
pub(crate) struct BindingContext<'a> {
    lookup: Option<&'a dyn RegistryLookup>,
    holders: BTreeSet<RegistryHolder>,
}
impl<'a> BindingContext<'a> {
    pub(crate) fn new(lookup: Option<&'a dyn RegistryLookup>) -> Self {
        Self {
            lookup,
            holders: BTreeSet::new(),
        }
    }
    pub(crate) fn finish(self) -> Vec<RegistryHolder> {
        self.holders.into_iter().collect()
    }
    fn reference(&mut self, registry: RegistryKey, reference: RegistryReference) -> Result<()> {
        let expected = RegistryHolder {
            registry,
            reference,
        };
        if self.holders.contains(&expected) {
            return Ok(());
        }
        let lookup = self
            .lookup
            .ok_or_else(|| LootError::Unsupported("registry.binding".into()))?;
        let holder = lookup
            .lookup(registry, &expected.reference)?
            .ok_or_else(|| LootError::MissingReference {
                registry: registry.name(),
                id: expected.reference.id().clone(),
            })?;
        if holder != expected {
            return Err(codec::error("registry lookup returned a different holder"));
        }
        self.holders.insert(holder);
        Ok(())
    }
    pub(crate) fn value(&mut self, registry: RegistryKey, id: &Identifier) -> Result<()> {
        self.reference(registry, RegistryReference::Value(id.clone()))
    }
    pub(crate) fn set(&mut self, registry: RegistryKey, set: &HolderSet) -> Result<()> {
        match set {
            HolderSet::Named(id) => self.reference(registry, RegistryReference::Tag(id.clone())),
            HolderSet::Direct(ids) => {
                for id in ids {
                    self.value(registry, id)?;
                }
                Ok(())
            }
        }
    }
    fn optional_set(&mut self, registry: RegistryKey, set: &Option<HolderSet>) -> Result<()> {
        if let Some(set) = set {
            self.set(registry, set)?;
        }
        Ok(())
    }
    fn collection<T>(
        &mut self,
        collection: &CollectionPredicate<T>,
        mut visit: impl FnMut(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        for value in collection.contains.iter().flatten() {
            visit(self, value)?;
        }
        for value in collection.count.iter().flatten() {
            visit(self, &value.test)?;
        }
        Ok(())
    }
    pub(crate) fn components(&mut self, components: &DataComponentMatchers) -> Result<()> {
        for value in &components.exact {
            self.component(value)?;
        }
        for predicate in &components.partial.0 {
            match predicate {
                ComponentPredicate::Enchantments(values)
                | ComponentPredicate::StoredEnchantments(values) => {
                    for value in values {
                        self.optional_set(RegistryKey::Enchantment, &value.enchantments)?;
                    }
                }
                ComponentPredicate::Potions(value) => {
                    self.optional_set(RegistryKey::Potion, &value.potions)?;
                    if let Some(effects) = &value.effects {
                        self.collection(effects, |c, v| c.effects(v))?;
                    }
                }
                ComponentPredicate::AttributeModifiers(value) => {
                    if let Some(values) = &value.modifiers {
                        self.collection(values, |c, v| {
                            c.optional_set(RegistryKey::Attribute, &v.attribute)
                        })?;
                    }
                }
                ComponentPredicate::Trim(value) => {
                    self.optional_set(RegistryKey::TrimMaterial, &value.material)?;
                    self.optional_set(RegistryKey::TrimPattern, &value.pattern)?;
                }
                ComponentPredicate::JukeboxPlayable(value) => {
                    self.optional_set(RegistryKey::JukeboxSong, &value.song)?
                }
                ComponentPredicate::VillagerType(value) => {
                    self.set(RegistryKey::VillagerType, value)?
                }
                ComponentPredicate::Container(value)
                | ComponentPredicate::BundleContents(value) => {
                    if let Some(items) = &value.items {
                        self.collection(items, |c, v| c.item(v))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn holder<T>(
        &mut self,
        registry: RegistryKey,
        value: &ferraloom_protocol::codec::Holder<T>,
    ) -> Result<()> {
        if let ferraloom_protocol::codec::Holder::Reference(id) = value {
            self.value(registry, id)?;
        }
        Ok(())
    }
    fn template(&mut self, value: &ItemStackTemplate) -> Result<()> {
        for component in &value.components.added {
            self.component(component)?;
        }
        Ok(())
    }
    fn component(&mut self, value: &DataComponent) -> Result<()> {
        use DataComponent as C;
        match value {
            C::Enchantments(values) | C::StoredEnchantments(values) => {
                for (id, _) in &values.0 {
                    self.value(RegistryKey::Enchantment, &id.0)?;
                }
            }
            C::DamageType(id) => self.value(RegistryKey::DamageType, &id.0)?,
            C::DamageResistant(set) => self.set(RegistryKey::DamageType, &set.0)?,
            C::AttributeModifiers(values) => {
                for value in values {
                    self.value(RegistryKey::Attribute, &value.attribute.0)?;
                }
            }
            C::Repairable(set) => self.set(RegistryKey::Item, &set.0)?,
            C::Tool(value) => {
                for rule in &value.rules {
                    self.set(RegistryKey::Block, &rule.blocks.0)?;
                }
            }
            C::PotionContents(value) => {
                if let Some(id) = &value.potion {
                    self.value(RegistryKey::Potion, &id.0)?;
                }
                for effect in &value.custom_effects {
                    self.value(RegistryKey::MobEffect, &effect.effect.0)?;
                }
            }
            C::SuspiciousStewEffects(values) => {
                for value in values {
                    self.value(RegistryKey::MobEffect, &value.effect.0)?;
                }
            }
            C::Instrument(value) => self.holder(RegistryKey::Instrument, &value.0)?,
            C::ProvidesTrimMaterial(value) => self.holder(RegistryKey::TrimMaterial, &value.0)?,
            C::Trim(value) => {
                self.holder(RegistryKey::TrimMaterial, &value.material.0)?;
                self.holder(RegistryKey::TrimPattern, &value.pattern.0)?;
            }
            C::JukeboxPlayable(value) => self.holder(RegistryKey::JukeboxSong, &value.0)?,
            C::ProvidesBannerPatterns(value) => self.set(RegistryKey::BannerPattern, &value.0)?,
            C::BannerPatterns(values) => {
                for value in values {
                    self.holder(RegistryKey::BannerPattern, &value.pattern.0)?;
                }
            }
            C::VillagerVariant(value) => self.value(RegistryKey::VillagerType, &value.0)?,
            C::EntityData(value) => self.value(RegistryKey::EntityType, &value.entity_type.0)?,
            C::Lock(value) => self.item(value)?,
            C::CanPlaceOn(value) | C::CanBreak(value) => {
                for value in &value.predicates {
                    self.block(&crate::predicates::BlockPredicate(value.clone()))?;
                }
            }
            C::Container(values) => {
                for value in values.0.iter().flatten() {
                    self.template(value)?;
                }
            }
            C::ChargedProjectiles(values) => {
                for value in &values.0 {
                    self.template(value)?;
                }
            }
            C::BundleContents(values) => {
                for value in values {
                    self.template(value)?;
                }
            }
            C::UseRemainder(value) | C::SulfurCubeContent(value) => self.template(value)?,
            // These arbitrary persistent value families require shared RegistryOps
            // support. Reject the pending binding instead of publishing unchecked IDs.
            C::BlockTransformer(_)
            | C::BlockEntityData(_)
            | C::Consumable(_)
            | C::DeathProtection(_)
            | C::Equippable(_)
            | C::BlocksAttacks(_)
            | C::PiercingWeapon(_)
            | C::KineticWeapon(_)
            | C::WolfVariant(_)
            | C::WolfSoundVariant(_)
            | C::PigVariant(_)
            | C::CowVariant(_)
            | C::ChickenVariant(_)
            | C::FrogVariant(_)
            | C::CatVariant(_)
            | C::PaintingVariant(_)
            | C::ProvidesPotteryPattern(_)
            | C::BreakSound(_)
            | C::MapDecorations(_) => {
                return Err(LootError::Unsupported(
                    if self.lookup.is_none() {
                        "registry.binding"
                    } else {
                        "registry.binding.component_values"
                    }
                    .into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
    fn effects(&mut self, effects: &MobEffectsPredicate) -> Result<()> {
        for (effect, _) in &effects.0 {
            self.value(RegistryKey::MobEffect, &effect.0)?;
        }
        Ok(())
    }
    fn item(&mut self, value: &LockCode) -> Result<()> {
        if let Some(items) = &value.items {
            self.set(RegistryKey::Item, &items.0)?;
        }
        self.components(&value.components)
    }
    fn block(&mut self, value: &crate::predicates::BlockPredicate) -> Result<()> {
        if let Some(blocks) = &value.0.blocks {
            self.set(RegistryKey::Block, &blocks.0)?;
        }
        self.components(&value.0.components)
    }
    pub(crate) fn location(&mut self, value: &crate::predicates::LocationPredicate) -> Result<()> {
        self.optional_set(RegistryKey::Biome, &value.biomes)?;
        self.optional_set(RegistryKey::Structure, &value.structures)?;
        if let Some(block) = &value.block {
            self.block(block)?;
        }
        if let Some(fluid) = &value.fluid {
            self.optional_set(RegistryKey::Fluid, &fluid.fluids)?;
        }
        Ok(())
    }
    pub(crate) fn entity(&mut self, value: &crate::predicates::EntityPredicate) -> Result<()> {
        use crate::predicates::EntitySubPredicate as P;
        for part in &value.parts {
            match part {
                P::Type(value) => self.set(RegistryKey::EntityType, value)?,
                P::Location(value) | P::SteppingOn(value) | P::MovementAffectedBy(value) => {
                    self.location(value)?
                }
                P::Effects(value) => self.effects(value)?,
                P::Slots(values) => {
                    for (_, value) in values {
                        self.item(&value.0)?;
                    }
                }
                P::Equipment(values) => {
                    for (_, value) in values {
                        self.item(&value.0)?;
                    }
                }
                P::Player(value) => {
                    if let Some(value) = &value.looking_at {
                        self.entity(value)?;
                    }
                }
                P::Lightning {
                    entity_struck: Some(value),
                    ..
                }
                | P::Vehicle(value)
                | P::Passenger(value)
                | P::TargetedEntity(value) => self.entity(value)?,
                P::Components(value) => self.components(value)?,
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn damage(
        &mut self,
        value: &crate::predicates::DamageSourcePredicate,
    ) -> Result<()> {
        for tag in &value.tags {
            self.set(RegistryKey::DamageType, &tag.tag)?;
        }
        for entity in [&value.direct_entity, &value.source_entity]
            .into_iter()
            .flatten()
        {
            self.entity(entity)?;
        }
        Ok(())
    }
    pub(crate) fn item_predicate(
        &mut self,
        value: &crate::predicates::ItemPredicate,
    ) -> Result<()> {
        self.item(&value.0)
    }
    pub(crate) fn block_predicate(
        &mut self,
        value: &crate::predicates::BlockPredicate,
    ) -> Result<()> {
        self.block(value)
    }
    fn fluid_predicate(&mut self, value: &crate::predicates::FluidPredicate) -> Result<()> {
        self.optional_set(RegistryKey::Fluid, &value.fluids)
    }
    fn damage_predicate(&mut self, value: &crate::predicates::DamagePredicate) -> Result<()> {
        if let Some(entity) = &value.source_entity {
            self.entity(entity)?;
        }
        if let Some(source) = &value.damage_type {
            self.damage(source)?;
        }
        Ok(())
    }
}
macro_rules! bindable {
    ($ty:ty, $method:ident) => {
        impl RegistryBindable for $ty {
            fn bind_registries(
                &self,
                lookup: Option<&dyn RegistryLookup>,
            ) -> Result<Vec<RegistryHolder>> {
                let mut context = BindingContext::new(lookup);
                context.$method(self)?;
                Ok(context.finish())
            }
        }
    };
}
bindable!(crate::predicates::ItemPredicate, item_predicate);
bindable!(crate::predicates::BlockPredicate, block_predicate);
bindable!(crate::predicates::EntityPredicate, entity);
bindable!(crate::predicates::LocationPredicate, location);
bindable!(crate::predicates::DamageSourcePredicate, damage);
bindable!(crate::predicates::DamagePredicate, damage_predicate);
bindable!(crate::predicates::FluidPredicate, fluid_predicate);

trait Bind {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()>;
}
impl<T: Bind> Bind for super::Holder<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        if let Self::Inline(value) = self {
            value.bind(c)?;
        }
        Ok(())
    }
}
impl<T: Bind> Bind for super::HolderSet<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        if let Self::Direct(values) = self {
            values.bind(c)?;
        }
        Ok(())
    }
}
impl<T: Bind> Bind for Option<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        if let Some(value) = self {
            value.bind(c)?;
        }
        Ok(())
    }
}
impl<T: Bind> Bind for Vec<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        for value in self {
            value.bind(c)?;
        }
        Ok(())
    }
}
impl<T: Bind> Bind for super::condition::ProviderRange<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        match self {
            Self::Point(value) => value.bind(c),
            Self::Line { min, max } => {
                min.bind(c)?;
                max.bind(c)
            }
        }
    }
}
impl Bind for super::LootTable {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        self.modifier.bind(c)?;
        for pool in &self.pools {
            pool.condition.bind(c)?;
            pool.modifier.bind(c)?;
            pool.rolls.bind(c)?;
            pool.bonus_rolls.bind(c)?;
            pool.entries.bind(c)?;
        }
        Ok(())
    }
}
impl Bind for super::LootPoolEntryContainer {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        self.condition.bind(c)?;
        self.modifier.bind(c)?;
        match &self.kind {
            super::EntryKind::Tag { items, .. } => c.set(RegistryKey::Item, items),
            super::EntryKind::Table { tables, .. } => tables.bind(c),
            super::EntryKind::Alternatives(values)
            | super::EntryKind::Sequence(values)
            | super::EntryKind::Group(values) => values.bind(c),
            super::EntryKind::Slots(value) => value.bind(c),
            _ => Ok(()),
        }
    }
}
impl<T> Bind for super::number::NumberProvider<T> {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        use super::number::NumberProvider as P;
        match self {
            P::EnvironmentAttribute { attribute, .. } => {
                c.value(RegistryKey::EnvironmentAttribute, attribute)
            }
            P::Unary(_, value) | P::Storage(_, _, value) | P::Score(_, _, value) => value.bind(c),
            P::Binary(_, a, b) | P::Uniform(a, b) => {
                a.bind(c)?;
                b.bind(c)
            }
            P::Aggregate(_, values) => values.bind(c),
            P::Conditional(test, a, b) => {
                test.bind(c)?;
                a.bind(c)?;
                b.bind(c)
            }
            P::Dispatcher(values, fallback) => {
                for (test, value) in values {
                    test.bind(c)?;
                    value.bind(c)?;
                }
                fallback.bind(c)
            }
            P::Weighted(values) => {
                for (value, _) in values {
                    value.bind(c)?;
                }
                Ok(())
            }
            P::FromInt(value) => value.bind(c),
            P::FromFloat(value) => value.bind(c),
            P::Binomial(n, p) => {
                n.bind(c)?;
                p.bind(c)
            }
            _ => Ok(()),
        }
    }
}
impl Bind for super::LootItemCondition {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        match self {
            Self::Inverted(value) => value.bind(c),
            Self::AllOf(values) | Self::AnyOf(values) => values.bind(c),
            Self::RandomChance(value) => value.bind(c),
            Self::TableBonus(id, _)
            | Self::RandomChanceWithEnchantedBonus {
                enchantment: id, ..
            } => c.value(RegistryKey::Enchantment, id),
            Self::MatchTool(Some(value)) => c.item_predicate(value),
            Self::MatchBlock(value) => c.block(value),
            Self::EntityProperties(_, Some(value)) => c.entity(value),
            Self::LocationCheck(Some(value), _) => c.location(value),
            Self::DamageSource(Some(value)) => c.damage(value),
            Self::TimeCheck { clock, value, .. } => {
                c.value(RegistryKey::WorldClock, clock)?;
                value.bind(c)
            }
            Self::EnvironmentAttribute { attribute, .. } => {
                c.value(RegistryKey::EnvironmentAttribute, attribute)
            }
            Self::EntityScores(_, scores) => {
                for (_, value) in scores {
                    value.bind(c)?;
                }
                Ok(())
            }
            Self::IntValueCheck(value, range) => {
                value.bind(c)?;
                range.bind(c)
            }
            Self::FloatValueCheck(value, range) => {
                value.bind(c)?;
                range.bind(c)
            }
            _ => Ok(()),
        }
    }
}
impl Bind for super::SlotSource {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        match self {
            Self::Group(values) => values.bind(c),
            Self::Filtered { source, filter } => {
                source.bind(c)?;
                c.item_predicate(filter)
            }
            Self::Limit { source, .. } | Self::Contents { source, .. } => source.bind(c),
            _ => Ok(()),
        }
    }
}
impl Bind for super::LootItemFunction {
    fn bind(&self, c: &mut BindingContext<'_>) -> Result<()> {
        use super::{
            ComponentFunction as C, ContainerFunction as K, FunctionKind as F, ItemFunction as I,
            ListFunction as L, WorldFunction as W,
        };
        self.condition.bind(c)?;
        match &self.kind {
            F::Sequence(values) => values.bind(c),
            F::SetCount { count, .. } => count.bind(c),
            F::SetDamage { damage, .. } => damage.bind(c),
            F::LimitCount { min, max } => {
                min.bind(c)?;
                max.bind(c)
            }
            F::SetOminousBottleAmplifier(value) => value.bind(c),
            F::SetPotion(value) => c.value(RegistryKey::Potion, &value.0),
            F::SetComponents(value) => {
                for value in &value.added {
                    c.component(value)?;
                }
                Ok(())
            }
            F::Filtered {
                filter,
                on_pass,
                on_fail,
            } => {
                c.item_predicate(filter)?;
                on_pass.bind(c)?;
                on_fail.bind(c)
            }
            F::ApplyBonus { enchantment, .. } => c.value(RegistryKey::Enchantment, enchantment),
            F::EnchantedCountIncrease {
                enchantment, count, ..
            } => {
                c.value(RegistryKey::Enchantment, enchantment)?;
                count.bind(c)
            }
            F::EnchantRandomly { options, .. } => c.optional_set(RegistryKey::Enchantment, options),
            F::EnchantWithLevels {
                levels, options, ..
            } => {
                levels.bind(c)?;
                c.optional_set(RegistryKey::Enchantment, options)
            }
            F::Component(value) => match value {
                C::SetInstrument(values) => c.set(RegistryKey::Instrument, values),
                C::SetRandomPotion(values) => c.optional_set(RegistryKey::Potion, values),
                C::SetBannerPattern { patterns, .. } => {
                    for pattern in patterns {
                        if let ferraloom_protocol::codec::Holder::Reference(id) = &pattern.pattern.0
                        {
                            c.value(RegistryKey::BannerPattern, id)?;
                        }
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            F::World(value) => match value {
                W::SetEnchantments { enchantments, .. } => {
                    for (id, value) in enchantments {
                        c.value(RegistryKey::Enchantment, id)?;
                        value.bind(c)?;
                    }
                    Ok(())
                }
                W::SetStewEffect(values) => {
                    for (effect, value) in values {
                        c.value(RegistryKey::MobEffect, &effect.0)?;
                        value.bind(c)?;
                    }
                    Ok(())
                }
                W::ExplorationMap(value) => c.set(RegistryKey::Structure, &value.destination),
                _ => Ok(()),
            },
            F::Item(value) => match value {
                I::SetAttributes { modifiers, .. } => {
                    for modifier in modifiers {
                        c.value(RegistryKey::Attribute, &modifier.attribute.0)?;
                        modifier.amount.bind(c)?;
                    }
                    Ok(())
                }
                I::SetRandomDyes(value) => value.bind(c),
                _ => Ok(()),
            },
            F::Container(value) => match value {
                K::Set { entries, .. } => entries.bind(c),
                K::Modify { modifier, .. } => modifier.bind(c),
            },
            F::List(L::SetCustomModelData { floats, colors, .. }) => {
                if let Some(edit) = floats {
                    edit.values.bind(c)?;
                }
                if let Some(edit) = colors {
                    edit.values.bind(c)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
macro_rules! bind_loot {
    ($($ty:ty),* $(,)?) => { $(
        impl RegistryBindable for $ty {
            fn bind_registries(&self, lookup: Option<&dyn RegistryLookup>) -> Result<Vec<RegistryHolder>> {
                let mut context = BindingContext::new(lookup);
                self.bind(&mut context)?;
                Ok(context.finish())
            }
        }
    )* };
}
bind_loot!(
    super::LootTable,
    super::LootItemFunction,
    super::LootItemCondition,
    super::SlotSource,
    super::ContextIntProvider,
    super::ContextFloatProvider
);
impl RegistryBindable for super::LootData {
    fn bind_registries(&self, lookup: Option<&dyn RegistryLookup>) -> Result<Vec<RegistryHolder>> {
        let mut c = BindingContext::new(lookup);
        for value in self.tables.values() {
            value.bind(&mut c)?;
        }
        for value in self.modifiers.values() {
            value.bind(&mut c)?;
        }
        for value in self.predicates.values() {
            value.bind(&mut c)?;
        }
        for value in self.slot_sources.values() {
            value.bind(&mut c)?;
        }
        for value in self.int_providers.values() {
            value.bind(&mut c)?;
        }
        for value in self.float_providers.values() {
            value.bind(&mut c)?;
        }
        Ok(c.finish())
    }
}
