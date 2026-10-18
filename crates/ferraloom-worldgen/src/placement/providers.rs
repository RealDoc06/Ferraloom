//! Ports of heightproviders and util.valueproviders, including codec validation.
use crate::{Error, codec::*, material::VerticalAnchor};
use ferraloom_util::{mth, random::RandomSource};
use serde_json::Value;

/// Port of WorldGenerationContext. Height is the minimum of the two depths,
/// independently of the maximum of the minimum Y coordinates.
#[derive(Clone, Copy, Debug)]
pub struct WorldGenerationContext {
    pub min_y: i32,
    pub height: i32,
    pub sea_level: i32,
}
impl WorldGenerationContext {
    pub fn new(
        generator_min_y: i32,
        generator_height: i32,
        level_min_y: i32,
        level_height: i32,
        sea_level: i32,
    ) -> Self {
        Self {
            min_y: generator_min_y.max(level_min_y),
            height: generator_height.min(level_height),
            sea_level,
        }
    }
    fn resolve(self, anchor: VerticalAnchor) -> i32 {
        anchor.resolve(self.min_y, self.height, self.sea_level)
    }
}

/// Port of WeightedList. Zero-weight entries remain in the reported range.
#[derive(Clone, Debug)]
pub struct WeightedList<T> {
    entries: Vec<(T, i32)>,
    total: i32,
}
impl<T> WeightedList<T> {
    pub fn entries(&self) -> &[(T, i32)] {
        &self.entries
    }
    pub fn decode(v: &Value, decode: impl Fn(&Value) -> Result<T, Error>) -> Result<Self, Error> {
        let mut entries = Vec::new();
        let mut total = 0i32;
        for entry in list(v)? {
            let weight = int_range(field(entry, "weight")?, 0, i32::MAX)?;
            total = total
                .checked_add(weight)
                .ok_or_else(|| Error("weight total exceeds i32::MAX".into()))?;
            entries.push((decode(field(entry, "data")?)?, weight));
        }
        if total == 0 {
            return Err(Error("weighted list has no nonzero weights".into()));
        }
        Ok(Self { entries, total })
    }
    pub fn sample(&self, random: &mut impl RandomSource) -> &T {
        let mut selection = random.next_int_bound(self.total);
        for (entry, weight) in &self.entries {
            selection -= weight;
            if selection < 0 {
                return entry;
            }
        }
        unreachable!("validated weight total")
    }
}

