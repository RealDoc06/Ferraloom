//! Per-invocation access to canonical facts, random streams and dynamic drops.
use super::{EnchantmentOperation, Identifier, LootError, LootFacts, LootParams, Result};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::random::{PositionalRandomFactory, Random, RandomSource};

/// Port of LootContext's selected random source and LootParams.DynamicDrop.
/// Implementations may own a mutable Level view. Each method ends its world
/// borrow before the evaluator applies the emitted stack's consumer.
pub trait LootRuntime {
    fn facts(&self) -> &dyn LootFacts;

    /// Invoke the callback exactly once with the selected canonical stream.
    /// The stream must be the same on successive calls, including its Gaussian
    /// cache. No source borrow survives the callback.
    fn with_random(&mut self, operation: &mut dyn FnMut(&mut dyn RandomSource));

    /// Read one dynamic drop lazily. Ordinals start at zero for each expansion;
    /// its consumer completes before the next ordinal is requested. Some(empty)
    /// still emits an empty stack. None ends the provider or names no provider.
    /// Registered providers whose facts are unavailable return Unsupported.
    fn dynamic_drop_next(
        &mut self,
        _id: &Identifier,
        _ordinal: usize,
        _params: &LootParams,
    ) -> Result<Option<ItemStack>> {
        Ok(None)
    }

    /// The enchantment hook needs facts and the selected stream together. Live
    /// adapters implement it with short borrows of their canonical services.
    fn enchant_item(
        &mut self,
        _operation: &EnchantmentOperation,
        _stack: ItemStack,
        _params: &LootParams,
    ) -> Result<ItemStack> {
        Err(LootError::Unsupported(
            "enchantment.selection_and_application".into(),
        ))
    }
}

pub(crate) struct BorrowedRuntime<'a> {
    facts: &'a dyn LootFacts,
    random: &'a mut dyn RandomSource,
    dynamic_drops: Vec<std::vec::IntoIter<ItemStack>>,
}
impl<'a> BorrowedRuntime<'a> {
    pub fn new(random: &'a mut dyn RandomSource, facts: &'a dyn LootFacts) -> Self {
        Self {
            facts,
            random,
            dynamic_drops: Vec::new(),
        }
    }
}
impl LootRuntime for BorrowedRuntime<'_> {
    fn facts(&self) -> &dyn LootFacts {
        self.facts
    }
    fn with_random(&mut self, operation: &mut dyn FnMut(&mut dyn RandomSource)) {
        operation(self.random);
    }
    fn dynamic_drop_next(
        &mut self,
        id: &Identifier,
        ordinal: usize,
        _params: &LootParams,
    ) -> Result<Option<ItemStack>> {
        if ordinal == 0 {
            self.dynamic_drops
                .push(self.facts.dynamic_drops(id)?.into_iter());
        }
        let stack = self
            .dynamic_drops
            .last_mut()
            .expect("dynamic drop starts at zero")
            .next();
        if stack.is_none() {
            self.dynamic_drops.pop();
        }
        Ok(stack)
    }
    fn enchant_item(
        &mut self,
        operation: &EnchantmentOperation,
        stack: ItemStack,
        params: &LootParams,
    ) -> Result<ItemStack> {
        self.facts
            .enchant_item(operation, stack, params, self.random)
    }
}

/// Delegates each whole RandomSource operation without implementing another RNG.
pub(crate) struct RuntimeRandom<'a>(pub &'a mut dyn LootRuntime);
impl RuntimeRandom<'_> {
    fn call<T>(&mut self, operation: impl FnOnce(&mut dyn RandomSource) -> T) -> T {
        let mut operation = Some(operation);
        let mut result = None;
        self.0.with_random(&mut |random| {
            result = Some(operation.take().expect("with_random called twice")(random));
        });
        result.expect("with_random did not call its operation")
    }
}
macro_rules! delegate_random {
    ($($name:ident($($arg:ident: $ty:ty),*) $(-> $result:ty)?;)*) => {$ (
        fn $name(&mut self, $($arg: $ty),*) $(-> $result)? {
            self.call(|random| random.$name($($arg),*))
        }
    )*};
}
impl RandomSource for RuntimeRandom<'_> {
    delegate_random! {
        set_seed(seed: i64);
        next_bits(bits: u32) -> i32;
        next_gaussian() -> f64;
        fork() -> Random;
        fork_positional() -> PositionalRandomFactory;
        legacy_next_bits(bits: u32) -> Option<i32>;
        next_int() -> i32;
        next_int_bound(bound: i32) -> i32;
        next_long() -> i64;
        next_boolean() -> bool;
        next_float() -> f32;
        next_double() -> f64;
        next_int_between_inclusive(min: i32, max: i32) -> i32;
        next_int_between(origin: i32, bound: i32) -> i32;
        triangle(mean: f64, spread: f64) -> f64;
        triangle_f32(mean: f32, spread: f32) -> f32;
        consume_count(rounds: i32);
    }
}

/// Port of LootContext.Builder's optional seed and random-sequence selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LootRandomSelection {
    Level,
    Sequence(Identifier),
    Seeded(i64),
}
