use super::function::{max_stack_size, stack_count};
use super::{ContextIntProvider, EvalContext, Holder, Identifier, LootData, Result, codec};
use crate::loot::codec::{DecodeInput, PersistentInput};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::{HolderSet, VarInt},
    component::*,
};
use ferraloom_util::random::RandomSource;

/// Port of ExplorationMapFunction's codec. Its live search and saved map
/// allocation are implemented by the level adapter at the original call site.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplorationMapOptions {
    pub destination: HolderSet,
    pub decoration: Identifier,
    pub zoom: i8,
    pub search_radius: i32,
    pub skip_existing_chunks: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum WorldFunction {
    FurnaceSmelt {
        use_input_count: bool,
    },
    ExplorationMap(ExplorationMapOptions),
    SetEnchantments {
        enchantments: Vec<(Identifier, Holder<ContextIntProvider>)>,
        add: bool,
    },
    SetStewEffect(Vec<(MobEffect, Holder<ContextIntProvider>)>),
}
impl WorldFunction {
    pub(crate) fn decode<I: DecodeInput>(kind: &str, v: &I) -> Result<Option<Self>> {
        Ok(Some(match kind {
            "minecraft:furnace_smelt" => Self::FurnaceSmelt {
                use_input_count: codec::optional(v, "use_input_count", codec::boolean)?
                    .unwrap_or(true),
            },
            "minecraft:exploration_map" => Self::ExplorationMap(ExplorationMapOptions {
                destination: HolderSet::from_input(codec::field(v, "destination")?)?,
                decoration: codec::optional(v, "decoration", codec::id)?
                    .unwrap_or(Identifier::parse("woodland_mansion")?),
                zoom: codec::optional(v, "zoom", codec::int)?.unwrap_or(2) as i8,
                search_radius: codec::optional(v, "search_radius", codec::int)?.unwrap_or(50),
                skip_existing_chunks: codec::optional(v, "skip_existing_chunks", codec::boolean)?
                    .unwrap_or(true),
            }),
            "minecraft:set_enchantments" => Self::SetEnchantments {
                enchantments: codec::optional(v, "enchantments", |v| {
                    let mut keys = std::collections::BTreeSet::new();
                    codec::map(v)?
                        .entries()?
                        .into_iter()
                        .map(|(key, value)| {
                            let id = Identifier::parse(key)?;
                            if !keys.insert(id.clone()) {
                                return Err(codec::error("duplicate enchantment"));
                            }
                            Ok((id, Holder::decode(value, ContextIntProvider::decode)?))
                        })
                        .collect::<Result<_>>()
                })?
                .unwrap_or_default(),
                add: codec::optional(v, "add", codec::boolean)?.unwrap_or(false),
            },
            "minecraft:set_stew_effect" => {
                let effects = codec::optional(v, "effects", |v| {
                    codec::list(v, |v| {
                        Ok((
                            MobEffect::from_input(codec::field(v, "type")?)?,
                            Holder::decode(
                                codec::field(v, "duration")?,
                                ContextIntProvider::decode,
                            )?,
                        ))
                    })
                })?
                .unwrap_or_default();
                let mut keys = std::collections::BTreeSet::new();
                for (effect, _) in &effects {
                    if !keys.insert(effect.0.clone()) {
                        return Err(codec::error("duplicate stew effect"));
                    }
                }
                Self::SetStewEffect(effects)
            }
            _ => return Ok(None),
        }))
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        match self {
            Self::FurnaceSmelt { use_input_count } => {
                if !stack.is_empty()
                    && let Some(mut result) = c.facts().smelting_recipe_result(&stack)?
                    && !result.is_empty()
                {
                    let count = if *use_input_count {
                        stack_count(&stack)
                    } else {
                        1
                    };
                    let count = count
                        .wrapping_mul(stack_count(&result))
                        .min(max_stack_size(&result)?);
                    result.0.as_mut().expect("nonempty").count = count;
                    return Ok(result);
                }
            }
            Self::ExplorationMap(options) => {
                if !stack.is_empty()
                    && let Some(origin) = c.params.origin
                {
                    return c.facts().exploration_map(options, stack, origin);
                }
            }
            Self::SetEnchantments { enchantments, add } => {
                if !stack.is_empty() && stack.0.as_ref().is_some_and(|s| s.item == Item::BOOK) {
                    let s = stack.0.take().expect("nonempty");
                    stack = ItemStack::new(Item::ENCHANTED_BOOK, s.count, s.components);
                }
                let stored = stack
                    .0
                    .as_ref()
                    .is_some_and(|s| s.item == Item::ENCHANTED_BOOK);
                let component = if stored {
                    ComponentTypeId::STORED_ENCHANTMENTS
                } else {
                    ComponentTypeId::ENCHANTMENTS
                };
                let map = stack.components()?;
                let Some(
                    DataComponent::Enchantments(value) | DataComponent::StoredEnchantments(value),
                ) = map.get(component)
                else {
                    return Ok(stack);
                };
                let mut value = value.clone();
                let keys = enchantments
                    .iter()
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                // Literal levels commute and consume no RNG. Otherwise vanilla's
                // Map.copyOf iteration needs the running JVM's captured order.
                let ordered = if enchantments.iter().all(|(_,value)|matches!(value,Holder::Inline(value) if matches!(**value,ContextIntProvider::Constant(_)))) {
                    keys
                } else {
                    c.facts().enchantment_provider_order(&keys)?
                };
                for key in ordered {
                    let provider = &enchantments
                        .iter()
                        .find(|(id, _)| *id == key)
                        .expect("adapter returns provider keys")
                        .1;
                    let level = provider.get(data, c)?;
                    let existing = value.0.iter().position(|(id, _)| id.0 == key);
                    let old = existing.map_or(0, |i| value.0[i].1.0);
                    let level = if *add { old.wrapping_add(level) } else { level }.clamp(0, 255);
                    if let Some(index) = existing {
                        if level == 0 {
                            value.0.remove(index);
                        } else {
                            value.0[index].1 = VarInt(level);
                        }
                    } else if level > 0 {
                        value.0.push((Enchantment(key), VarInt(level)));
                    }
                }
                if let Some(s) = &mut stack.0 {
                    s.components.set(if stored {
                        DataComponent::StoredEnchantments(value)
                    } else {
                        DataComponent::Enchantments(value)
                    });
                }
            }
            Self::SetStewEffect(effects) => {
                if stack.is_empty()
                    || stack
                        .0
                        .as_ref()
                        .is_none_or(|s| s.item != Item::SUSPICIOUS_STEW)
                    || effects.is_empty()
                {
                    return Ok(stack);
                }
                let (effect, duration) =
                    &effects[c.random().next_int_bound(effects.len() as i32) as usize];
                let mut duration = duration.get(data, c)?;
                // MobEffects' only InstantaneousMobEffect subclasses in 26.3.
                if !matches!(
                    effect.0.as_str(),
                    "minecraft:instant_health"
                        | "minecraft:instant_damage"
                        | "minecraft:saturation"
                ) {
                    duration = duration.wrapping_mul(20);
                }
                let mut value = match stack
                    .components()?
                    .get(ComponentTypeId::SUSPICIOUS_STEW_EFFECTS)
                {
                    Some(DataComponent::SuspiciousStewEffects(value)) => value.clone(),
                    _ => Vec::new(),
                };
                value.push(StewEffect {
                    effect: effect.clone(),
                    duration: VarInt(duration),
                });
                if let Some(s) = &mut stack.0 {
                    s.components
                        .set(DataComponent::SuspiciousStewEffects(value));
                }
            }
        }
        Ok(stack)
    }
}
