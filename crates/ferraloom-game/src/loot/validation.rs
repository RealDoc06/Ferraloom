use super::{
    ContextKeySet, ContextParam, EntryKind, FunctionKind, Holder, HolderSet, Identifier, LootData,
    LootItemCondition, LootItemFunction, LootPoolEntryContainer, LootTable,
};
use super::{
    condition::ProviderRange,
    number::{NumberProvider, ScoreboardNameProvider},
    registry::RegistryValue,
};
use std::collections::BTreeSet;

/// Port of ValidationContext problems. Recursive references are fatal; parameter
/// usage and minimum-size problems are nonfatal warnings during registry reload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationProblem {
    pub path: String,
    pub message: String,
    pub fatal: bool,
}
pub struct ValidationContext<'a> {
    data: &'a LootData,
    params: ContextKeySet,
    path: String,
    visited: BTreeSet<(&'static str, Identifier)>,
    pub problems: Vec<ValidationProblem>,
}
trait Validatable {
    fn validate(&self, c: &mut ValidationContext<'_>);
}
impl ValidationContext<'_> {
    fn problem(&mut self, message: impl Into<String>, fatal: bool) {
        self.problems.push(ValidationProblem {
            path: self.path.clone(),
            message: message.into(),
            fatal,
        });
    }
    fn field(&mut self, name: &str, visit: impl FnOnce(&mut Self)) {
        let old = self.path.clone();
        self.path.push('.');
        self.path.push_str(name);
        visit(self);
        self.path = old;
    }
    fn params(&mut self, params: impl IntoIterator<Item = ContextParam>) {
        let unavailable: Vec<_> = params
            .into_iter()
            .filter(|p| !self.params.allowed.contains(p))
            .collect();
        if !unavailable.is_empty() {
            self.problem(
                format!("parameters {unavailable:?} are not provided in this context"),
                false,
            );
        }
    }
    fn holder<T: RegistryValue + Validatable>(&mut self, name: &str, holder: &Holder<T>) {
        self.field(name, |c| {
            let key = match holder {
                Holder::Reference(id) => Some((T::REGISTRY, id.clone())),
                _ => None,
            };
            if let Some(key) = &key {
                if !c.visited.insert(key.clone()) {
                    c.problem("recursive element reference", true);
                    return;
                }
            }
            match holder.resolve(c.data) {
                Ok(value) => value.validate(c),
                Err(e) => c.problem(e.to_string(), true),
            }
            if let Some(key) = key {
                c.visited.remove(&key);
            }
        });
    }
    fn optional<T: RegistryValue + Validatable>(&mut self, name: &str, holder: &Option<Holder<T>>) {
        if let Some(holder) = holder {
            self.holder(name, holder);
        }
    }
    fn set<T: RegistryValue + Validatable + Clone>(
        &mut self,
        name: &str,
        set: &HolderSet<T>,
        minimum: usize,
    ) {
        self.field(name, |c| {
            let key = match set {
                HolderSet::Tag(id) => Some((T::REGISTRY, id.clone())),
                _ => None,
            };
            // Tags and elements have distinct identity, even with matching ids.
            let tag_key = key.as_ref().map(|(registry, id)| {
                (
                    *registry,
                    Identifier::parse(&format!("{}:tags/{}", id.namespace(), id.path()))
                        .expect("valid tag path"),
                )
            });
            if let Some(key) = &tag_key {
                if !c.visited.insert(key.clone()) {
                    c.problem("recursive tag reference", true);
                    return;
                }
            }
            match set.resolve(c.data) {
                Ok(holders) => {
                    if holders.len() < minimum {
                        c.problem(
                            format!("list must contain at least {minimum} element(s)"),
                            false,
                        );
                    }
                    for (i, holder) in holders.iter().enumerate() {
                        c.holder(&format!("[{i}]"), holder);
                    }
                }
                Err(e) => c.problem(e.to_string(), true),
            }
            if let Some(key) = tag_key {
                c.visited.remove(&key);
            }
        });
    }
    fn range<T: RegistryValue + Validatable>(&mut self, test: &ProviderRange<T>) {
        match test {
            ProviderRange::Point(p) => self.holder("value", p),
            ProviderRange::Line { min, max } => {
                self.optional("min", min);
                self.optional("max", max);
            }
        }
    }
}
impl LootData {
    pub fn validate(&self) -> Vec<ValidationProblem> {
        let mut problems = Vec::new();
        macro_rules! validate_registry {
            ($field:ident,$registry:literal,$params:expr) => {
                for (id, value) in &self.$field {
                    let mut c = ValidationContext {
                        data: self,
                        params: $params(value),
                        path: format!("{}:{id}", $registry),
                        visited: BTreeSet::from([($registry, id.clone())]),
                        problems: Vec::new(),
                    };
                    value.validate(&mut c);
                    problems.extend(c.problems);
                }
            };
        }
        validate_registry!(tables, "loot_table", |v: &LootTable| {
            ContextKeySet::vanilla(&v.param_set).expect("decoded parameter set")
        });
        fn generic<T>(_: &T) -> ContextKeySet {
            ContextKeySet::vanilla(&Identifier::parse("generic").expect("vanilla id"))
                .expect("generic context set")
        }
        validate_registry!(predicates, "predicate", generic);
        validate_registry!(modifiers, "item_modifier", generic);
        validate_registry!(int_providers, "context_int_provider", generic);
        validate_registry!(float_providers, "context_float_provider", generic);
        problems
    }
}
impl Validatable for LootTable {
    fn validate(&self, c: &mut ValidationContext<'_>) {
        for (i, pool) in self.pools.iter().enumerate() {
            c.field(&format!("pools[{i}]"), |c| {
                c.optional("condition", &pool.condition);
                c.optional("modifier", &pool.modifier);
                c.holder("rolls", &pool.rolls);
                c.holder("bonus_rolls", &pool.bonus_rolls);
                for (j, entry) in pool.entries.iter().enumerate() {
                    c.field(&format!("entries[{j}]"), |c| entry.validate(c));
                }
            });
        }
        c.optional("modifier", &self.modifier);
    }
}
impl Validatable for LootPoolEntryContainer {
    fn validate(&self, c: &mut ValidationContext<'_>) {
        c.optional("condition", &self.condition);
        c.optional("modifier", &self.modifier);
        match &self.kind {
            EntryKind::Table { tables, .. } => c.set("value", tables, 0),
            EntryKind::Alternatives(children)
            | EntryKind::Sequence(children)
            | EntryKind::Group(children) => {
                if children.is_empty() {
                    c.problem("empty children list", false);
                }
                for (i, child) in children.iter().enumerate() {
                    c.field(&format!("children[{i}]"), |c| child.validate(c));
                }
            }
            _ => {}
        }
    }
}
impl Validatable for LootItemFunction {
    fn validate(&self, c: &mut ValidationContext<'_>) {
        c.optional("condition", &self.condition);
        match &self.kind {
            FunctionKind::Sequence(functions) => c.set("functions", functions, 0),
            FunctionKind::SetCount { count, .. } => c.holder("count", count),
            FunctionKind::ApplyBonus { .. } => c.params([ContextParam::Tool]),
            FunctionKind::EnchantedCountIncrease { count, .. } => {
                c.params([ContextParam::AttackingEntity]);
                c.holder("count", count);
            }
            FunctionKind::EnchantWithLevels {
                levels,
                include_additional_cost_component,
                ..
            } => {
                c.holder("levels", levels);
                if *include_additional_cost_component {
                    c.params([ContextParam::AdditionalCostComponentAllowed]);
                }
            }
            FunctionKind::EnchantRandomly {
                include_additional_cost_component,
                ..
            } => {
                if *include_additional_cost_component {
                    c.params([ContextParam::AdditionalCostComponentAllowed]);
                }
            }
            FunctionKind::SetDamage { damage, .. } => c.holder("damage", damage),
            FunctionKind::LimitCount { min, max } => {
                c.optional("min", min);
                c.optional("max", max);
            }
            FunctionKind::SetOminousBottleAmplifier(p) => c.holder("amplifier", p),
            FunctionKind::Filtered {
                on_pass, on_fail, ..
            } => {
                c.optional("on_pass", on_pass);
                c.optional("on_fail", on_fail);
            }
            _ => {}
        }
    }
}
impl Validatable for LootItemCondition {
    fn validate(&self, c: &mut ValidationContext<'_>) {
        c.params(self.params());
        match self {
            Self::Inverted(term) => c.holder("term", term),
            Self::AllOf(terms) | Self::AnyOf(terms) => c.set("terms", terms, 0),
            Self::RandomChance(chance) => c.holder("chance", chance),
            Self::IntValueCheck(value, test) => {
                c.holder("value", value);
                c.range(test);
            }
            Self::FloatValueCheck(value, test) => {
                c.holder("value", value);
                c.range(test);
            }
            Self::EntityScores(_, scores) => {
                for (name, test) in scores {
                    c.field(name, |c| c.range(test));
                }
            }
            _ => {}
        }
    }
}
impl<T: Copy + Clone> Validatable for NumberProvider<T>
where
    Self: RegistryValue,
{
    fn validate(&self, c: &mut ValidationContext<'_>) {
        match self {
            Self::Constant(_) => {}
            Self::EnchantmentLevel(_) => c.params([ContextParam::EnchantmentLevel]),
            Self::Unary(_, p) => c.holder("input", p),
            Self::Binary(_, left, right) | Self::Uniform(left, right) => {
                c.holder("left", left);
                c.holder("right", right);
            }
            Self::Aggregate(_, inputs) => c.set("inputs", inputs, 1),
            Self::Conditional(test, a, b) => {
                c.holder("condition", test);
                c.holder("on_true", a);
                c.holder("on_false", b);
            }
            Self::Dispatcher(cases, default) => {
                for (test, value) in cases {
                    c.holder("condition", test);
                    c.holder("value", value);
                }
                c.holder("default", default);
            }
            Self::Weighted(values) => {
                for (value, _) in values {
                    c.holder("distribution", value);
                }
            }
            Self::FromInt(value) => c.holder("input", value),
            Self::FromFloat(value) => c.holder("input", value),
            Self::Binomial(n, p) => {
                c.holder("n", n);
                c.holder("p", p);
            }
            Self::Storage(_, _, fallback) => c.holder("fallback", fallback),
            Self::Score(target, _, fallback) => {
                if let ScoreboardNameProvider::Context(target) = target {
                    c.params([target.param()]);
                }
                c.holder("fallback", fallback);
            }
        }
    }
}
