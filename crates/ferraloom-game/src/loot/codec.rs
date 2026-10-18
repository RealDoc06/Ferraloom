use super::{Identifier, LootError, Result};
use serde_json::Value;

pub(crate) fn object(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().ok_or_else(|| error("expected object"))
}
pub(crate) fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    object(v)?
        .get(key)
        .ok_or_else(|| error(format!("missing {key}")))
}
pub(crate) fn optional<'a, T>(
    v: &'a Value,
    key: &str,
    decode: impl FnOnce(&'a Value) -> Result<T>,
) -> Result<Option<T>> {
    object(v)?.get(key).map(decode).transpose()
}
pub(crate) fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| error("expected string"))
}
pub(crate) fn id(v: &Value) -> Result<Identifier> {
    Ok(Identifier::parse(string(v)?)?)
}
pub(crate) fn kind(v: &Value) -> Result<Identifier> {
    id(field(v, "type")?)
}
/// JsonOps delegates to Number.intValue: decimal truncation then low 32 bits.
pub(crate) fn int(v: &Value) -> Result<i32> {
    let number = v
        .as_number()
        .ok_or_else(|| error("expected number"))?
        .to_string();
    let (negative, unsigned) = number
        .strip_prefix('-')
        .map_or((false, number.as_str()), |v| (true, v));
    let (mantissa, exponent) =
        unsigned
            .split_once(['e', 'E'])
            .map_or((unsigned, 0_i64), |(m, e)| {
                let exponent = e.parse::<i64>().unwrap_or_else(|_| {
                    if e.starts_with('-') {
                        i64::MIN
                    } else {
                        i64::MAX
                    }
                });
                (m, exponent)
            });
    let decimals = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i64);
    let digits = mantissa.bytes().filter(|v| *v != b'.').collect::<Vec<_>>();
    let scale = exponent.saturating_sub(decimals);
    let (length, zeroes) = if scale < 0 {
        (
            digits
                .len()
                .saturating_sub(scale.unsigned_abs().min(usize::MAX as u64) as usize),
            0,
        )
    } else {
        (digits.len(), scale)
    };
    if zeroes >= 32 {
        return Ok(0);
    }
    let mut result = 0_i32;
    for digit in &digits[..length] {
        result = result
            .wrapping_mul(10)
            .wrapping_add(i32::from(digit - b'0'));
    }
    for _ in 0..zeroes {
        result = result.wrapping_mul(10);
    }
    Ok(if negative {
        result.wrapping_neg()
    } else {
        result
    })
}
pub(crate) fn float(v: &Value) -> Result<f32> {
    v.as_number()
        .and_then(|v| v.to_string().parse::<f32>().ok())
        .ok_or_else(|| error("expected float"))
}
pub(crate) fn double(v: &Value) -> Result<f64> {
    v.as_number()
        .and_then(|v| v.to_string().parse::<f64>().ok())
        .ok_or_else(|| error("expected double"))
}
pub(crate) fn boolean(v: &Value) -> Result<bool> {
    v.as_bool().ok_or_else(|| error("expected boolean"))
}
pub(crate) fn list<T>(v: &Value, decode: impl Fn(&Value) -> Result<T>) -> Result<Vec<T>> {
    v.as_array()
        .ok_or_else(|| error("expected list"))?
        .iter()
        .map(decode)
        .collect()
}
pub(crate) fn error(message: impl Into<String>) -> LootError {
    LootError::Decode(message.into())
}