/// Ports of all eight 26.3 IntProviders.
#[derive(Clone, Debug)]
pub enum IntProvider {
    Constant(i32),
    Uniform(i32, i32),
    BiasedToBottom(i32, i32),
    VeryBiasedToBottom(i32, i32),
    Clamped {
        source: Box<Self>,
        min: i32,
        max: i32,
    },
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min: i32,
        max: i32,
    },
    WeightedList(WeightedList<Self>),
    Trapezoid {
        min: i32,
        max: i32,
        plateau: i32,
    },
}
impl IntProvider {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        if v.is_number() {
            return Ok(Self::Constant(integer(v)?));
        }
        let ty = kind(v)?;
        if ty.to_string() == "minecraft:constant" {
            return Ok(Self::Constant(integer(field(v, "value")?)?));
        }
        if ty.to_string() == "minecraft:weighted_list" {
            return Ok(Self::WeightedList(WeightedList::decode(
                field(v, "distribution")?,
                Self::decode,
            )?));
        }
        let trapezoid = ty.to_string() == "minecraft:trapezoid";
        let min = integer(field(v, if trapezoid { "min" } else { "min_inclusive" })?)?;
        let max = integer(field(v, if trapezoid { "max" } else { "max_inclusive" })?)?;
        if max < min {
            return Err(Error("provider maximum below minimum".into()));
        }
        Ok(match ty.to_string().as_str() {
            "minecraft:uniform" => Self::Uniform(min, max),
            "minecraft:biased_to_bottom" => Self::BiasedToBottom(min, max),
            "minecraft:very_biased_to_bottom" => Self::VeryBiasedToBottom(min, max),
            "minecraft:clamped" => Self::Clamped {
                source: Box::new(Self::decode(field(v, "source")?)?),
                min,
                max,
            },
            "minecraft:clamped_normal" => Self::ClampedNormal {
                mean: float(field(v, "mean")?)?,
                deviation: float(field(v, "deviation")?)?,
                min,
                max,
            },
            "minecraft:trapezoid" => {
                let plateau = integer(field(v, "plateau")?)?;
                if plateau > max.wrapping_sub(min) {
                    return Err(Error("plateau exceeds range".into()));
                }
                Self::Trapezoid { min, max, plateau }
            }
            _ => return Err(Error(format!("unknown int provider {ty}"))),
        })
    }
    pub fn decode_bounded(v: &Value, min: i32, max: i32) -> Result<Self, Error> {
        let provider = Self::decode(v)?;
        let (lo, hi) = provider.range();
        if lo < min || hi > max {
            return Err(Error("int provider outside required range".into()));
        }
        Ok(provider)
    }
    pub fn range(&self) -> (i32, i32) {
        match self {
            Self::Constant(n) => (*n, *n),
            Self::Uniform(min, max)
            | Self::BiasedToBottom(min, max)
            | Self::VeryBiasedToBottom(min, max)
            | Self::ClampedNormal { min, max, .. }
            | Self::Trapezoid { min, max, .. } => (*min, *max),
            Self::Clamped { source, min, max } => {
                let (lo, hi) = source.range();
                ((*min).max(lo), (*max).min(hi))
            }
            Self::WeightedList(list) => list
                .entries
                .iter()
                .map(|(p, _)| p.range())
                .fold((i32::MAX, i32::MIN), |(min, max), (lo, hi)| {
                    (min.min(lo), max.max(hi))
                }),
        }
    }
    pub fn sample(&self, random: &mut impl RandomSource) -> i32 {
        match self {
            Self::Constant(n) => *n,
            Self::Uniform(min, max) => mth::random_between_inclusive(random, *min, *max),
            Self::BiasedToBottom(min, max) => {
                let n = random.next_int_bound(max.wrapping_sub(*min).wrapping_add(1));
                min.wrapping_add(random.next_int_bound(n + 1))
            }
            Self::VeryBiasedToBottom(min, max) => {
                let n = random.next_int_bound(max.wrapping_sub(*min).wrapping_add(1));
                let n = random.next_int_bound(n + 1);
                min.wrapping_add(random.next_int_bound(n + 1))
            }
            Self::Clamped { source, min, max } => mth::clamp_int(source.sample(random), *min, *max),
            Self::ClampedNormal {
                mean,
                deviation,
                min,
                max,
            } => mth::clamp_f32(
                mth::normal(random, *mean, *deviation),
                *min as f32,
                *max as f32,
            ) as i32,
            Self::WeightedList(list) => list.sample(random).sample(random),
            Self::Trapezoid { min, max, plateau } => {
                if *plateau == 0 && *max == min.wrapping_neg() {
                    return random
                        .next_int_bound(max.wrapping_add(1))
                        .wrapping_sub(random.next_int_bound(max.wrapping_add(1)));
                }
                let range = max.wrapping_sub(*min);
                if *plateau == range {
                    return mth::random_between_inclusive(random, *min, *max);
                }
                let start = range.wrapping_sub(*plateau) / 2;
                let end = range.wrapping_sub(start);
                min.wrapping_add(mth::random_between_inclusive(random, 0, end))
                    .wrapping_add(mth::random_between_inclusive(random, 0, start))
            }
        }
    }
}

/// Ports of all four FloatProviders. All arithmetic retains Java float rounding.
#[derive(Clone, Debug)]
pub enum FloatProvider {
    Constant(f32),
    Uniform(f32, f32),
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min: f32,
        max: f32,
    },
    Trapezoid {
        min: f32,
        max: f32,
        plateau: f32,
    },
}
impl FloatProvider {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        if v.is_number() {
            return Ok(Self::Constant(float(v)?));
        }
        let ty = kind(v)?;
        let p = match ty.to_string().as_str() {
            "minecraft:constant" => Self::Constant(float(field(v, "value")?)?),
            "minecraft:uniform" => {
                let min = float(field(v, "min_inclusive")?)?;
                let max = float(field(v, "max_exclusive")?)?;
                if max <= min {
                    return Err(Error("uniform float maximum must exceed minimum".into()));
                }
                Self::Uniform(min, max)
            }
            "minecraft:clamped_normal" => Self::ClampedNormal {
                mean: float(field(v, "mean")?)?,
                deviation: float(field(v, "deviation")?)?,
                min: float(field(v, "min")?)?,
                max: float(field(v, "max")?)?,
            },
            "minecraft:trapezoid" => {
                let min = float(field(v, "min")?)?;
                let max = float(field(v, "max")?)?;
                let plateau = float(field(v, "plateau")?)?;
                if plateau > max - min {
                    return Err(Error("plateau exceeds range".into()));
                }
                Self::Trapezoid { min, max, plateau }
            }
            _ => return Err(Error(format!("unknown float provider {ty}"))),
        };
        let (min, max) = p.range();
        if max < min {
            return Err(Error("float provider maximum below minimum".into()));
        }
        Ok(p)
    }
    pub fn decode_bounded(v: &Value, min: f32, max: f32) -> Result<Self, Error> {
        let provider = Self::decode(v)?;
        let (lo, hi) = provider.range();
        if lo < min || hi > max {
            return Err(Error("float provider outside required range".into()));
        }
        Ok(provider)
    }
    pub fn range(&self) -> (f32, f32) {
        match self {
            Self::Constant(n) => (*n, *n),
            Self::Uniform(min, max)
            | Self::ClampedNormal { min, max, .. }
            | Self::Trapezoid { min, max, .. } => (*min, *max),
        }
    }
    pub fn sample(&self, random: &mut impl RandomSource) -> f32 {
        match self {
            Self::Constant(n) => *n,
            Self::Uniform(min, max) => mth::random_between(random, *min, *max),
            Self::ClampedNormal {
                mean,
                deviation,
                min,
                max,
            } => mth::clamp_f32(mth::normal(random, *mean, *deviation), *min, *max),
            Self::Trapezoid { min, max, plateau } => {
                let range = max - min;
                let start = (range - plateau) / 2.0;
                let end = range - start;
                min + random.next_float() * end + random.next_float() * start
            }
        }
    }
}

