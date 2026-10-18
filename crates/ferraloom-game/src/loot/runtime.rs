//! Per-invocation access to canonical facts, random streams and dynamic drops.
use super::{EnchantmentOperation, Identifier, LootError, LootFacts, LootParams, Result};
use ferraloom_nbt::Tag;
use ferraloom_protocol::chat::Component;
use ferraloom_protocol::component::{DataComponentMap, ItemStack};
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

    /// Port of ContextNbtProvider.BlockEntitySource#get. Live adapters read the
    /// retained context receiver at this call, even after removal or replacement.
    fn block_entity_nbt(&self, params: &LootParams) -> Result<Option<Tag>> {
        Ok(params.block_entity.clone())
    }

    /// Port of ComponentSource.BlockEntitySource#get. A live implementation
    /// collects components afresh from the same retained context receiver.
    fn block_entity_components(&self, params: &LootParams) -> Result<Option<DataComponentMap>> {
        Ok(params.block_entity_components.clone())
    }

    /// Port of CopyNameFunction's Nameable#getCustomName read. None identifies
    /// a non-Nameable receiver; Some with a null custom name removes the target
    /// name. Live adapters use the retained receiver's actual class and state.
    fn block_entity_display_name(&self, params: &LootParams) -> Result<Option<NameableName>> {
        if params.block_entity.is_none() && params.block_entity_components.is_none() {
            Ok(None)
        } else {
            Err(LootError::Unsupported("block_entity.custom_name".into()))
        }
    }

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

/// Port of a Nameable receiver's nullable getCustomName result.
#[derive(Clone, Debug, PartialEq)]
pub struct NameableName {
    pub custom_name: Option<Component>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loot::{Holder, LootData, LootItemCondition, LootItemFunction, LootTable};
    use ferraloom_data::{Block, Item};
    use ferraloom_nbt::CompoundTag;
    use ferraloom_protocol::{chat::Component, component::*};
    use ferraloom_util::random::LegacyRandomSource;
    use serde_json::json;
    use std::cell::RefCell;

    struct Facts;
    impl crate::predicates::PredicateContext for Facts {}
    impl LootFacts for Facts {
        fn item_enabled(&self, _stack: &ItemStack) -> Result<bool> {
            Ok(true)
        }
    }

    struct RetainedSource {
        random: LegacyRandomSource,
        phase: i32,
        reads: RefCell<Vec<&'static str>>,
        name: Option<NameableName>,
    }
    impl RetainedSource {
        fn new() -> Self {
            Self {
                random: LegacyRandomSource::new(42),
                phase: 0,
                reads: RefCell::new(Vec::new()),
                name: None,
            }
        }
    }
    impl LootRuntime for RetainedSource {
        fn facts(&self) -> &dyn LootFacts {
            &Facts
        }
        fn with_random(&mut self, operation: &mut dyn FnMut(&mut dyn RandomSource)) {
            operation(&mut self.random);
        }
        fn dynamic_drop_next(
            &mut self,
            _id: &Identifier,
            ordinal: usize,
            _params: &LootParams,
        ) -> Result<Option<ItemStack>> {
            if ordinal >= 2 {
                return Ok(None);
            }
            self.phase += 1;
            Ok(Some(ItemStack::new(Item::STONE, 1, Default::default())))
        }
        fn block_entity_nbt(&self, _params: &LootParams) -> Result<Option<Tag>> {
            self.reads.borrow_mut().push("nbt");
            let mut tag = CompoundTag::new();
            tag.insert("phase", Tag::Int(self.phase));
            Ok(Some(Tag::Compound(tag)))
        }
        fn block_entity_components(
            &self,
            _params: &LootParams,
        ) -> Result<Option<DataComponentMap>> {
            self.reads.borrow_mut().push("components");
            Ok(Some(DataComponentMap {
                entries: vec![DataComponent::CustomName(Box::new(Component::literal(
                    self.phase.to_string(),
                )))],
            }))
        }
        fn block_entity_display_name(&self, _params: &LootParams) -> Result<Option<NameableName>> {
            self.reads.borrow_mut().push("name");
            Ok(self.name.clone())
        }
    }

