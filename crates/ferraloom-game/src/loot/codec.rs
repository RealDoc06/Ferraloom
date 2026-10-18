use super::{Identifier, LootError, Result};
use ferraloom_protocol::component::Persistent;
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
/// Primitive codecs share the protocol's Gson/JsonOps coercion and limits.
pub(crate) fn int(v: &Value) -> Result<i32> {
    Ok(i32::from_json(v)?)
}
pub(crate) fn long(v: &Value) -> Result<i64> {
    Ok(i64::from_json(v)?)
}
pub(crate) fn float(v: &Value) -> Result<f32> {
    Ok(f32::from_json(v)?)
}
pub(crate) fn double(v: &Value) -> Result<f64> {
    Ok(f64::from_json(v)?)
}
pub(crate) fn boolean(v: &Value) -> Result<bool> {
    Ok(bool::from_json(v)?)
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

/// Apply the shared persistent codec's value constraints once during decoding.
/// Several protocol value types validate their range on persistent encoding.
pub(crate) fn persistent<T: ferraloom_protocol::component::Persistent>(v: &Value) -> Result<T> {
    let value = T::from_json(v)?;
    value.to_persistent()?;
    Ok(value)
}
