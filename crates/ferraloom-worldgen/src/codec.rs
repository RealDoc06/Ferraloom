//! Codec primitives preserve vanilla's field defaults and numeric bounds.
use crate::{Error, Identifier};
use serde_json::Value;

pub(crate) fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value, Error> {
    v.get(name)
        .ok_or_else(|| Error(format!("missing field {name}")))
}
pub(crate) fn string(v: &Value) -> Result<&str, Error> {
    v.as_str().ok_or_else(|| Error("expected string".into()))
}
pub(crate) fn boolean(v: &Value) -> Result<bool, Error> {
    v.as_bool().ok_or_else(|| Error("expected boolean".into()))
}
pub(crate) fn list(v: &Value) -> Result<&[Value], Error> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| Error("expected list".into()))
}
pub(crate) fn double(v: &Value) -> Result<f64, Error> {
    let Value::Number(number) = v else {
        return Err(Error("expected number".into()));
    };
    number
        .to_string()
        .parse()
        .map_err(|_| Error("invalid double".into()))
}
pub(crate) fn float(v: &Value) -> Result<f32, Error> {
    let Value::Number(number) = v else {
        return Err(Error("expected number".into()));
    };
    // Gson's LazilyParsedNumber#floatValue rounds the decimal directly to float.
    number
        .to_string()
        .parse()
        .map_err(|_| Error("invalid float".into()))
}
/// Port of Codec.floatRange: round to float before checking Float.compare bounds.
pub(crate) fn float_range(v: &Value, min: f32, max: f32) -> Result<f32, Error> {
    let value = float(v)?;
    if !value.total_cmp(&min).is_lt() && !value.total_cmp(&max).is_gt() {
        Ok(value)
    } else {
        Err(Error(format!("number {value} outside [{min},{max}]")))
    }
}
pub(crate) fn bounded(v: &Value, min: f64, max: f64) -> Result<f64, Error> {
    let value = double(v)?;
    if !value.total_cmp(&min).is_lt() && !value.total_cmp(&max).is_gt() {
        Ok(value)
    } else {
        Err(Error(format!("number {value} outside [{min},{max}]")))
    }
}
pub(crate) fn noise_value(v: &Value) -> Result<f32, Error> {
    float_range(v, -1_000_000.0, 1_000_000.0)
}
/// Port of LazilyParsedNumber#intValue, preserving decimal truncation and
/// low-32-bit overflow. Both integer codecs use BigDecimal's low-bit result.
pub(crate) fn integer(v: &Value) -> Result<i32, Error> {
    Ok(decimal_integer(v)? as i32)
}
/// Port of LazilyParsedNumber#longValue (Codec.LONG under JsonOps).
pub(crate) fn long(v: &Value) -> Result<i64, Error> {
    Ok(decimal_integer(v)? as i64)
}
fn decimal_integer(v: &Value) -> Result<u64, Error> {
    let Value::Number(number) = v else {
        return Err(Error("expected number".into()));
    };
    let text = number.to_string();
    // Gson's NumberLimits applies these before BigDecimal's narrowing methods.
    if text.len() > 10_000 {
        return Err(Error("decimal string exceeds Gson limit".into()));
    }
    let negative = text.starts_with('-');
    let text = text.trim_start_matches('-');
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let exponent = exponent
        .parse::<i64>()
        .map_err(|_| Error("decimal exponent overflow".into()))?;
    let integer_digits = mantissa.find('.').unwrap_or(mantissa.len()) as i64;
    let fractional_digits =
        mantissa.len() as i64 - integer_digits - i64::from(mantissa.contains('.'));
    let scale = fractional_digits.saturating_sub(exponent);
    // BigDecimal rejects exponents/scales beyond its signed 32-bit scale.
    if exponent.unsigned_abs() > i32::MAX as u64
        || !(i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&scale)
    {
        return Err(Error("decimal scale overflow".into()));
    }
    if scale.unsigned_abs() >= 10_000 {
        return Err(Error("decimal scale exceeds Gson limit".into()));
    }
    let length = integer_digits.saturating_add(exponent);
    if length <= 0 {
        return Ok(0);
    }
    let digits = mantissa.bytes().filter(|&b| b != b'.').collect::<Vec<_>>();
    let take = (length as u64).min(digits.len() as u64) as usize;
    let mut value = digits[..take].iter().fold(0u64, |value, b| {
        value.wrapping_mul(10).wrapping_add(u64::from(b - b'0'))
    });
    let zeros = length.saturating_sub(digits.len() as i64);
    if zeros >= 64 {
        return Ok(0);
    }
    for _ in 0..zeros {
        value = value.wrapping_mul(10);
    }
    Ok(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}
pub(crate) fn int_range(v: &Value, min: i32, max: i32) -> Result<i32, Error> {
    let n = integer(v)?;
    if (min..=max).contains(&n) {
        Ok(n)
    } else {
        Err(Error(format!("integer {n} outside [{min},{max}]")))
    }
}
pub(crate) fn id(v: &Value) -> Result<Identifier, Error> {
    Ok(string(v)?.parse()?)
}
pub(crate) fn kind(v: &Value) -> Result<Identifier, Error> {
    id(field(v, "type")?)
}
pub(crate) fn optional<T>(
    v: &Value,
    name: &str,
    default: T,
    decode: impl FnOnce(&Value) -> Result<T, Error>,
) -> Result<T, Error> {
    match v.get(name) {
        Some(v) => decode(v),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn numeric_codecs_match_gson_truncation_overflow_and_float_bits() {
        let mut json = String::new();
        flate2::read::GzDecoder::new(
            include_bytes!("../tests/golden/numeric_codecs.json.gz").as_slice(),
        )
        .read_to_string(&mut json)
        .unwrap();
        let cases: Value = serde_json::from_str(&json).unwrap();
        for case in cases.as_array().unwrap() {
            let value = &case["input"];
            for (kind, actual) in [
                ("int", integer(value).map(|v| v.to_string())),
                ("long", long(value).map(|v| v.to_string())),
                ("float", float(value).map(|v| v.to_bits().to_string())),
                (
                    "float_range",
                    float_range(value, 0.0, 1.0).map(|v| v.to_bits().to_string()),
                ),
                ("double", double(value).map(|v| v.to_bits().to_string())),
            ] {
                assert_eq!(
                    actual.ok().as_deref(),
                    case[kind].as_str(),
                    "{kind} input={value}"
                );
            }
        }
    }
}
