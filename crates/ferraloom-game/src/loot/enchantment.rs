use super::{Result, codec};
use crate::loot::codec::DecodeInput;

/// Port of LevelBasedValue, shared by enchanted loot probabilities and counts.
#[derive(Clone, Debug, PartialEq)]
pub enum LevelBasedValue {
    Constant(f32),
    Clamped {
        value: Box<Self>,
        min: f32,
        max: f32,
    },
    Fraction(Box<Self>, Box<Self>),
    LevelsSquared(f32),
    Linear {
        base: f32,
        per_level_above_first: f32,
    },
    Exponent(Box<Self>, Box<Self>),
    Lookup {
        values: Vec<f32>,
        fallback: Box<Self>,
    },
}
impl LevelBasedValue {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_number() {
            return Ok(Self::Constant(codec::float(v)?));
        }
        let nested =
            |name| -> Result<Box<Self>> { Ok(Box::new(Self::decode(codec::field(v, name)?)?)) };
        Ok(match codec::kind(v)?.as_str() {
            "minecraft:clamped" => {
                let min = codec::float(codec::field(v, "min")?)?;
                let max = codec::float(codec::field(v, "max")?)?;
                if max <= min {
                    return Err(codec::error("max must be larger than min"));
                }
                Self::Clamped {
                    value: nested("value")?,
                    min,
                    max,
                }
            }
            "minecraft:fraction" => Self::Fraction(nested("numerator")?, nested("denominator")?),
            "minecraft:levels_squared" => {
                Self::LevelsSquared(codec::float(codec::field(v, "added")?)?)
            }
            "minecraft:linear" => Self::Linear {
                base: codec::float(codec::field(v, "base")?)?,
                per_level_above_first: codec::float(codec::field(v, "per_level_above_first")?)?,
            },
            "minecraft:exponent" => Self::Exponent(nested("base")?, nested("power")?),
            "minecraft:lookup" => Self::Lookup {
                values: codec::list(codec::field(v, "values")?, codec::float)?,
                fallback: nested("fallback")?,
            },
            _ => return Err(codec::error("unknown level based value type")),
        })
    }
    pub fn calculate(&self, level: i32) -> Result<f32> {
        Ok(match self {
            Self::Constant(value) => *value,
            Self::Clamped { value, min, max } => value.calculate(level)?.clamp(*min, *max),
            Self::Fraction(numerator, denominator) => {
                let denominator = denominator.calculate(level)?;
                if denominator == 0.0 {
                    0.0
                } else {
                    numerator.calculate(level)? / denominator
                }
            }
            Self::LevelsSquared(added) => level.wrapping_mul(level) as f32 + added,
            Self::Linear {
                base,
                per_level_above_first,
            } => base + per_level_above_first * level.wrapping_sub(1) as f32,
            Self::Exponent(base, power) => {
                f64::from(base.calculate(level)?).powf(f64::from(power.calculate(level)?)) as f32
            }
            Self::Lookup { values, fallback } => {
                if level <= values.len() as i32 {
                    *values
                        .get(level.wrapping_sub(1) as usize)
                        .ok_or_else(|| codec::error("level based lookup index out of bounds"))?
                } else {
                    fallback.calculate(level)?
                }
            }
        })
    }
}
/// Enchantment operations whose registry rules belong to the enchantment port.
/// The hook receives the exact live RNG at vanilla's call site.
#[derive(Clone, Debug, PartialEq)]
pub enum EnchantmentOperation {
    Randomly {
        options: Option<ferraloom_protocol::codec::HolderSet>,
        only_compatible: bool,
        include_additional_cost_component: bool,
    },
    WithLevels {
        levels: i32,
        options: Option<ferraloom_protocol::codec::HolderSet>,
        include_additional_cost_component: bool,
    },
}
