use super::function::{max_stack_size, stack_count};
use super::{
    ContextFloatProvider, ContextIntProvider, ContextKeySet, EvalContext, Holder, Identifier,
    LootData, LootFacts, LootItemCondition, LootItemFunction, LootParams, LootPoolEntryContainer,
    LootRandomSelection, LootRuntime, RandomSequences, Result, codec, runtime::BorrowedRuntime,
};
use crate::loot::codec::DecodeInput;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{
    mth,
    random::{LegacyRandomSource, RandomSource},
};
use std::collections::BTreeSet;
pub(crate) type Output<'a> = dyn FnMut(ItemStack, &mut EvalContext<'_>) -> Result<()> + 'a;

/// Port of LootTable#DIRECT_CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct LootTable {
    pub param_set: Identifier,
    pub random_sequence: Option<Identifier>,
    pub pools: Vec<LootPool>,
    pub modifier: Option<Holder<LootItemFunction>>,
}
/// Port of LootPool#CODEC and LootPool#addRandomItems.
#[derive(Clone, Debug, PartialEq)]
pub struct LootPool {
    pub entries: Vec<LootPoolEntryContainer>,
    pub condition: Option<Holder<LootItemCondition>>,
    pub modifier: Option<Holder<LootItemFunction>>,
    pub rolls: Holder<ContextIntProvider>,
    pub bonus_rolls: Holder<ContextFloatProvider>,
}
impl LootTable {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        // `type` alone is lenientOptionalFieldOf: invalid/unknown values default.
        let param_set = codec::optional(v, "type", codec::id)
            .ok()
            .flatten()
            .filter(|id| ContextKeySet::vanilla(id).is_some())
            .unwrap_or(Identifier::parse("generic")?);
        Ok(Self {
            param_set,
            random_sequence: codec::optional(v, "random_sequence", codec::id)?,
            pools: codec::optional(v, "pools", |v| codec::list(v, LootPool::decode))?
                .unwrap_or_default(),
            modifier: codec::optional(v, "modifier", |v| {
                Holder::decode(v, LootItemFunction::decode)
            })?,
        })
    }
    pub(crate) fn raw(
        &self,
        data: &LootData,
        c: &mut EvalContext<'_>,
        output: &mut Output<'_>,
    ) -> Result<()> {
        let mut decorated = |stack: ItemStack, c: &mut EvalContext<'_>| {
            let stack = if let Some(modifier) = &self.modifier {
                modifier.apply(stack, data, c)?
            } else {
                stack
            };
            output(stack, c)
        };
        for pool in &self.pools {
            pool.add_random_items(data, c, &mut decorated)?;
        }
        Ok(())
    }
}
impl Holder<LootTable> {
    pub(crate) fn raw(
        &self,
        data: &LootData,
        c: &mut EvalContext<'_>,
        output: &mut Output<'_>,
    ) -> Result<()> {
        if let Self::Reference(id) = self {
            let key = ("loot_table", id.clone());
            if !c.visited.insert(key.clone()) {
                return Ok(());
            }
            let result = self.resolve(data)?.raw(data, c, output);
            c.visited.remove(&key);
            result
        } else {
            self.resolve(data)?.raw(data, c, output)
        }
    }
}
impl LootPool {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            entries: codec::list(codec::field(v, "entries")?, LootPoolEntryContainer::decode)?,
            condition: codec::optional(v, "condition", |v| {
                Holder::decode(v, LootItemCondition::decode)
            })?,
            modifier: codec::optional(v, "modifier", |v| {
                Holder::decode(v, LootItemFunction::decode)
            })?,
            rolls: Holder::decode(codec::field(v, "rolls")?, ContextIntProvider::decode)?,
            bonus_rolls: codec::optional(v, "bonus_rolls", |v| {
                Holder::decode(v, ContextFloatProvider::decode)
            })?
            .unwrap_or_else(|| Holder::direct(ContextFloatProvider::Constant(0.0))),
        })
    }
    fn add_random_items(
        &self,
        data: &LootData,
        c: &mut EvalContext<'_>,
        output: &mut Output<'_>,
    ) -> Result<()> {
        if let Some(condition) = &self.condition
            && !condition.test(data, c)?
        {
            return Ok(());
        }
        let count = self.rolls.get(data, c)?.wrapping_add(mth::floor(f64::from(
            self.bonus_rolls.get(data, c)? * c.params.luck,
        )));
        let mut decorated = |stack: ItemStack, c: &mut EvalContext<'_>| {
            let stack = if let Some(modifier) = &self.modifier {
                modifier.apply(stack, data, c)?
            } else {
                stack
            };
            output(stack, c)
        };
        for _ in 0..count {
            let mut entries = Vec::new();
            for entry in &self.entries {
                entry.expand(data, c, &mut entries)?;
            }
            entries.retain(|e| e.weight > 0);
            let total = entries
                .iter()
                .fold(0_i32, |sum, e| sum.wrapping_add(e.weight));
            if total == 0 || entries.is_empty() {
                continue;
            }
            if entries.len() == 1 {
                entries[0].create(data, c, &mut decorated)?;
            } else {
                if total < 0 {
                    return Err(codec::error("pool total weight overflow"));
                }
                let mut index = c.random().next_int_bound(total);
                for entry in entries {
                    index = index.wrapping_sub(entry.weight);
                    if index < 0 {
                        entry.create(data, c, &mut decorated)?;
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
impl LootData {
    pub fn evaluate_int(
        &self,
        provider: &Holder<ContextIntProvider>,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<i32> {
        self.evaluate_int_with_runtime(provider, params, &mut BorrowedRuntime::new(random, facts))
    }
    pub fn evaluate_int_with_runtime(
        &self,
        provider: &Holder<ContextIntProvider>,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<i32> {
        provider.get(
            self,
            &mut EvalContext {
                params,
                runtime,
                visited: BTreeSet::new(),
            },
        )
    }
    pub fn evaluate_float(
        &self,
        provider: &Holder<ContextFloatProvider>,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<f32> {
        self.evaluate_float_with_runtime(provider, params, &mut BorrowedRuntime::new(random, facts))
    }
    pub fn evaluate_float_with_runtime(
        &self,
        provider: &Holder<ContextFloatProvider>,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<f32> {
        provider.get(
            self,
            &mut EvalContext {
                params,
                runtime,
                visited: BTreeSet::new(),
            },
        )
    }
    /// Evaluation API for `/loot`; use the caller's validated parameter set and RNG.
    pub fn get_random_items(
        &self,
        table: &Identifier,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<Vec<ItemStack>> {
        self.get_random_items_with_runtime(table, params, &mut BorrowedRuntime::new(random, facts))
    }
    /// Evaluate with an invocation that owns canonical live access and stream identity.
    pub fn get_random_items_with_runtime(
        &self,
        table: &Identifier,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<Vec<ItemStack>> {
        let mut output = Vec::new();
        let mut context = EvalContext {
            params,
            runtime,
            visited: BTreeSet::new(),
        };
        Holder::Reference(table.clone()).raw(self, &mut context, &mut |stack, c| {
            if c.facts().item_enabled(&stack)? {
                split_stack(stack, &mut output)?;
            }
            Ok(())
        })?;
        Ok(output)
    }
    /// Port of LootTable#getRandomItems(LootParams), including inline holders.
    /// The table's own random_sequence chooses a stream, independently of its key.
    pub fn get_random_items_from_holder(
        &self,
        table: &Holder<LootTable>,
        params: &LootParams,
        sources: &mut LootRandomSources<'_>,
        facts: &dyn LootFacts,
    ) -> Result<Vec<ItemStack>> {
        let evaluate = |random: &mut dyn RandomSource| {
            self.get_random_items_from_holder_with_runtime(
                table,
                params,
                &mut BorrowedRuntime::new(random, facts),
            )
        };
        if let Some(sequence) = &table.resolve(self)?.random_sequence {
            let mut random = sources.sequences.get(sequence, sources.world_seed);
            evaluate(&mut random)
        } else {
            evaluate(sources.level_random)
        }
    }
    /// Evaluate a holder with the caller's selected canonical runtime, as for
    /// get_random_items_with_runtime. Select the table value's random_sequence
    /// before constructing the runtime; reference keys do not choose a stream.
    pub fn get_random_items_from_holder_with_runtime(
        &self,
        table: &Holder<LootTable>,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<Vec<ItemStack>> {
        let mut output = Vec::new();
        let mut context = EvalContext {
            params,
            runtime,
            visited: BTreeSet::new(),
        };
        table.raw(self, &mut context, &mut |stack, c| {
            if c.facts().item_enabled(&stack)? {
                split_stack(stack, &mut output)?;
            }
            Ok(())
        })?;
        Ok(output)
    }
    /// Port of LootContext.Builder's seed precedence. Zero leaves the table's
    /// named sequence (or level random) selected; a nonzero seed is legacy RNG.
    pub fn get_random_items_seeded(
        &self,
        table: &Identifier,
        params: &LootParams,
        seed: i64,
        sources: &mut LootRandomSources<'_>,
        facts: &dyn LootFacts,
    ) -> Result<Vec<ItemStack>> {
        match self.random_selection(table, seed)? {
            LootRandomSelection::Seeded(seed) => {
                self.get_random_items(table, params, &mut LegacyRandomSource::new(seed), facts)
            }
            LootRandomSelection::Sequence(id) => {
                let mut random = sources.sequences.get(&id, sources.world_seed);
                self.get_random_items(table, params, &mut random, facts)
            }
            LootRandomSelection::Level => {
                self.get_random_items(table, params, sources.level_random, facts)
            }
        }
    }
    /// Select once for the invocation. Live adapters retain this identity and
    /// borrow its canonical stream only inside LootRuntime::with_random.
    pub fn random_selection(&self, table: &Identifier, seed: i64) -> Result<LootRandomSelection> {
        if seed != 0 {
            return Ok(LootRandomSelection::Seeded(seed));
        }
        let value = self
            .tables
            .get(table)
            .ok_or_else(|| super::LootError::MissingReference {
                registry: "loot_table",
                id: table.clone(),
            })?;
        Ok(value
            .random_sequence
            .as_ref()
            .map_or(LootRandomSelection::Level, |id| {
                LootRandomSelection::Sequence(id.clone())
            }))
    }

    pub fn test_predicate(
        &self,
        predicate: &Holder<LootItemCondition>,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<bool> {
        self.test_predicate_with_runtime(
            predicate,
            params,
            &mut BorrowedRuntime::new(random, facts),
        )
    }
    pub fn test_predicate_with_runtime(
        &self,
        predicate: &Holder<LootItemCondition>,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<bool> {
        predicate.test(
            self,
            &mut EvalContext {
                params,
                runtime,
                visited: BTreeSet::new(),
            },
        )
    }

    /// Item modifier registry API for `/item modify`, preserving sequential RNG use.
    pub fn apply_modifier(
        &self,
        modifier: &Holder<LootItemFunction>,
        stack: ItemStack,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<ItemStack> {
        self.apply_modifier_with_runtime(
            modifier,
            stack,
            params,
            &mut BorrowedRuntime::new(random, facts),
        )
    }
    pub fn apply_modifier_with_runtime(
        &self,
        modifier: &Holder<LootItemFunction>,
        stack: ItemStack,
        params: &LootParams,
        runtime: &mut dyn LootRuntime,
    ) -> Result<ItemStack> {
        modifier.apply(
            stack,
            self,
            &mut EvalContext {
                params,
                runtime,
                visited: BTreeSet::new(),
            },
        )
    }

    /// Port of LootTable#fill. Existing slots stay occupied; an overfull result
    /// stops insertion at vanilla's capture point after all shuffling draws.
    pub fn fill(
        &self,
        table: &Identifier,
        container: &mut [ItemStack],
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
    ) -> Result<()> {
        let placements = self.fill_placements(table, params, random, facts, || {
            Ok(container
                .iter()
                .enumerate()
                .filter_map(|(slot, stack)| stack.is_empty().then_some(slot))
                .collect())
        })?;
        for (slot, stack) in placements {
            container[slot] = stack;
        }
        Ok(())
    }
    /// Port of LootTable#fill up to its Container#setItem calls. The callback
    /// reads canonical empty slots in ascending order after generation and its
    /// feature checks. All shuffling draws finish before placements are returned.
    /// Deliver each placement through the original container handle, including
    /// its synchronous callbacks, before delivering the next placement.
    pub fn fill_placements(
        &self,
        table: &Identifier,
        params: &LootParams,
        random: &mut dyn RandomSource,
        facts: &dyn LootFacts,
        available_slots: impl FnOnce() -> Result<Vec<usize>>,
    ) -> Result<Vec<(usize, ItemStack)>> {
        self.fill_placements_with_runtime(
            table,
            params,
            &mut BorrowedRuntime::new(random, facts),
            |_| available_slots(),
        )
    }
    /// The availability callback receives the concrete live adapter after all
    /// generation/feature checks, and before any slot shuffling.
    pub fn fill_placements_with_runtime<R: LootRuntime>(
        &self,
        table: &Identifier,
        params: &LootParams,
        runtime: &mut R,
        available_slots: impl FnOnce(&mut R) -> Result<Vec<usize>>,
    ) -> Result<Vec<(usize, ItemStack)>> {
        let mut items = self.get_random_items_with_runtime(table, params, runtime)?;
        let mut slots = available_slots(runtime)?;
        let mut random = super::runtime::RuntimeRandom(runtime);
        shuffle(&mut slots, &mut random);
        shuffle_and_split_items(&mut items, slots.len(), &mut random);
        let mut placements = Vec::with_capacity(items.len().min(slots.len()));
        for item in items {
            let Some(slot) = slots.pop() else {
                break;
            };
            placements.push((
                slot,
                if item.is_empty() {
                    ItemStack::default()
                } else {
                    item
                },
            ));
        }
        Ok(placements)
    }

    /// Port of LootTable#fill's optional seed and named sequence selection.
    pub fn fill_seeded(
        &self,
        table: &Identifier,
        container: &mut [ItemStack],
        params: &LootParams,
        seed: i64,
        sources: &mut LootRandomSources<'_>,
        facts: &dyn LootFacts,
    ) -> Result<()> {
        let placements =
            self.fill_seeded_placements(table, params, seed, sources, facts, || {
                Ok(container
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, stack)| stack.is_empty().then_some(slot))
                    .collect())
            })?;
        for (slot, stack) in placements {
            container[slot] = stack;
        }
        Ok(())
    }
    /// Seed selection for fill_placements, sharing generation and shuffling on
    /// one stream. The caller delivers the returned placements in their order.
    pub fn fill_seeded_placements(
        &self,
        table: &Identifier,
        params: &LootParams,
        seed: i64,
        sources: &mut LootRandomSources<'_>,
        facts: &dyn LootFacts,
        available_slots: impl FnOnce() -> Result<Vec<usize>>,
    ) -> Result<Vec<(usize, ItemStack)>> {
        match self.random_selection(table, seed)? {
            LootRandomSelection::Seeded(seed) => self.fill_placements(
                table,
                params,
                &mut LegacyRandomSource::new(seed),
                facts,
                available_slots,
            ),
            LootRandomSelection::Sequence(id) => {
                let mut random = sources.sequences.get(&id, sources.world_seed);
                self.fill_placements(table, params, &mut random, facts, available_slots)
            }
            LootRandomSelection::Level => {
                self.fill_placements(table, params, sources.level_random, facts, available_slots)
            }
        }
    }
}

pub(crate) fn split_stack(stack: ItemStack, output: &mut Vec<ItemStack>) -> Result<()> {
    let max = max_stack_size(&stack)?;
    let mut count = stack_count(&stack);
    if count < max {
        output.push(stack);
        return Ok(());
    }
    if max <= 0 {
        return Err(codec::error("nonpositive maximum stack size"));
    }
    while count > 0 {
        let mut copy = stack.clone();
        let amount = max.min(count);
        copy.0.as_mut().expect("nonempty").count = amount;
        count -= amount;
        output.push(copy);
    }
    Ok(())
}
fn shuffle<T>(values: &mut [T], random: &mut dyn RandomSource) {
    for size in (2..=values.len()).rev() {
        values.swap(size - 1, random.next_int_bound(size as i32) as usize);
    }
}
fn next_int(random: &mut dyn RandomSource, min: i32, max: i32) -> i32 {
    if min >= max {
        min
    } else {
        random.next_int_bound(max - min + 1) + min
    }
}
/// Port of LootTable#shuffleAndSplitItems (remove preserves insertion order).
fn shuffle_and_split_items(
    result: &mut Vec<ItemStack>,
    available: usize,
    random: &mut dyn RandomSource,
) {
    let mut splittable = Vec::new();
    result.retain(|s| {
        if s.is_empty() {
            false
        } else if stack_count(s) > 1 {
            splittable.push(s.clone());
            false
        } else {
            true
        }
    });
    while available > result.len() + splittable.len() && !splittable.is_empty() {
        let index = next_int(random, 0, splittable.len() as i32 - 1) as usize;
        let mut stack = splittable.remove(index);
        let remove = next_int(random, 1, stack_count(&stack) / 2);
        let mut copy = stack.clone();
        copy.0.as_mut().expect("nonempty").count = remove;
        stack.0.as_mut().expect("nonempty").count -= remove;
        if stack_count(&stack) > 1 && random.next_boolean() {
            splittable.push(stack);
        } else {
            result.push(stack);
        }
        if stack_count(&copy) > 1 && random.next_boolean() {
            splittable.push(copy);
        } else {
            result.push(copy);
        }
    }
    result.extend(splittable);
    shuffle(result, random);
}

/// Caller-owned RNG sources used by LootContext.Builder's optional seed selection.
pub struct LootRandomSources<'a> {
    pub world_seed: i64,
    pub sequences: &'a mut RandomSequences,
    pub level_random: &'a mut dyn RandomSource,
}
