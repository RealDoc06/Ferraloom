//! The six native EnchantmentValueEffect implementations share LevelBasedValue.
use crate::loot::{LevelBasedValue, Result, codec};
use ferraloom_util::random::RandomSource;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum EnchantmentValueEffect {
    Add(LevelBasedValue),
    AllOf(Vec<Self>),
    Multiply(LevelBasedValue),
    RemoveBinomial(LevelBasedValue),
    Exponential {
        base: LevelBasedValue,
        exponent: LevelBasedValue,
    },
    Set(LevelBasedValue),
}
impl EnchantmentValueEffect {
    /// Port of EnchantmentValueEffect#CODEC, preserving the explicit all_of list.
    pub fn decode(value: &Value) -> Result<Self> {
        let based = |key| LevelBasedValue::decode(codec::field(value, key)?);
        Ok(match codec::kind(value)?.as_str() {
            "minecraft:add" => Self::Add(based("value")?),
            "minecraft:all_of" => {
                Self::AllOf(codec::list(codec::field(value, "effects")?, Self::decode)?)
            }
            "minecraft:multiply" => Self::Multiply(based("factor")?),
            "minecraft:remove_binomial" => Self::RemoveBinomial(based("chance")?),
            "minecraft:exponential" => Self::Exponential {
                base: based("base")?,
                exponent: based("exponent")?,
            },
            "minecraft:set" => Self::Set(based("value")?),
            other => {
                return Err(codec::error(format!(
                    "unknown enchantment value effect {other}"
                )));
            }
        })
    }
    /// Port of EnchantmentValueEffect#process. Arithmetic narrows at the same
    /// f32 boundaries as Java; chance zero still consumes the native draws.
    pub fn process<R: RandomSource + ?Sized>(
        &self,
        level: i32,
        random: &mut R,
        input: f32,
    ) -> Result<f32> {
        Ok(match self {
            Self::Add(value) => input + value.calculate(level)?,
            Self::AllOf(effects) => {
                let mut value = input;
                for effect in effects {
                    value = effect.process(level, random, value)?;
                }
                value
            }
            Self::Multiply(factor) => input * factor.calculate(level)?,
            Self::RemoveBinomial(chance) => {
                remove_binomial(input, chance.calculate(level)?, random)
            }
            Self::Exponential { base, exponent } => {
                (f64::from(input)
                    * f64::from(base.calculate(level)?).powf(f64::from(exponent.calculate(level)?)))
                    as f32
            }
            Self::Set(value) => value.calculate(level)?,
        })
    }
}

/// Port of RemoveBinomial#process. The approximation has native Gaussian-cache
/// behavior; the small-n path loops against the float (and therefore ceil(n)).
fn remove_binomial<R: RandomSource + ?Sized>(n: f32, chance: f32, random: &mut R) -> f32 {
    let drop = if n <= 128.0 || n * chance < 20.0 || n * (1.0 - chance) < 20.0 {
        let mut drop = 0_i32;
        let mut trial = 0_i32;
        while (trial as f32) < n {
            if random.next_float() < chance {
                drop = drop.wrapping_add(1);
            }
            trial = trial.wrapping_add(1);
        }
        drop
    } else {
        let mean = f64::from(n * chance).floor();
        let sigma = f64::from(n * chance * (1.0 - chance)).sqrt();
        // Math.round(double) returns a saturated long, then Java narrows that
        // long to int before Math.clamp. Rust's f64->i32 cast would lose this.
        let rounded = (mean + random.next_gaussian() * sigma + 0.5).floor() as i64;
        (rounded as i32).clamp(0, n as i32)
    };
    n - drop as f32
}

#[cfg(test)]
mod tests;
