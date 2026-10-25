use super::function::stack_count;
use super::{ContextFloatProvider, EvalContext, Holder, Identifier, LootData, Result, codec};
use crate::loot::codec::DecodeInput;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent, ItemStack};
use ferraloom_util::random::RandomSource;

/// Port of ApplyBonusCount's three formula codecs and evaluation.
#[derive(Clone, Debug, PartialEq)]
pub enum BonusFormula {
    OreDrops,
    UniformBonusCount(i32),
    BinomialWithBonusCount { extra: i32, probability: f32 },
}
impl BonusFormula {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(match codec::id(codec::field(v, "formula")?)?.as_str() {
            "minecraft:ore_drops" => Self::OreDrops,
            "minecraft:uniform_bonus_count" => Self::UniformBonusCount(codec::int(codec::field(
                codec::field(v, "parameters")?,
                "bonusMultiplier",
            )?)?),
            "minecraft:binomial_with_bonus_count" => {
                let p = codec::field(v, "parameters")?;
                Self::BinomialWithBonusCount {
                    extra: codec::int(codec::field(p, "extra")?)?,
                    probability: codec::float(codec::field(p, "probability")?)?,
                }
            }
            _ => return Err(codec::error("unknown bonus formula")),
        })
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        enchantment: &Identifier,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        let Some(tool) = &c.params.tool else {
            return Ok(stack);
        };
        let level = item_enchantment_level(tool, enchantment)?;
        let count = stack_count(&stack);
        let count = match self {
            Self::OreDrops => {
                if level > 0 {
                    let bonus = (c.random().next_int_bound(level.wrapping_add(2)) - 1).max(0);
                    count.wrapping_mul(bonus + 1)
                } else {
                    count
                }
            }
            Self::UniformBonusCount(multiplier) => {
                let bound = multiplier.wrapping_mul(level).wrapping_add(1);
                if bound <= 0 {
                    return Err(codec::error("nonpositive bonus bound"));
                }
                count.wrapping_add(c.random().next_int_bound(bound))
            }
            Self::BinomialWithBonusCount { extra, probability } => {
                let mut result = count;
                for _ in 0..level.wrapping_add(*extra) {
                    if c.random().next_float() < *probability {
                        result = result.wrapping_add(1);
                    }
                }
                result
            }
        };
        if let Some(s) = &mut stack.0 {
            s.count = count;
        }
        Ok(stack)
    }
}
pub(crate) fn item_enchantment_level(stack: &ItemStack, enchantment: &Identifier) -> Result<i32> {
    Ok(
        match stack.components()?.get(ComponentTypeId::ENCHANTMENTS) {
            Some(DataComponent::Enchantments(value)) => value
                .0
                .iter()
                .find(|(id, _)| id.0 == *enchantment)
                .map_or(0, |(_, v)| v.0),
            _ => 0,
        },
    )
}
pub(crate) fn enchanted_count_increase(
    mut stack: ItemStack,
    enchantment: &Identifier,
    count: &Holder<ContextFloatProvider>,
    limit: i32,
    data: &LootData,
    c: &mut EvalContext<'_>,
) -> Result<ItemStack> {
    let Some(entity) = c.params.attacking_entity else {
        return Ok(stack);
    };
    let level = c.facts().entity_enchantment_level(entity, enchantment)?;
    if level == 0 {
        return Ok(stack);
    }
    let addition = level as f32 * count.get(data, c)?;
    let addition = (f64::from(addition) + 0.5).floor() as i32;
    let value = stack_count(&stack).wrapping_add(addition);
    if let Some(s) = &mut stack.0 {
        s.count = if limit > 0 { value.min(limit) } else { value };
    }
    Ok(stack)
}