    #[test]
    fn dynamic_consumers_read_retained_block_entity_at_call_time() {
        let mut data = LootData::default();
        let id = Identifier::parse("test").unwrap();
        data.tables.insert(
            id.clone(),
            LootTable::decode(&json!({
                "pools":[{"rolls":1,"entries":[{"type":"dynamic","name":"contents",
                    "modifier":[
                        {"type":"copy_custom_data","source":"block_entity",
                         "ops":[{"source":"phase","target":"phase","op":"replace"}]},
                        {"type":"copy_components","source":"block_entity","include":["custom_name"]}
                    ]
                }]}]
            }))
            .unwrap(),
        );
        let params = LootParams {
            block_entity: Some(Tag::Compound(CompoundTag::new())),
            block_entity_components: Some(DataComponentMap::default()),
            ..Default::default()
        };
        let mut runtime = RetainedSource::new();
        let stacks = data
            .get_random_items_with_runtime(&id, &params, &mut runtime)
            .unwrap();
        assert_eq!(stacks.len(), 2);
        for (index, stack) in stacks.iter().enumerate() {
            let phase = index as i32 + 1;
            let expected = ItemStack::from_json(&json!({"id":"stone","count":1,"components":{
                "minecraft:custom_data":format!("{{phase:{phase}}}"),
                "minecraft:custom_name":phase.to_string()
            }}))
            .unwrap();
            assert_eq!(stack, &expected);
        }
        assert_eq!(
            *runtime.reads.borrow(),
            ["nbt", "components", "nbt", "components"]
        );
    }

    #[test]
    fn match_block_reads_nbt_before_components_and_short_circuits() {
        let data = LootData::default();
        let params = LootParams {
            block_state: Some(Block::CHEST.default_state()),
            block_entity: Some(Tag::Compound(CompoundTag::new())),
            block_entity_components: Some(DataComponentMap::default()),
            ..Default::default()
        };
        let mut runtime = RetainedSource::new();
        for (phase, expected, reads) in [
            (1, false, vec!["nbt"]),
            (0, true, vec!["nbt", "components"]),
        ] {
            let predicate = Holder::direct(
                LootItemCondition::decode(&json!({
                    "type":"match_block","nbt":format!("{{phase:{phase}}}"),
                    "components":{"minecraft:custom_name":"0"}
                }))
                .unwrap(),
            );
            runtime.reads.borrow_mut().clear();
            assert_eq!(
                data.test_predicate_with_runtime(&predicate, &params, &mut runtime)
                    .unwrap(),
                expected
            );
            assert_eq!(*runtime.reads.borrow(), reads);
        }
    }

    #[test]
    fn copy_name_distinguishes_non_nameable_null_and_present_custom_name() {
        let data = LootData::default();
        let modifier = Holder::direct(
            LootItemFunction::decode(&json!({
                "type":"copy_name","source":"block_entity"
            }))
            .unwrap(),
        );
        let original = Component::literal("target");
        let source = Component::literal("source");
        let stack = ItemStack::from_json(&json!({"id":"stone","components":{
            "minecraft:custom_name":"target"
        }}))
        .unwrap();
        let params = LootParams {
            block_entity: Some(Tag::Compound(CompoundTag::new())),
            ..Default::default()
        };
        let mut runtime = RetainedSource::new();
        for (name, expected) in [
            (None, Some(original)),
            (Some(NameableName { custom_name: None }), None),
            (
                Some(NameableName {
                    custom_name: Some(source.clone()),
                }),
                Some(source),
            ),
        ] {
            runtime.name = name;
            runtime.reads.borrow_mut().clear();
            let result = data
                .apply_modifier_with_runtime(&modifier, stack.clone(), &params, &mut runtime)
                .unwrap();
            let components = result.components().unwrap();
            let expected = expected.map(|name| DataComponent::CustomName(Box::new(name)));
            assert_eq!(
                components.get(ComponentTypeId::CUSTOM_NAME),
                expected.as_ref()
            );
            assert_eq!(*runtime.reads.borrow(), ["name"]);
        }
        runtime.reads.borrow_mut().clear();
        assert_eq!(
            data.apply_modifier_with_runtime(
                &modifier,
                stack.clone(),
                &LootParams::default(),
                &mut runtime
            )
            .unwrap(),
            stack
        );
        assert!(runtime.reads.borrow().is_empty());

        let mut random = LegacyRandomSource::new(42);
        assert_eq!(
            data.apply_modifier(
                &modifier,
                stack.clone(),
                &LootParams::default(),
                &mut random,
                &Facts
            )
            .unwrap(),
            stack
        );
        assert!(
            matches!(data.apply_modifier(&modifier, stack, &params, &mut random, &Facts),
            Err(LootError::Unsupported(fact)) if fact == "block_entity.custom_name")
        );
    }
}
