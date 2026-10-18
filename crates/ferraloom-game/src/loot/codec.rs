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
pub(crate) fn optional<T>(
    v: &Value,
    key: &str,
    decode: impl FnOnce(&Value) -> Result<T>,
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
pub(crate) fn int(v: &Value) -> Result<i32> {
    // JsonOps#getNumberValue follows Number.intValue, including truncation.
    v.as_f64()
        .map(|n| n as i64 as i32)
        .ok_or_else(|| error("expected number"))
}
pub(crate) fn float(v: &Value) -> Result<f32> {
    v.as_f64()
        .map(|n| n as f32)
        .filter(|n| n.is_finite())
        .ok_or_else(|| error("expected finite float"))
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
