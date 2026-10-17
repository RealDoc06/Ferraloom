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
    v.as_f64().ok_or_else(|| Error("expected number".into()))
}
pub(crate) fn float(v: &Value) -> Result<f32, Error> {
    Ok(double(v)? as f32)
}
pub(crate) fn bounded(v: &Value, min: f64, max: f64) -> Result<f64, Error> {
    let value = double(v)?;
    if value >= min && value <= max {
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
/// JsonOps#getNumberValue followed by Number#intValue truncates fractional JSON
/// numbers. Bounds are checked after conversion, just as Codec.intRange does.
pub(crate) fn integer(v: &Value) -> Result<i32, Error> {
    if let Some(n) = v.as_i64() {
        Ok(n as i32)
    } else if let Some(n) = v.as_u64() {
        Ok(n as i32)
    } else {
        Ok(double(v)? as i32)
    }
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