/// Port of HeightProvider and all six registered implementations.
#[derive(Clone, Debug)]
pub enum HeightProvider {
    Constant(VerticalAnchor),
    Uniform(VerticalAnchor, VerticalAnchor),
    BiasedToBottom {
        min: VerticalAnchor,
        max: VerticalAnchor,
        inner: i32,
    },
    VeryBiasedToBottom {
        min: VerticalAnchor,
        max: VerticalAnchor,
        inner: i32,
    },
    Trapezoid {
        min: VerticalAnchor,
        max: VerticalAnchor,
        plateau: i32,
    },
    WeightedList(WeightedList<Self>),
}
impl HeightProvider {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        if let Ok(anchor) = VerticalAnchor::decode(v) {
            return Ok(Self::Constant(anchor));
        }
        let ty = kind(v)?;
        if ty.to_string() == "minecraft:constant" {
            return Ok(Self::Constant(VerticalAnchor::decode(field(v, "value")?)?));
        }
        if ty.to_string() == "minecraft:weighted_list" {
            return Ok(Self::WeightedList(WeightedList::decode(
                field(v, "distribution")?,
                Self::decode,
            )?));
        }
        let min = VerticalAnchor::decode(field(v, "min_inclusive")?)?;
        let max = VerticalAnchor::decode(field(v, "max_inclusive")?)?;
        Ok(match ty.to_string().as_str() {
            "minecraft:uniform" => Self::Uniform(min, max),
            "minecraft:biased_to_bottom" => Self::BiasedToBottom {
                min,
                max,
                inner: optional(v, "inner", 1, |v| int_range(v, 1, i32::MAX))?,
            },
            "minecraft:very_biased_to_bottom" => Self::VeryBiasedToBottom {
                min,
                max,
                inner: optional(v, "inner", 1, |v| int_range(v, 1, i32::MAX))?,
            },
            "minecraft:trapezoid" => Self::Trapezoid {
                min,
                max,
                plateau: optional(v, "plateau", 0, integer)?,
            },
            _ => return Err(Error(format!("unknown height provider {ty}"))),
        })
    }
    pub fn sample(&self, random: &mut impl RandomSource, context: WorldGenerationContext) -> i32 {
        match self {
            Self::Constant(anchor) => context.resolve(*anchor),
            Self::WeightedList(list) => list.sample(random).sample(random, context),
            Self::Uniform(min, max) => {
                let (min, max) = (context.resolve(*min), context.resolve(*max));
                if min > max {
                    min
                } else {
                    mth::random_between_inclusive(random, min, max)
                }
            }
            Self::BiasedToBottom { min, max, inner }
            | Self::VeryBiasedToBottom { min, max, inner } => {
                let (min, max) = (context.resolve(*min), context.resolve(*max));
                let range = max.wrapping_sub(min).wrapping_sub(*inner).wrapping_add(1);
                if range <= 0 {
                    return min;
                }
                if matches!(self, Self::BiasedToBottom { .. }) {
                    let limit = random.next_int_bound(range);
                    min.wrapping_add(random.next_int_bound(limit.wrapping_add(*inner)))
                } else {
                    let upper = mth::next_int(random, min.wrapping_add(*inner), max);
                    let biased = mth::next_int(random, min, upper.wrapping_sub(1));
                    mth::next_int(random, min, biased.wrapping_sub(1).wrapping_add(*inner))
                }
            }
            Self::Trapezoid { min, max, plateau } => {
                let (min, max) = (context.resolve(*min), context.resolve(*max));
                if min > max {
                    return min;
                }
                let range = max.wrapping_sub(min);
                if *plateau >= range {
                    return mth::random_between_inclusive(random, min, max);
                }
                let start = range.wrapping_sub(*plateau) / 2;
                let end = range.wrapping_sub(start);
                min.wrapping_add(mth::random_between_inclusive(random, 0, end))
                    .wrapping_add(mth::random_between_inclusive(random, 0, start))
            }
        }
    }
}
