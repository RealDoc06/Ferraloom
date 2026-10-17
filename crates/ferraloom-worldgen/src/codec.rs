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
pub(crate) fn bounded(v: &Value, min: f64, max: f64) -> Result<f64, Error> {
    let value = double(v)?;
    if !value.total_cmp(&min).is_lt() && !value.total_cmp(&max).is_gt() {
        Ok(value)
    } else {
        Err(Error(format!("number {value} outside [{min},{max}]")))
    }
}
pub(crate) fn noise_value(v: &Value) -> Result<f32, Error> {
    let value = float(v)?;
    if (-1_000_000.0..=1_000_000.0).contains(&value) {
        Ok(value)
    } else {
        Err(Error("noise value outside [-1000000,1000000]".into()))
    }
}
/// Port of LazilyParsedNumber#intValue: BigDecimal truncation with low-32-bit
/// overflow, followed by Codec.intRange's bounds check.
pub(crate) fn integer(v: &Value) -> Result<i32, Error> {
    let Value::Number(number) = v else {
        return Err(Error("expected number".into()));
    };
    let text = number.to_string();
    let negative = text.starts_with('-');
    let text = text.trim_start_matches('-');
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let exponent = exponent.parse::<i64>().unwrap_or_else(|_| {
        if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    let integer_digits = mantissa.find('.').unwrap_or(mantissa.len()) as i64;
    let length = integer_digits.saturating_add(exponent);
    if length <= 0 {
        return Ok(0);
    }
    let digits = mantissa.bytes().filter(|&b| b != b'.').collect::<Vec<_>>();
    let take = (length as u64).min(digits.len() as u64) as usize;
    let mut value = digits[..take].iter().fold(0u32, |value, b| {
        value.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
    });
    let zeros = length.saturating_sub(digits.len() as i64);
    if zeros >= 32 {
        return Ok(0);
    }
    for _ in 0..zeros {
        value = value.wrapping_mul(10);
    }
    Ok(if negative {
        value.wrapping_neg()
    } else {
        value
    } as i32)
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
