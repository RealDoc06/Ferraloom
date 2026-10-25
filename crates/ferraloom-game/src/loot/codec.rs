use super::{Identifier, LootError, Result};
use ferraloom_protocol::component::Persistent;
mod input;
pub use input::{DecodeInput, NbtOps};

pub(crate) fn object(v: &serde_json::Value) -> Result<&serde_json::Map<String, serde_json::Value>> {
    v.as_object().ok_or_else(|| error("expected object"))
}
pub(crate) fn map<I: DecodeInput>(v: &I) -> Result<&I> {
    if v.is_object() {
        Ok(v)
    } else {
        Err(error("expected object"))
    }
}
pub(crate) fn field<'a, I: DecodeInput>(v: &'a I, key: &str) -> Result<&'a I> {
    map(v)?
        .get(key)
        .ok_or_else(|| error(format!("missing {key}")))
}
pub(crate) fn optional<'a, I: DecodeInput, T>(
    v: &'a I,
    key: &str,
    decode: impl FnOnce(&'a I) -> Result<T>,
) -> Result<Option<T>> {
    map(v)?.get(key).map(decode).transpose()
}
pub(crate) fn string(v: &impl DecodeInput) -> Result<&str> {
    v.as_str().ok_or_else(|| error("expected string"))
}
pub(crate) fn id(v: &impl DecodeInput) -> Result<Identifier> {
    Ok(Identifier::parse(string(v)?)?)
}
pub(crate) fn kind(v: &impl DecodeInput) -> Result<Identifier> {
    id(field(v, "type")?)
}
/// Primitive codecs retain the protocol's JsonOps/NbtOps coercions and limits.
pub(crate) fn int(v: &impl DecodeInput) -> Result<i32> {
    Ok(v.read()?)
}
pub(crate) fn long(v: &impl DecodeInput) -> Result<i64> {
    Ok(v.read()?)
}
pub(crate) fn float(v: &impl DecodeInput) -> Result<f32> {
    Ok(v.read()?)
}
pub(crate) fn double(v: &impl DecodeInput) -> Result<f64> {
    Ok(v.read()?)
}
pub(crate) fn boolean(v: &impl DecodeInput) -> Result<bool> {
    Ok(v.read()?)
}
pub(crate) fn list<I: DecodeInput, T>(v: &I, decode: impl Fn(&I) -> Result<T>) -> Result<Vec<T>> {
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
pub(crate) fn persistent<T: ferraloom_protocol::component::Persistent>(
    v: &impl DecodeInput,
) -> Result<T> {
    let value = v.read::<T>()?;
    value.to_persistent()?;
    Ok(value)
}

/// Select the original DynamicOps for an existing persistent value codec.
pub(crate) trait PersistentInput: Persistent {
    fn from_input(v: &impl DecodeInput) -> ferraloom_protocol::Result<Self> {
        v.read()
    }
}
impl<T: Persistent> PersistentInput for T {}

#[cfg(test)]
mod tests;
