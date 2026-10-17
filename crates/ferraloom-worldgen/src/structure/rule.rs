//! Structure template rule predicates. Evaluation preserves short-circuit order
//! because even predicates that fail can advance the per-block random stream.
use crate::{Error, Identifier, codec::*};
use ferraloom_data::{
    Block, BlockState,
    datapack::{ResourceManager, decode_block_state, load_tags},
};
use ferraloom_util::{mth, random::RandomSource};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct BlockTags(pub BTreeMap<Identifier, Vec<Block>>);
impl BlockTags {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let tags = load_tags(resources, "block", |id, _| {
            Block::from_name(&id.to_string())
        });
        if !tags.diagnostics.is_empty() {
            return Err(Error(format!("invalid block tags: {:?}", tags.diagnostics)));
        }
        Ok(Self(tags.values))
    }
    pub fn contains(&self, tag: &Identifier, block: Block) -> bool {
        self.0.get(tag).is_some_and(|v| v.contains(&block))
    }
}
/// Port of all ten RuleTest types registered in vanilla 26.3.
#[derive(Clone, Debug)]
pub enum RuleTest {
    AlwaysTrue,
    BlockMatch(Block),
    BlockStateMatch(BlockState),
    TagMatch(Identifier),
    HeightMatch { min: i32, max: i32 },
    RandomBlockMatch { block: Block, probability: f32 },
    RandomBlockStateMatch { state: BlockState, probability: f32 },
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
    Not(Box<Self>),
}
fn block(v: &Value) -> Result<Block, Error> {
    let name = id(v)?;
    Block::from_name(&name.to_string()).ok_or_else(|| Error(format!("unknown block {name}")))
}
impl RuleTest {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let ty = id(field(v, "predicate_type")?)?;
        let state = |key| decode_block_state(field(v, key)?).map_err(Error::from);
        match ty.to_string().as_str() {
            "minecraft:always_true" => Ok(Self::AlwaysTrue),
            "minecraft:block_match" => Ok(Self::BlockMatch(block(field(v, "block")?)?)),
            "minecraft:blockstate_match" => Ok(Self::BlockStateMatch(state("block_state")?)),
            "minecraft:tag_match" => Ok(Self::TagMatch(id(field(v, "tag")?)?)),
            "minecraft:height_match" => Ok(Self::HeightMatch {
                min: integer(field(v, "min_inclusive")?)?,
                max: integer(field(v, "max_inclusive")?)?,
            }),
            "minecraft:random_block_match" => Ok(Self::RandomBlockMatch {
                block: block(field(v, "block")?)?,
                probability: float(field(v, "probability")?)?,
            }),
            "minecraft:random_blockstate_match" => Ok(Self::RandomBlockStateMatch {
                state: state("block_state")?,
                probability: float(field(v, "probability")?)?,
            }),
            "minecraft:all_of" => Ok(Self::AllOf(
                list(field(v, "rules")?)?
                    .iter()
                    .map(Self::decode)
                    .collect::<Result<_, _>>()?,
            )),
            "minecraft:any_of" => Ok(Self::AnyOf(
                list(field(v, "rules")?)?
                    .iter()
                    .map(Self::decode)
                    .collect::<Result<_, _>>()?,
            )),
            "minecraft:not" => Ok(Self::Not(Box::new(Self::decode(field(v, "rule")?)?))),
            _ => Err(Error(format!("unknown rule test {ty}"))),
        }
    }
    pub fn test(
        &self,
        state: BlockState,
        position: [i32; 3],
        tags: &BlockTags,
        random: &mut impl RandomSource,
    ) -> bool {
        match self {
            Self::AlwaysTrue => true,
            Self::BlockMatch(b) => state.block() == *b,
            Self::BlockStateMatch(s) => state == *s,
            Self::TagMatch(tag) => tags.contains(tag, state.block()),
            Self::HeightMatch { min, max } => position[1] >= *min && position[1] <= *max,
            Self::RandomBlockMatch { block, probability } => {
                state.block() == *block && random.next_float() < *probability
            }
            Self::RandomBlockStateMatch {
                state: s,
                probability,
            } => state == *s && random.next_float() < *probability,
            Self::AllOf(rules) => rules.iter().all(|r| r.test(state, position, tags, random)),
            Self::AnyOf(rules) => rules.iter().any(|r| r.test(state, position, tags, random)),
            Self::Not(rule) => !rule.test(state, position, tags, random),
        }
    }
}
/// Port of PosRuleTest and its three registered types.
#[derive(Clone, Debug)]
pub enum PosRuleTest {
    AlwaysTrue,
    Linear {
        min_chance: f32,
        max_chance: f32,
        min_dist: i32,
        max_dist: i32,
    },
    AxisAlignedLinear {
        min_chance: f32,
        max_chance: f32,
        min_dist: i32,
        max_dist: i32,
        axis: usize,
    },
}
impl PosRuleTest {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let ty = id(field(v, "predicate_type")?)?;
        if ty.to_string() == "minecraft:always_true" {
            return Ok(Self::AlwaysTrue);
        }
        let min_chance = optional(v, "min_chance", 0.0, float)?;
        let max_chance = optional(v, "max_chance", 0.0, float)?;
        let min_dist = optional(v, "min_dist", 0, integer)?;
        let max_dist = optional(v, "max_dist", 0, integer)?;
        if min_dist >= max_dist {
            return Err(Error(format!("Invalid range: [{min_dist},{max_dist}]")));
        }
        match ty.to_string().as_str() {
            "minecraft:linear_pos" => Ok(Self::Linear {
                min_chance,
                max_chance,
                min_dist,
                max_dist,
            }),
            "minecraft:axis_aligned_linear_pos" => {
                let axis = optional(v, "axis", 1, |v| match string(v)? {
                    "x" => Ok(0),
                    "y" => Ok(1),
                    "z" => Ok(2),
                    s => Err(Error(format!("unknown axis {s}"))),
                })?;
                Ok(Self::AxisAlignedLinear {
                    min_chance,
                    max_chance,
                    min_dist,
                    max_dist,
                    axis,
                })
            }
            _ => Err(Error(format!("unknown positional rule test {ty}"))),
        }
    }
    pub fn test(
        &self,
        world: [i32; 3],
        reference: [i32; 3],
        random: &mut impl RandomSource,
    ) -> bool {
        let (min_chance, max_chance, min_dist, max_dist, dist) = match *self {
            Self::AlwaysTrue => return true,
            Self::Linear {
                min_chance,
                max_chance,
                min_dist,
                max_dist,
            } => {
                // Vec3i#distManhattan performs int subtraction/abs, then
                // float addition and a saturating int conversion.
                let delta = std::array::from_fn::<_, 3, _>(|i| {
                    reference[i].wrapping_sub(world[i]).wrapping_abs() as f32
                });
                (
                    min_chance,
                    max_chance,
                    min_dist,
                    max_dist,
                    (delta[0] + delta[1] + delta[2]) as i32,
                )
            }
            Self::AxisAlignedLinear {
                min_chance,
                max_chance,
                min_dist,
                max_dist,
                axis,
            } => {
                let delta = world[axis].wrapping_sub(reference[axis]).wrapping_abs() as f32;
                (min_chance, max_chance, min_dist, max_dist, delta as i32)
            }
        };
        let draw = random.next_float();
        draw <= mth::clamped_lerp_f32(
            mth::inverse_lerp_f32(dist as f32, min_dist as f32, max_dist as f32),
            min_chance,
            max_chance,
        )
    }
}

#[cfg(test)]
#[path = "rule_tests.rs"]
mod tests;
