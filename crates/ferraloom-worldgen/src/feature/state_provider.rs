//! Ports of all ten 26.3 BlockStateProvider types. Providers are immutable;
//! random streams and context-dependent reads belong to FeatureContext.
use super::{FeatureContext, FeatureLoader};
use crate::{
    Error,
    codec::*,
    density::NoiseSpec,
    placement::{
        predicate::{BlockPredicate, direction},
        providers::{IntProvider, WeightedList},
    },
};
use ferraloom_data::{Block, BlockState, datapack::decode_block_state, properties};
use ferraloom_util::{
    Direction, mth,
    noise::{Noise, NoiseStack},
    random::{LegacyRandomSource, RandomSource, WorldgenRandom},
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct NoiseState {
    noise: NoiseStack,
    scale: f32,
}
impl NoiseState {
    fn decode(v: &Value) -> Result<(Self, i64), Error> {
        let seed = long(field(v, "seed")?)?;
        let scale = float(field(v, "scale")?)?;
        if scale <= 0.0 {
            return Err(Error("noise scale must be positive".into()));
        }
        let noise = NoiseSpec::decode(field(v, "noise")?)?
            .noise()
            .create(&mut WorldgenRandom::new(LegacyRandomSource::new(seed)));
        Ok((Self { noise, scale }, seed))
    }
    fn sample(&self, p: [i32; 3]) -> f32 {
        self.noise.get(
            f64::from(p[0]) * f64::from(self.scale),
            f64::from(p[1]) * f64::from(self.scale),
            f64::from(p[2]) * f64::from(self.scale),
        )
    }
}
#[derive(Clone, Debug)]
pub enum BlockStateProvider {
    Simple(BlockState),
    Weighted(WeightedList<BlockState>),
    RandomBlock(Vec<Block>),
    CopyProperties(Arc<Self>),
    RandomizedInt {
        source: Arc<Self>,
        property: String,
        values: IntProvider,
    },
    Rotated {
        source: Arc<Self>,
        direction: Option<Direction>,
    },
    RuleBased {
        fallback: Option<Arc<Self>>,
        rules: Vec<(BlockPredicate, Arc<Self>)>,
    },
    Noise {
        noise: NoiseState,
        states: Vec<BlockState>,
    },
    NoiseThreshold {
        noise: NoiseState,
        threshold: f32,
        high_chance: f32,
        default: BlockState,
        low: Vec<BlockState>,
        high: Vec<BlockState>,
    },
    DualNoise {
        noise: NoiseState,
        states: Vec<BlockState>,
        variety: [i32; 2],
        slow: NoiseStack,
        slow_scale: f32,
    },
}
fn states(v: &Value) -> Result<Vec<BlockState>, Error> {
    let result = list(v)?
        .iter()
        .map(|v| Ok(decode_block_state(v)?))
        .collect::<Result<Vec<_>, Error>>()?;
    if result.is_empty() {
        return Err(Error("state list is empty".into()));
    }
    Ok(result)
}
fn range_float(v: &Value, min: f32, max: f32) -> Result<f32, Error> {
    let n = float(v)?;
    if !(min..=max).contains(&n) {
        return Err(Error("float outside provider bounds".into()));
    }
    Ok(n)
}
fn select(states: &[BlockState], noise: f32) -> BlockState {
    let placement = mth::clamp_f32((1.0 + noise) / 2.0, 0.0, 0.9999);
    states[(placement * states.len() as f32) as usize]
}
impl BlockStateProvider {
    pub fn decode(v: &Value, loader: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        // Codec.xor tries both alternatives. Unknown/invalid typed fields do
        // not invalidate an otherwise valid full block state, and vice versa.
        let state = v.get("id").and_then(|_| decode_block_state(v).ok());
        let typed = Self::decode_typed(v, loader);
        match (state, typed) {
            (Some(_), Ok(_)) => Err(Error("state provider matches both exclusive codecs".into())),
            (Some(state), Err(_)) => Ok(Self::Simple(state)),
            (None, typed) => typed,
        }
    }
    fn decode_typed(v: &Value, loader: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        let ty = kind(v)?;
        Ok(match ty.to_string().as_str() {
            "minecraft:simple" => Self::Simple(decode_block_state(field(v, "state")?)?),
            "minecraft:weighted" => {
                Self::Weighted(WeightedList::decode(field(v, "entries")?, |v| {
                    Ok(decode_block_state(v)?)
                })?)
            }
            "minecraft:random_block" => Self::RandomBlock(loader.data.blocks(field(v, "blocks")?)?),
            "minecraft:copy_properties" => {
                Self::CopyProperties(loader.state_provider(field(v, "source")?)?)
            }
            "minecraft:randomized_int" => Self::RandomizedInt {
                source: loader.state_provider(field(v, "source")?)?,
                property: string(field(v, "property")?)?.into(),
                values: IntProvider::decode(field(v, "values")?)?,
            },
            "minecraft:rotated" => Self::Rotated {
                source: loader.state_provider(field(v, "state")?)?,
                direction: v.get("direction").map(direction).transpose()?,
            },
            "minecraft:rule_based" => {
                let fallback = v
                    .get("fallback")
                    .map(|v| loader.state_provider(v))
                    .transpose()?;
                let rules = list(field(v, "rules")?)?
                    .iter()
                    .map(|v| {
                        Ok((
                            BlockPredicate::decode(field(v, "if_true")?, &loader.data)?,
                            loader.state_provider(field(v, "then")?)?,
                        ))
                    })
                    .collect::<Result<_, Error>>()?;
                Self::RuleBased { fallback, rules }
            }
            "minecraft:noise" => Self::Noise {
                noise: NoiseState::decode(v)?.0,
                states: states(field(v, "states")?)?,
            },
            "minecraft:noise_threshold" => Self::NoiseThreshold {
                noise: NoiseState::decode(v)?.0,
                threshold: range_float(field(v, "threshold")?, -1.0, 1.0)?,
                high_chance: range_float(field(v, "high_chance")?, 0.0, 1.0)?,
                default: decode_block_state(field(v, "default_state")?)?,
                low: states(field(v, "low_states")?)?,
                high: states(field(v, "high_states")?)?,
            },
            "minecraft:dual_noise" => {
                let (noise, seed) = NoiseState::decode(v)?;
                let variety = field(v, "variety")?;
                let (min, max) = if variety.is_number() {
                    let n = int_range(variety, 1, 64)?;
                    (n, n)
                } else if variety.is_array() {
                    let values = list(variety)?;
                    if values.len() != 2 {
                        return Err(Error("variety requires two bounds".into()));
                    }
                    (int_range(&values[0], 1, 64)?, int_range(&values[1], 1, 64)?)
                } else {
                    (
                        int_range(field(variety, "min_inclusive")?, 1, 64)?,
                        int_range(field(variety, "max_inclusive")?, 1, 64)?,
                    )
                };
                if min > max {
                    return Err(Error("invalid variety range".into()));
                }
                let slow_scale = float(field(v, "slow_scale")?)?;
                if slow_scale <= 0.0 {
                    return Err(Error("slow scale must be positive".into()));
                }
                let slow = NoiseSpec::decode(field(v, "slow_noise")?)?
                    .noise()
                    .create(&mut WorldgenRandom::new(LegacyRandomSource::new(seed)));
                Self::DualNoise {
                    noise,
                    states: states(field(v, "states")?)?,
                    variety: [min, max],
                    slow,
                    slow_scale,
                }
            }
            _ => return Err(Error(format!("unknown state provider {ty}"))),
        })
    }
    pub fn get_state(
        &self,
        context: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
    ) -> Result<BlockState, Error> {
        match self.get_optional_state(context, p)? {
            Some(state) => Ok(state),
            None => context.region.block(p),
        }
    }
    pub fn get_optional_state(
        &self,
        context: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
    ) -> Result<Option<BlockState>, Error> {
        Ok(Some(match self {
            Self::Simple(state) => *state,
            Self::Weighted(list) => *list.sample(context.random),
            Self::RandomBlock(blocks) => {
                if blocks.is_empty() {
                    return Ok(None);
                }
                blocks[context.random.next_int_bound(blocks.len() as i32) as usize].default_state()
            }
            Self::CopyProperties(source) => source
                .get_state(context, p)?
                .with_properties_of(context.region.block(p)?),
            Self::RandomizedInt {
                source,
                property,
                values,
            } => {
                let state = source.get_state(context, p)?;
                if !state.has_integer_property(property) {
                    return Ok(Some(state));
                }
                state
                    .with_integer_property(property, values.sample(context.random))
                    .map_err(|e| Error(format!("{e:?}")))?
            }
            Self::Rotated { source, direction } => {
                let direction = direction.unwrap_or_else(|| {
                    Direction::VALUES[context.random.next_int_bound(6) as usize]
                });
                let mut state = source.get_state(context, p)?;
                let axis = match direction.axis() {
                    ferraloom_util::core::Axis::X => properties::Axis::X,
                    ferraloom_util::core::Axis::Y => properties::Axis::Y,
                    ferraloom_util::core::Axis::Z => properties::Axis::Z,
                };
                let facing = match direction {
                    Direction::Down => properties::Facing::Down,
                    Direction::Up => properties::Facing::Up,
                    Direction::North => properties::Facing::North,
                    Direction::South => properties::Facing::South,
                    Direction::West => properties::Facing::West,
                    Direction::East => properties::Facing::East,
                };
                state = state.with(properties::AXIS, axis).unwrap_or(state);
                state.with(properties::FACING, facing).unwrap_or(state)
            }
            Self::RuleBased { fallback, rules } => {
                for (predicate, then) in rules {
                    if predicate.test(context.region, p, context.generator)?
                        && let Some(state) = then.get_optional_state(context, p)?
                    {
                        return Ok(Some(state));
                    }
                }
                return fallback
                    .as_ref()
                    .map(|provider| provider.get_optional_state(context, p))
                    .unwrap_or(Ok(None));
            }
            Self::Noise { noise, states } => select(states, noise.sample(p)),
            Self::NoiseThreshold {
                noise,
                threshold,
                high_chance,
                default,
                low,
                high,
            } => {
                if noise.sample(p) < *threshold {
                    low[context.random.next_int_bound(low.len() as i32) as usize]
                } else if context.random.next_float() < *high_chance {
                    high[context.random.next_int_bound(high.len() as i32) as usize]
                } else {
                    *default
                }
            }
            Self::DualNoise {
                noise,
                states,
                variety,
                slow,
                slow_scale,
            } => {
                let sample = |p: [i32; 3]| {
                    slow.get(
                        f64::from(p[0] as f32 * slow_scale),
                        f64::from(p[1] as f32 * slow_scale),
                        f64::from(p[2] as f32 * slow_scale),
                    )
                };
                let count = mth::clamped_map(
                    f64::from(sample(p)),
                    -1.0,
                    1.0,
                    f64::from(variety[0]),
                    f64::from(variety[1] + 1),
                ) as i32;
                let possible = (0..count)
                    .map(|i| {
                        select(
                            states,
                            sample([
                                p[0].wrapping_add(i * 54545),
                                p[1],
                                p[2].wrapping_add(i * 34234),
                            ]),
                        )
                    })
                    .collect::<Vec<_>>();
                select(&possible, noise.sample(p))
            }
        }))
    }
}
