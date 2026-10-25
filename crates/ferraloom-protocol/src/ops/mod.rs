//! Encode-side subset of vanilla DynamicOps. Decoders remain format-specific.
use crate::{Error, Result};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use serde_json::Value;

/// The scalar kinds are deliberately distinct: HashOps does not hash an int like a byte or boolean.
pub trait EncodeOps {
    type Value;
    fn empty(&self) -> Self::Value;
    fn create_boolean(&self, value: bool) -> Self::Value;
    fn create_byte(&self, value: i8) -> Self::Value;
    fn create_short(&self, value: i16) -> Self::Value;
    fn create_int(&self, value: i32) -> Self::Value;
    fn create_long(&self, value: i64) -> Self::Value;
    fn create_float(&self, value: f32) -> Self::Value;
    fn create_double(&self, value: f64) -> Self::Value;
    fn create_string(&self, value: &str) -> Self::Value {
        self.create_string_utf16(&value.encode_utf16().collect::<Vec<_>>())
    }
    fn create_string_utf16(&self, value: &[u16]) -> Self::Value;
    fn create_byte_list(&self, value: &[i8]) -> Self::Value;
    fn create_int_list(&self, value: &[i32]) -> Self::Value;
    fn create_long_list(&self, value: &[i64]) -> Self::Value;
    fn create_list(&self, value: Vec<Self::Value>) -> Self::Value;
    fn create_map(&self, value: Vec<(Self::Value, Self::Value)>) -> Result<Self::Value>;
}
/// Port of NullOps: run persistent codec constraints without retaining encoded values.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullOps;
impl EncodeOps for NullOps {
    type Value = ();
    fn empty(&self) {}
    fn create_boolean(&self, _: bool) {}
    fn create_byte(&self, _: i8) {}
    fn create_short(&self, _: i16) {}
    fn create_int(&self, _: i32) {}
    fn create_long(&self, _: i64) {}
    fn create_float(&self, _: f32) {}
    fn create_double(&self, _: f64) {}
    fn create_string_utf16(&self, _: &[u16]) {}
    fn create_byte_list(&self, _: &[i8]) {}
    fn create_int_list(&self, _: &[i32]) {}
    fn create_long_list(&self, _: &[i64]) {}
    fn create_list(&self, _: Vec<()>) {}
    fn create_map(&self, _: Vec<((), ())>) -> Result<()> {
        Ok(())
    }
}
/// Port of NbtOps' creation operations. CompoundTag owns vanilla's Java key traversal order.
#[derive(Clone, Copy, Debug, Default)]
pub struct NbtOps;
impl EncodeOps for NbtOps {
    type Value = Tag;
    fn empty(&self) -> Tag {
        Tag::End
    }
    fn create_boolean(&self, v: bool) -> Tag {
        Tag::Byte(i8::from(v))
    }
    fn create_byte(&self, v: i8) -> Tag {
        Tag::Byte(v)
    }
    fn create_short(&self, v: i16) -> Tag {
        Tag::Short(v)
    }
    fn create_int(&self, v: i32) -> Tag {
        Tag::Int(v)
    }
    fn create_long(&self, v: i64) -> Tag {
        Tag::Long(v)
    }
    fn create_float(&self, v: f32) -> Tag {
        // FloatTag.valueOf and DoubleTag.valueOf share their positive-zero tag.
        Tag::Float(if v == 0.0 { 0.0 } else { v })
    }
    fn create_double(&self, v: f64) -> Tag {
        Tag::Double(if v == 0.0 { 0.0 } else { v })
    }
    fn create_string_utf16(&self, v: &[u16]) -> Tag {
        Tag::String(NbtString::from_utf16(v.to_vec()))
    }
    fn create_byte_list(&self, v: &[i8]) -> Tag {
        Tag::ByteArray(v.to_vec())
    }
    fn create_int_list(&self, v: &[i32]) -> Tag {
        Tag::IntArray(v.to_vec())
    }
    fn create_long_list(&self, v: &[i64]) -> Tag {
        Tag::LongArray(v.to_vec())
    }
    fn create_list(&self, v: Vec<Tag>) -> Tag {
        Tag::List(ListTag(v))
    }
    fn create_map(&self, v: Vec<(Tag, Tag)>) -> Result<Tag> {
        let mut result = CompoundTag::new();
        for (key, value) in v {
            let Tag::String(key) = key else {
                return Err(Error::invalid("NBT map key", "expected string"));
            };
            result.insert(key, value);
        }
        Ok(Tag::Compound(result))
    }
}
/// Port of JsonOps' creation operations, retaining insertion order through serde_json's map.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonOps;
impl EncodeOps for JsonOps {
    type Value = Value;
    fn empty(&self) -> Value {
        Value::Null
    }
    fn create_boolean(&self, v: bool) -> Value {
        v.into()
    }
    fn create_byte(&self, v: i8) -> Value {
        v.into()
    }
    fn create_short(&self, v: i16) -> Value {
        v.into()
    }
    fn create_int(&self, v: i32) -> Value {
        v.into()
    }
    fn create_long(&self, v: i64) -> Value {
        v.into()
    }
    fn create_float(&self, v: f32) -> Value {
        serde_json::to_value(v).unwrap_or(Value::Null)
    }
    fn create_double(&self, v: f64) -> Value {
        v.into()
    }
    fn create_string_utf16(&self, v: &[u16]) -> Value {
        String::from_utf16_lossy(v).into()
    }
    fn create_byte_list(&self, v: &[i8]) -> Value {
        Value::Array(v.iter().map(|v| (*v).into()).collect())
    }
    fn create_int_list(&self, v: &[i32]) -> Value {
        Value::Array(v.iter().map(|v| (*v).into()).collect())
    }
    fn create_long_list(&self, v: &[i64]) -> Value {
        Value::Array(v.iter().map(|v| (*v).into()).collect())
    }
    fn create_list(&self, v: Vec<Value>) -> Value {
        Value::Array(v)
    }
    fn create_map(&self, v: Vec<(Value, Value)>) -> Result<Value> {
        let mut result = serde_json::Map::new();
        for (key, value) in v {
            let key = match key {
                Value::String(v) => v,
                Value::Number(v) => v.to_string(),
                Value::Bool(v) => v.to_string(),
                _ => return Err(Error::invalid("JSON map key", "expected primitive")),
            };
            result.insert(key, value);
        }
        Ok(Value::Object(result))
    }
}
/// A Guava HashCode.asInt-compatible CRC32C value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HashCode(pub i32);
/// Port of HashOps.CRC32C_INSTANCE, including little-endian primitive sinks and unsigned hash ordering.
#[derive(Clone, Copy, Debug, Default)]
pub struct HashOps;
impl EncodeOps for HashOps {
    type Value = HashCode;
    fn empty(&self) -> HashCode {
        hash(&[1])
    }
    fn create_boolean(&self, v: bool) -> HashCode {
        hash(&[13, u8::from(v)])
    }
    fn create_byte(&self, v: i8) -> HashCode {
        tagged(6, &v.to_le_bytes())
    }
    fn create_short(&self, v: i16) -> HashCode {
        tagged(7, &v.to_le_bytes())
    }
    fn create_int(&self, v: i32) -> HashCode {
        tagged(8, &v.to_le_bytes())
    }
    fn create_long(&self, v: i64) -> HashCode {
        tagged(9, &v.to_le_bytes())
    }
    fn create_float(&self, v: f32) -> HashCode {
        tagged(10, &v.to_le_bytes())
    }
    fn create_double(&self, v: f64) -> HashCode {
        tagged(11, &v.to_le_bytes())
    }
    fn create_string_utf16(&self, v: &[u16]) -> HashCode {
        let mut bytes = vec![12];
        bytes.extend((v.len() as i32).to_le_bytes());
        for unit in v {
            bytes.extend(unit.to_le_bytes());
        }
        hash(&bytes)
    }
    fn create_byte_list(&self, v: &[i8]) -> HashCode {
        let mut bytes = vec![14];
        bytes.extend(v.iter().map(|v| *v as u8));
        bytes.push(15);
        hash(&bytes)
    }
    fn create_int_list(&self, v: &[i32]) -> HashCode {
        let mut bytes = vec![16];
        for n in v {
            bytes.extend(n.to_le_bytes());
        }
        bytes.push(17);
        hash(&bytes)
    }
    fn create_long_list(&self, v: &[i64]) -> HashCode {
        let mut bytes = vec![18];
        for n in v {
            bytes.extend(n.to_le_bytes());
        }
        bytes.push(19);
        hash(&bytes)
    }
    fn create_list(&self, v: Vec<HashCode>) -> HashCode {
        let mut bytes = vec![4];
        for n in v {
            bytes.extend(n.0.to_le_bytes());
        }
        bytes.push(5);
        hash(&bytes)
    }
    fn create_map(&self, mut v: Vec<(HashCode, HashCode)>) -> Result<HashCode> {
        v.sort_by_key(|(k, v)| (k.0 as u32, v.0 as u32));
        let mut bytes = vec![2];
        for (k, v) in v {
            bytes.extend(k.0.to_le_bytes());
            bytes.extend(v.0.to_le_bytes());
        }
        bytes.push(3);
        Ok(hash(&bytes))
    }
}
fn tagged(tag: u8, value: &[u8]) -> HashCode {
    let mut bytes = vec![tag];
    bytes.extend(value);
    hash(&bytes)
}
fn hash(bytes: &[u8]) -> HashCode {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    HashCode(!crc as i32)
}
/// Port of NbtOps#convertTo. It retains array and numeric kinds.
pub fn encode_nbt<O: EncodeOps>(tag: &Tag, ops: &O) -> Result<O::Value> {
    Ok(match tag {
        Tag::End => ops.empty(),
        Tag::Byte(v) => ops.create_byte(*v),
        Tag::Short(v) => ops.create_short(*v),
        Tag::Int(v) => ops.create_int(*v),
        Tag::Long(v) => ops.create_long(*v),
        Tag::Float(v) => ops.create_float(*v),
        Tag::Double(v) => ops.create_double(*v),
        Tag::String(v) => ops.create_string_utf16(v.as_utf16()),
        Tag::ByteArray(v) => ops.create_byte_list(v),
        Tag::IntArray(v) => ops.create_int_list(v),
        Tag::LongArray(v) => ops.create_long_list(v),
        Tag::List(v) => ops.create_list(
            v.iter()
                .map(|v| encode_nbt(v, ops))
                .collect::<Result<_>>()?,
        ),
        Tag::Compound(v) => ops.create_map(
            // CompoundTag.CODEC copies in the source HashMap traversal order.
            v.iter_java_order()
                .map(|(k, v)| Ok((ops.create_string_utf16(k.as_utf16()), encode_nbt(v, ops)?)))
                .collect::<Result<_>>()?,
        )?,
    })
}

#[cfg(test)]
mod tests;
