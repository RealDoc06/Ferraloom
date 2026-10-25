//! Persistent component codecs and HashOps' typed CRC32C representation.
use super::*;
use crate::ops::{EncodeOps, HashOps, JsonOps, NbtOps, encode_nbt};
use crate::{
    BlockPos, Error, Identifier, Result, Uuid,
    chat::Component,
    codec::{Either, GlobalPos, Holder, HolderSet, VarInt},
};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use serde_json::{Map, Value as Json};

/// Intermediate values preserve numeric kinds and booleans, which HashOps hashes differently.
#[derive(Clone, Debug, PartialEq)]
pub enum PersistentValue {
    Nbt(Tag),
    Component(Box<Component>),
    Boolean(bool),
    String(NbtString),
    Int(i32),
    Float(JavaF32),
    Double(JavaF64),
    List(Vec<PersistentValue>),
    Map(Vec<(String, PersistentValue)>),
}
impl PersistentValue {
    pub fn encode_ops<O: EncodeOps>(&self, ops: &O) -> Result<O::Value> {
        Ok(match self {
            Self::Nbt(v) => return encode_nbt(v, ops),
            Self::Component(v) => return v.as_ref().encode_ops(ops),
            Self::Boolean(v) => ops.create_boolean(*v),
            Self::String(v) => ops.create_string_utf16(v.as_utf16()),
            Self::Int(v) => ops.create_int(*v),
            Self::Float(v) => ops.create_float(v.0),
            Self::Double(v) => ops.create_double(v.0),
            Self::List(v) => {
                ops.create_list(v.iter().map(|v| v.encode_ops(ops)).collect::<Result<_>>()?)
            }
            Self::Map(v) => ops.create_map(
                v.iter()
                    .map(|(k, v)| Ok((ops.create_string(k), v.encode_ops(ops)?)))
                    .collect::<Result<_>>()?,
            )?,
        })
    }
    pub fn to_json(&self) -> Result<Json> {
        self.encode_ops(&JsonOps)
    }
    pub fn to_nbt(&self) -> Result<Tag> {
        self.encode_ops(&NbtOps)
    }
    pub fn hash(&self) -> Result<i32> {
        Ok(self.encode_ops(&HashOps)?.0)
    }
}
/// Port of a component's persistent Codec, shared by item defaults, storage and commands.
pub trait Persistent: Sized {
    fn from_json(value: &Json) -> Result<Self>;
    fn to_persistent(&self) -> Result<PersistentValue>;
    fn encode_ops<O: EncodeOps>(&self, ops: &O) -> Result<O::Value> {
        self.to_persistent()?.encode_ops(ops)
    }
    fn to_json(&self) -> Result<Json> {
        self.encode_ops(&JsonOps)
    }
    fn from_nbt(value: &Tag) -> Result<Self> {
        Self::from_json(&tag_json(value))
    }
    fn to_nbt(&self) -> Result<Tag> {
        self.encode_ops(&NbtOps)
    }
    fn hash(&self) -> Result<i32> {
        Ok(self.encode_ops(&HashOps)?.0)
    }
}
pub(super) fn object(value: &Json) -> Result<&Map<String, Json>> {
    value.as_object().ok_or_else(|| invalid("component object"))
}
pub(super) fn field_nbt<T: Persistent>(value: &Tag, name: &str, default: Option<T>) -> Result<T> {
    match value.as_compound().and_then(|v| v.get(name)) {
        Some(v) => T::from_nbt(v),
        None => default.ok_or_else(|| crate::Error::invalid("component field", name)),
    }
}
pub(super) fn field<T: Persistent>(value: &Json, name: &str, default: Option<T>) -> Result<T> {
    match value.get(name) {
        Some(v) => T::from_json(v),
        None => default.ok_or_else(|| Error::invalid("component field", name)),
    }
}
pub(super) fn entry<T: Persistent + PartialEq>(
    entries: &mut Vec<(String, PersistentValue)>,
    name: &str,
    value: &T,
    default: Option<T>,
) -> Result<()> {
    if default.as_ref() != Some(value) {
        entries.push((name.into(), value.to_persistent()?));
    }
    Ok(())
}
// ExtraCodecs uses Float.compareTo: -0.0 sorts below +0.0, and NaN above finite values.
fn float_range(value: f32, min: f32, max: f32) -> bool {
    value.total_cmp(&min).is_ge() && value.total_cmp(&max).is_le()
}
fn nonnegative_float(value: &JavaF32) -> bool {
    float_range(value.0, 0.0, f32::MAX)
}
fn positive_float(value: &JavaF32) -> bool {
    value.0.total_cmp(&0.0).is_gt() && value.0.total_cmp(&f32::MAX).is_le()
}
fn player_name(value: &str) -> bool {
    value.encode_utf16().count() <= 16 && value.bytes().all(|v| v > 32 && v < 127)
}
macro_rules! persistent_record {
    ($ty:ident { $($field:ident => $name:literal [$default:expr $(; $check:expr)?]),* $(,)? }) => {
        impl Persistent for $ty {
            fn from_json(value:&Json)->Result<Self> { object(value)?; Ok(Self { $($field:field(value,$name,$default)?),* }) }
            fn from_nbt(value:&Tag)->Result<Self>{if value.as_compound().is_none(){return Err(invalid("component object"));}Ok(Self{$($field:field_nbt(value,$name,$default)?),*})}
            fn to_persistent(&self)->Result<PersistentValue> {
                let mut entries=Vec::new();
                $(let default = $default;
                $(if !($check)(&self.$field) {
                    return Err(invalid($name));
                })?
                entry(&mut entries,$name,&self.$field,default)?;)*
                Ok(PersistentValue::Map(entries))
            }
        }
    };
}
/// Gson's LazilyParsedNumber uses BigDecimal after integer parsing fails.
/// NumberLimits rejects over 10,000 characters and scales outside (-10,000, 10,000).
struct DecimalNumber {
    negative: bool,
    digits: Vec<u8>,
    scale: i32,
}
impl DecimalNumber {
    fn parse(text: &str) -> Result<Self> {
        if text.len() > 10_000 {
            return Err(invalid("number length"));
        }
        let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
        let exponent: i32 = exponent.parse().map_err(|_| invalid("number exponent"))?;
        let fraction = mantissa
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len());
        let scale = i32::try_from(fraction)
            .ok()
            .and_then(|n| n.checked_sub(exponent))
            .filter(|n| (-9_999..=9_999).contains(n))
            .ok_or_else(|| invalid("number scale"))?;
        Ok(Self {
            negative: text.starts_with('-'),
            digits: mantissa
                .bytes()
                .filter(u8::is_ascii_digit)
                .map(|n| n - b'0')
                .collect(),
            scale,
        })
    }
    fn integer_bits(&self) -> u64 {
        let end = (self.digits.len() as i32 - self.scale).max(0) as usize;
        let mut bits = 0u64;
        for digit in self
            .digits
            .iter()
            .copied()
            .chain(std::iter::repeat(0))
            .take(end)
        {
            bits = bits.wrapping_mul(10).wrapping_add(u64::from(digit));
        }
        if self.negative {
            bits.wrapping_neg()
        } else {
            bits
        }
    }
    fn exact_long(&self) -> Option<i64> {
        let end = (self.digits.len() as i32 - self.scale).max(0) as usize;
        if self
            .digits
            .get(end..)
            .is_some_and(|digits| digits.iter().any(|n| *n != 0))
        {
            return None;
        }
        let mut magnitude = 0u64;
        for digit in self
            .digits
            .iter()
            .copied()
            .chain(std::iter::repeat(0))
            .take(end)
        {
            magnitude = magnitude.checked_mul(10)?.checked_add(u64::from(digit))?;
        }
        if self.negative {
            if magnitude == 1u64 << 63 {
                Some(i64::MIN)
            } else {
                i64::try_from(magnitude).ok().map(|n| -n)
            }
        } else {
            i64::try_from(magnitude).ok()
        }
    }
}
fn json_number(value: &Json) -> Result<String> {
    match value {
        Json::Number(number) => Ok(number.to_string()),
        _ => Err(invalid("number")),
    }
}
fn json_integer_bits(value: &Json) -> Result<u64> {
    let text = json_number(value)?;
    match text.parse::<i64>() {
        Ok(value) => Ok(value as u64),
        Err(_) => Ok(DecimalNumber::parse(&text)?.integer_bits()),
    }
}
// NbtOps returns the boxed numeric value, then each primitive codec calls Number's
// conversion. Integral narrowing wraps; floating-to-integer conversion saturates.
macro_rules! nbt_number {
    ($value:expr, $ty:ty) => {
        match $value {
            Tag::Byte(n) => Ok(*n as $ty),
            Tag::Short(n) => Ok(*n as $ty),
            Tag::Int(n) => Ok(*n as $ty),
            Tag::Long(n) => Ok(*n as $ty),
            Tag::Float(n) => Ok(*n as $ty),
            Tag::Double(n) => Ok(*n as $ty),
            _ => Err(invalid("number")),
        }
    };
}
impl Persistent for bool {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_bool().ok_or_else(|| invalid("boolean"))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(f64::from_nbt(v)? != 0.0)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Boolean(*self))
    }
}
impl Persistent for i32 {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(json_integer_bits(v)? as i32)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        nbt_number!(v, i32)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Int(*self))
    }
}
impl Persistent for VarInt {
    fn from_json(v: &Json) -> Result<Self> {
        i32::from_json(v).map(Self)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        i32::from_nbt(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for f32 {
    fn from_json(v: &Json) -> Result<Self> {
        json_number(v)?.parse().map_err(|_| invalid("float"))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        nbt_number!(v, f32)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Float(JavaF32(*self)))
    }
}
impl Persistent for f64 {
    fn from_json(v: &Json) -> Result<Self> {
        json_number(v)?.parse().map_err(|_| invalid("double"))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        nbt_number!(v, f64)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Double(JavaF64(*self)))
    }
}
impl Persistent for JavaF32 {
    fn from_json(v: &Json) -> Result<Self> {
        f32::from_json(v).map(Self)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        f32::from_nbt(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Float(*self))
    }
}
impl Persistent for JavaF64 {
    fn from_json(v: &Json) -> Result<Self> {
        f64::from_json(v).map(Self)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        f64::from_nbt(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Double(*self))
    }
}
impl Persistent for String {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid("string"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::String(self.as_str().into()))
    }
}
impl Persistent for Identifier {
    fn from_json(v: &Json) -> Result<Self> {
        Self::parse(&String::from_json(v)?)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.to_string().to_persistent()
    }
}
impl Persistent for Unit {
    fn from_json(v: &Json) -> Result<Self> {
        object(v)?;
        Ok(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(vec![]))
    }
}
impl<T: Persistent> Persistent for Vec<T> {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_array()
            .ok_or_else(|| invalid("list"))?
            .iter()
            .map(T::from_json)
            .collect()
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let len = v.collection_len().ok_or_else(|| invalid("list"))?;
        (0..len)
            .map(|i| T::from_nbt(&v.collection_get(i).expect("collection element")))
            .collect()
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::List(
            self.iter().map(T::to_persistent).collect::<Result<_>>()?,
        ))
    }
}
impl<T: Persistent, const MAX: usize> Persistent for BoundedList<T, MAX> {
    fn from_json(v: &Json) -> Result<Self> {
        let value = Vec::<T>::from_json(v)?;
        if value.len() > MAX {
            return Err(invalid("list length"));
        }
        Ok(Self(value))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let values = Vec::<T>::from_nbt(v)?;
        if values.len() > MAX {
            return Err(invalid("list length"));
        }
        Ok(Self(values))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if self.0.len() > MAX {
            return Err(invalid("list length"));
        }
        self.0.to_persistent()
    }
}
impl<const MAX: usize> Persistent for BoundedString<MAX> {
    fn from_json(v: &Json) -> Result<Self> {
        let value = String::from_json(v)?;
        if value.encode_utf16().count() > MAX {
            return Err(invalid("string length"));
        }
        Ok(Self(value))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if self.0.encode_utf16().count() > MAX {
            return Err(invalid("string length"));
        }
        self.0.to_persistent()
    }
}
impl<T: Persistent> Persistent for Option<T> {
    fn from_json(v: &Json) -> Result<Self> {
        if v.is_null() {
            Ok(None)
        } else {
            T::from_json(v).map(Some)
        }
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        T::from_nbt(v).map(Some)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Some(v) => v.to_persistent(),
            None => Err(invalid("absent persistent value")),
        }
    }
}
impl<T: Persistent> Persistent for Box<T> {
    fn from_json(v: &Json) -> Result<Self> {
        T::from_json(v).map(Box::new)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        T::from_nbt(v).map(Box::new)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        (**self).to_persistent()
    }
}
impl<L: Persistent, R: Persistent> Persistent for Either<L, R> {
    fn from_nbt(v: &Tag) -> Result<Self> {
        L::from_nbt(v)
            .map(Self::Left)
            .or_else(|_| R::from_nbt(v).map(Self::Right))
    }
    fn from_json(v: &Json) -> Result<Self> {
        L::from_json(v)
            .map(Self::Left)
            .or_else(|_| R::from_json(v).map(Self::Right))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Left(v) => v.to_persistent(),
            Self::Right(v) => v.to_persistent(),
        }
    }
}
impl Persistent for Component {
    fn from_json(v: &Json) -> Result<Self> {
        Self::from_json(&v.to_string())
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Component::from_nbt(v.clone())
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Component(Box::new(self.clone())))
    }
}
pub(super) fn tag_json(v: &Tag) -> Json {
    match v {
        Tag::End => Json::Null,
        Tag::Byte(n) => Json::from(*n),
        Tag::Short(n) => Json::from(*n),
        Tag::Int(n) => Json::from(*n),
        Tag::Long(n) => Json::from(*n),
        Tag::Float(n) => Json::from(*n),
        Tag::Double(n) => Json::from(*n),
        Tag::String(v) => v.to_string_lossy().into(),
        Tag::List(v) => Json::Array(v.iter().map(tag_json).collect()),
        Tag::Compound(v) => Json::Object(
            v.iter()
                .map(|(k, v)| (k.to_string_lossy(), tag_json(v)))
                .collect(),
        ),
        Tag::ByteArray(v) => Json::Array(v.iter().map(|n| Json::from(*n)).collect()),
        Tag::IntArray(v) => Json::Array(v.iter().map(|n| Json::from(*n)).collect()),
        Tag::LongArray(v) => Json::Array(v.iter().map(|n| Json::from(*n)).collect()),
    }
}
fn json_tag(v: &Json) -> Result<Tag> {
    Ok(match v {
        Json::Null => return Err(invalid("null tag")),
        Json::Bool(v) => Tag::Byte(i8::from(*v)),
        Json::String(v) => Tag::String(v.as_str().into()),
        Json::Number(v) => {
            let decimal = DecimalNumber::parse(&v.to_string())?;
            if let Some(n) = decimal.exact_long() {
                if let Ok(n) = i8::try_from(n) {
                    Tag::Byte(n)
                } else if let Ok(n) = i16::try_from(n) {
                    Tag::Short(n)
                } else if let Ok(n) = i32::try_from(n) {
                    Tag::Int(n)
                } else {
                    Tag::Long(n)
                }
            } else {
                let n: f64 = v.to_string().parse().map_err(|_| invalid("number"))?;
                if f64::from(n as f32) == n {
                    Tag::Float(n as f32)
                } else {
                    Tag::Double(n)
                }
            }
        }
        Json::Array(a) => Tag::List(ListTag(a.iter().map(json_tag).collect::<Result<_>>()?)),
        Json::Object(o) => {
            let mut t = CompoundTag::new();
            for (k, v) in o {
                t.insert(k.as_str(), json_tag(v)?);
            }
            Tag::Compound(t)
        }
    })
}
impl Persistent for Tag {
    fn from_json(v: &Json) -> Result<Self> {
        json_tag(v)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Nbt(self.clone()))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(v.clone())
    }
}
impl Persistent for CustomData {
    fn from_json(v: &Json) -> Result<Self> {
        let tag = if let Some(snbt) = v.as_str() {
            Tag::Compound(
                ferraloom_nbt::parse_compound(snbt).map_err(|_| invalid("compound SNBT"))?,
            )
        } else {
            object(v)?;
            json_tag(v)?
        };
        Ok(Self(tag))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if self.0.as_compound().is_none() {
            return Err(invalid("compound tag"));
        }
        self.0.to_persistent()
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        match v {
            Tag::Compound(_) => Ok(Self(v.clone())),
            Tag::String(snbt) => Ok(Self(Tag::Compound(
                ferraloom_nbt::parse_compound(&snbt.to_string_lossy())
                    .map_err(|_| invalid("compound SNBT"))?,
            ))),
            _ => Err(invalid("compound tag")),
        }
    }
}
impl Persistent for Enchantment {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for Attribute {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for DamageType {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for MobEffect {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for Potion {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for EntityType {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for BlockEntityType {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for VillagerType {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for VillagerProfession {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for WolfVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for WolfSoundVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for CowVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for CowSoundVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for PigVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for PigSoundVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for ChickenVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for ChickenSoundVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for CatVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for CatSoundVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for FrogVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for ZombieNautilusVariant {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for DecoratedPotPattern {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for BlockTransformerHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Identifier::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}

impl Persistent for BlockSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for ItemSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for EntityTypeSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for DamageTypeSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for MobEffectSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for BannerPatternSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for ComponentTypeSet {
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(&self.0)
    }
}
impl Persistent for Sound {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(Box::new(SoundEvent::from_json(v)?))
        }))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(if matches!(v, Tag::String(_)) {
            Holder::Reference(Identifier::from_nbt(v)?)
        } else {
            Holder::Direct(Box::new(SoundEvent::from_nbt(v)?))
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for TrimMaterialHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(Box::new(TrimMaterial::from_json(v)?))
        }))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(if matches!(v, Tag::String(_)) {
            Holder::Reference(Identifier::from_nbt(v)?)
        } else {
            Holder::Direct(Box::new(TrimMaterial::from_nbt(v)?))
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for TrimPatternHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(Box::new(TrimPattern::from_json(v)?))
        }))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(if matches!(v, Tag::String(_)) {
            Holder::Reference(Identifier::from_nbt(v)?)
        } else {
            Holder::Direct(Box::new(TrimPattern::from_nbt(v)?))
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for InstrumentHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(Box::new(Instrument::from_json(v)?))
        }))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(if matches!(v, Tag::String(_)) {
            Holder::Reference(Identifier::from_nbt(v)?)
        } else {
            Holder::Direct(Box::new(Instrument::from_nbt(v)?))
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for JukeboxSongHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(Holder::Reference(Identifier::from_json(v)?)))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(_) => Err(invalid("unregistered holder")),
        }
    }
}

impl Persistent for BannerPatternHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(Box::new(BannerPattern::from_json(v)?))
        }))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(if matches!(v, Tag::String(_)) {
            Holder::Reference(Identifier::from_nbt(v)?)
        } else {
            Holder::Direct(Box::new(BannerPattern::from_nbt(v)?))
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for PaintingVariantHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(Holder::Reference(Identifier::from_json(v)?)))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(_) => Err(invalid("unregistered holder")),
        }
    }
}

impl Persistent for Rarity {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("common") => Ok(Self::Common),
            Some("uncommon") => Ok(Self::Uncommon),
            Some("rare") => Ok(Self::Rare),
            Some("epic") => Ok(Self::Epic),
            _ => Err(invalid("Rarity")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Common => "common".to_owned().to_persistent(),
            Self::Uncommon => "uncommon".to_owned().to_persistent(),
            Self::Rare => "rare".to_owned().to_persistent(),
            Self::Epic => "epic".to_owned().to_persistent(),
        }
    }
}
impl Persistent for DyeColor {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("white") => Ok(Self::White),
            Some("orange") => Ok(Self::Orange),
            Some("magenta") => Ok(Self::Magenta),
            Some("light_blue") => Ok(Self::LightBlue),
            Some("yellow") => Ok(Self::Yellow),
            Some("lime") => Ok(Self::Lime),
            Some("pink") => Ok(Self::Pink),
            Some("gray") => Ok(Self::Gray),
            Some("light_gray") => Ok(Self::LightGray),
            Some("cyan") => Ok(Self::Cyan),
            Some("purple") => Ok(Self::Purple),
            Some("blue") => Ok(Self::Blue),
            Some("brown") => Ok(Self::Brown),
            Some("green") => Ok(Self::Green),
            Some("red") => Ok(Self::Red),
            Some("black") => Ok(Self::Black),
            _ => Err(invalid("DyeColor")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::White => "white".to_owned().to_persistent(),
            Self::Orange => "orange".to_owned().to_persistent(),
            Self::Magenta => "magenta".to_owned().to_persistent(),
            Self::LightBlue => "light_blue".to_owned().to_persistent(),
            Self::Yellow => "yellow".to_owned().to_persistent(),
            Self::Lime => "lime".to_owned().to_persistent(),
            Self::Pink => "pink".to_owned().to_persistent(),
            Self::Gray => "gray".to_owned().to_persistent(),
            Self::LightGray => "light_gray".to_owned().to_persistent(),
            Self::Cyan => "cyan".to_owned().to_persistent(),
            Self::Purple => "purple".to_owned().to_persistent(),
            Self::Blue => "blue".to_owned().to_persistent(),
            Self::Brown => "brown".to_owned().to_persistent(),
            Self::Green => "green".to_owned().to_persistent(),
            Self::Red => "red".to_owned().to_persistent(),
            Self::Black => "black".to_owned().to_persistent(),
        }
    }
}
impl Persistent for EquipmentSlot {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("mainhand") => Ok(Self::Mainhand),
            Some("feet") => Ok(Self::Feet),
            Some("legs") => Ok(Self::Legs),
            Some("chest") => Ok(Self::Chest),
            Some("head") => Ok(Self::Head),
            Some("offhand") => Ok(Self::Offhand),
            Some("body") => Ok(Self::Body),
            Some("saddle") => Ok(Self::Saddle),
            _ => Err(invalid("EquipmentSlot")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Mainhand => "mainhand".to_owned().to_persistent(),
            Self::Feet => "feet".to_owned().to_persistent(),
            Self::Legs => "legs".to_owned().to_persistent(),
            Self::Chest => "chest".to_owned().to_persistent(),
            Self::Head => "head".to_owned().to_persistent(),
            Self::Offhand => "offhand".to_owned().to_persistent(),
            Self::Body => "body".to_owned().to_persistent(),
            Self::Saddle => "saddle".to_owned().to_persistent(),
        }
    }
}
impl Persistent for EquipmentSlotGroup {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("any") => Ok(Self::Any),
            Some("mainhand") => Ok(Self::Mainhand),
            Some("offhand") => Ok(Self::Offhand),
            Some("hand") => Ok(Self::Hand),
            Some("feet") => Ok(Self::Feet),
            Some("legs") => Ok(Self::Legs),
            Some("chest") => Ok(Self::Chest),
            Some("head") => Ok(Self::Head),
            Some("armor") => Ok(Self::Armor),
            Some("body") => Ok(Self::Body),
            Some("saddle") => Ok(Self::Saddle),
            _ => Err(invalid("EquipmentSlotGroup")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Any => "any".to_owned().to_persistent(),
            Self::Mainhand => "mainhand".to_owned().to_persistent(),
            Self::Offhand => "offhand".to_owned().to_persistent(),
            Self::Hand => "hand".to_owned().to_persistent(),
            Self::Feet => "feet".to_owned().to_persistent(),
            Self::Legs => "legs".to_owned().to_persistent(),
            Self::Chest => "chest".to_owned().to_persistent(),
            Self::Head => "head".to_owned().to_persistent(),
            Self::Armor => "armor".to_owned().to_persistent(),
            Self::Body => "body".to_owned().to_persistent(),
            Self::Saddle => "saddle".to_owned().to_persistent(),
        }
    }
}
impl Persistent for AttributeOperation {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("add_value") => Ok(Self::AddValue),
            Some("add_multiplied_base") => Ok(Self::AddMultipliedBase),
            Some("add_multiplied_total") => Ok(Self::AddMultipliedTotal),
            _ => Err(invalid("AttributeOperation")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::AddValue => "add_value".to_owned().to_persistent(),
            Self::AddMultipliedBase => "add_multiplied_base".to_owned().to_persistent(),
            Self::AddMultipliedTotal => "add_multiplied_total".to_owned().to_persistent(),
        }
    }
}
impl Persistent for FireworkShape {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("small_ball") => Ok(Self::SmallBall),
            Some("large_ball") => Ok(Self::LargeBall),
            Some("star") => Ok(Self::Star),
            Some("creeper") => Ok(Self::Creeper),
            Some("burst") => Ok(Self::Burst),
            _ => Err(invalid("FireworkShape")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::SmallBall => "small_ball".to_owned().to_persistent(),
            Self::LargeBall => "large_ball".to_owned().to_persistent(),
            Self::Star => "star".to_owned().to_persistent(),
            Self::Creeper => "creeper".to_owned().to_persistent(),
            Self::Burst => "burst".to_owned().to_persistent(),
        }
    }
}
impl Persistent for ItemUseAnimation {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("none") => Ok(Self::None),
            Some("eat") => Ok(Self::Eat),
            Some("drink") => Ok(Self::Drink),
            Some("block") => Ok(Self::Block),
            Some("bow") => Ok(Self::Bow),
            Some("trident") => Ok(Self::Trident),
            Some("crossbow") => Ok(Self::Crossbow),
            Some("spyglass") => Ok(Self::Spyglass),
            Some("toot_horn") => Ok(Self::TootHorn),
            Some("brush") => Ok(Self::Brush),
            Some("bundle") => Ok(Self::Bundle),
            Some("spear") => Ok(Self::Spear),
            _ => Err(invalid("ItemUseAnimation")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::None => "none".to_owned().to_persistent(),
            Self::Eat => "eat".to_owned().to_persistent(),
            Self::Drink => "drink".to_owned().to_persistent(),
            Self::Block => "block".to_owned().to_persistent(),
            Self::Bow => "bow".to_owned().to_persistent(),
            Self::Trident => "trident".to_owned().to_persistent(),
            Self::Crossbow => "crossbow".to_owned().to_persistent(),
            Self::Spyglass => "spyglass".to_owned().to_persistent(),
            Self::TootHorn => "toot_horn".to_owned().to_persistent(),
            Self::Brush => "brush".to_owned().to_persistent(),
            Self::Bundle => "bundle".to_owned().to_persistent(),
            Self::Spear => "spear".to_owned().to_persistent(),
        }
    }
}
impl Persistent for SwingAnimationType {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("none") => Ok(Self::None),
            Some("whack") => Ok(Self::Whack),
            Some("stab") => Ok(Self::Stab),
            _ => Err(invalid("SwingAnimationType")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::None => "none".to_owned().to_persistent(),
            Self::Whack => "whack".to_owned().to_persistent(),
            Self::Stab => "stab".to_owned().to_persistent(),
        }
    }
}
impl Persistent for MapPostProcessing {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("lock") => Ok(Self::Lock),
            Some("scale") => Ok(Self::Scale),
            _ => Err(invalid("MapPostProcessing")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Lock => "lock".to_owned().to_persistent(),
            Self::Scale => "scale".to_owned().to_persistent(),
        }
    }
}
impl Persistent for FoxVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("red") => Ok(Self::Red),
            Some("snow") => Ok(Self::Snow),
            _ => Err(invalid("FoxVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Red => "red".to_owned().to_persistent(),
            Self::Snow => "snow".to_owned().to_persistent(),
        }
    }
}
impl Persistent for SalmonVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("small") => Ok(Self::Small),
            Some("medium") => Ok(Self::Medium),
            Some("large") => Ok(Self::Large),
            _ => Err(invalid("SalmonVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Small => "small".to_owned().to_persistent(),
            Self::Medium => "medium".to_owned().to_persistent(),
            Self::Large => "large".to_owned().to_persistent(),
        }
    }
}
impl Persistent for ParrotVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("red_blue") => Ok(Self::RedBlue),
            Some("blue") => Ok(Self::Blue),
            Some("green") => Ok(Self::Green),
            Some("yellow_blue") => Ok(Self::YellowBlue),
            Some("gray") => Ok(Self::Gray),
            _ => Err(invalid("ParrotVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::RedBlue => "red_blue".to_owned().to_persistent(),
            Self::Blue => "blue".to_owned().to_persistent(),
            Self::Green => "green".to_owned().to_persistent(),
            Self::YellowBlue => "yellow_blue".to_owned().to_persistent(),
            Self::Gray => "gray".to_owned().to_persistent(),
        }
    }
}
impl Persistent for TropicalFishPattern {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("kob") => Ok(Self::Kob),
            Some("sunstreak") => Ok(Self::Sunstreak),
            Some("snooper") => Ok(Self::Snooper),
            Some("dasher") => Ok(Self::Dasher),
            Some("brinely") => Ok(Self::Brinely),
            Some("spotty") => Ok(Self::Spotty),
            Some("flopper") => Ok(Self::Flopper),
            Some("stripey") => Ok(Self::Stripey),
            Some("glitter") => Ok(Self::Glitter),
            Some("blockfish") => Ok(Self::Blockfish),
            Some("betty") => Ok(Self::Betty),
            Some("clayfish") => Ok(Self::Clayfish),
            _ => Err(invalid("TropicalFishPattern")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Kob => "kob".to_owned().to_persistent(),
            Self::Sunstreak => "sunstreak".to_owned().to_persistent(),
            Self::Snooper => "snooper".to_owned().to_persistent(),
            Self::Dasher => "dasher".to_owned().to_persistent(),
            Self::Brinely => "brinely".to_owned().to_persistent(),
            Self::Spotty => "spotty".to_owned().to_persistent(),
            Self::Flopper => "flopper".to_owned().to_persistent(),
            Self::Stripey => "stripey".to_owned().to_persistent(),
            Self::Glitter => "glitter".to_owned().to_persistent(),
            Self::Blockfish => "blockfish".to_owned().to_persistent(),
            Self::Betty => "betty".to_owned().to_persistent(),
            Self::Clayfish => "clayfish".to_owned().to_persistent(),
        }
    }
}
impl Persistent for MooshroomVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("red") => Ok(Self::Red),
            Some("brown") => Ok(Self::Brown),
            _ => Err(invalid("MooshroomVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Red => "red".to_owned().to_persistent(),
            Self::Brown => "brown".to_owned().to_persistent(),
        }
    }
}
impl Persistent for RabbitVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("brown") => Ok(Self::Brown),
            Some("white") => Ok(Self::White),
            Some("black") => Ok(Self::Black),
            Some("white_splotched") => Ok(Self::WhiteSplotched),
            Some("gold") => Ok(Self::Gold),
            Some("salt") => Ok(Self::Salt),
            Some("evil") => Ok(Self::Evil),
            _ => Err(invalid("RabbitVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Brown => "brown".to_owned().to_persistent(),
            Self::White => "white".to_owned().to_persistent(),
            Self::Black => "black".to_owned().to_persistent(),
            Self::WhiteSplotched => "white_splotched".to_owned().to_persistent(),
            Self::Gold => "gold".to_owned().to_persistent(),
            Self::Salt => "salt".to_owned().to_persistent(),
            Self::Evil => "evil".to_owned().to_persistent(),
        }
    }
}
impl Persistent for HorseVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("white") => Ok(Self::White),
            Some("creamy") => Ok(Self::Creamy),
            Some("chestnut") => Ok(Self::Chestnut),
            Some("brown") => Ok(Self::Brown),
            Some("black") => Ok(Self::Black),
            Some("gray") => Ok(Self::Gray),
            Some("dark_brown") => Ok(Self::DarkBrown),
            _ => Err(invalid("HorseVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::White => "white".to_owned().to_persistent(),
            Self::Creamy => "creamy".to_owned().to_persistent(),
            Self::Chestnut => "chestnut".to_owned().to_persistent(),
            Self::Brown => "brown".to_owned().to_persistent(),
            Self::Black => "black".to_owned().to_persistent(),
            Self::Gray => "gray".to_owned().to_persistent(),
            Self::DarkBrown => "dark_brown".to_owned().to_persistent(),
        }
    }
}
impl Persistent for LlamaVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("creamy") => Ok(Self::Creamy),
            Some("white") => Ok(Self::White),
            Some("brown") => Ok(Self::Brown),
            Some("gray") => Ok(Self::Gray),
            _ => Err(invalid("LlamaVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Creamy => "creamy".to_owned().to_persistent(),
            Self::White => "white".to_owned().to_persistent(),
            Self::Brown => "brown".to_owned().to_persistent(),
            Self::Gray => "gray".to_owned().to_persistent(),
        }
    }
}
impl Persistent for AxolotlVariant {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("lucy") => Ok(Self::Lucy),
            Some("wild") => Ok(Self::Wild),
            Some("gold") => Ok(Self::Gold),
            Some("cyan") => Ok(Self::Cyan),
            Some("blue") => Ok(Self::Blue),
            _ => Err(invalid("AxolotlVariant")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Lucy => "lucy".to_owned().to_persistent(),
            Self::Wild => "wild".to_owned().to_persistent(),
            Self::Gold => "gold".to_owned().to_persistent(),
            Self::Cyan => "cyan".to_owned().to_persistent(),
            Self::Blue => "blue".to_owned().to_persistent(),
        }
    }
}

pub(super) fn set_from_json(v: &Json) -> Result<HolderSet> {
    if let Some(s) = v.as_str() {
        if let Some(tag) = s.strip_prefix('#') {
            return Ok(HolderSet::Named(Identifier::parse(tag)?));
        }
        return Ok(HolderSet::Direct(vec![Identifier::parse(s)?]));
    }
    Ok(HolderSet::Direct(Vec::<Identifier>::from_json(v)?))
}
pub(super) fn set_value(v: &HolderSet) -> Result<PersistentValue> {
    match v {
        HolderSet::Named(k) => format!("#{k}").to_persistent(),
        HolderSet::Direct(entries) => {
            if entries.len() == 1 {
                entries[0].to_persistent()
            } else {
                entries.to_persistent()
            }
        }
    }
}
persistent_record!(SoundEvent {location => "sound_id" [None],fixed_range => "range" [Some(None)]});
persistent_record!(TrimMaterial {palette_id => "palette_id" [None],description => "description" [None]});
persistent_record!(TrimPattern {asset_id => "asset_id" [None],description => "description" [None],decal => "decal" [Some(false)]});
persistent_record!(Instrument {sound_event => "sound_event" [None],use_duration => "use_duration" [None; nonnegative_float],range => "range" [None; positive_float],durability_damage => "durability_damage" [Some(VarInt(0)); |v: &VarInt| v.0 >= 0],description => "description" [None]});
persistent_record!(JukeboxSong {sound_event => "sound_event" [None],description => "description" [None],length_in_seconds => "length_in_seconds" [None; positive_float],comparator_output => "comparator_output" [None; |v: &VarInt| (0..=15).contains(&v.0)]});
persistent_record!(BannerPattern {asset_id => "asset_id" [None],translation_key => "translation_key" [None]});
persistent_record!(PaintingVariant {width => "width" [None; |v: &VarInt| (1..=16).contains(&v.0)],height => "height" [None; |v: &VarInt| (1..=16).contains(&v.0)],asset_id => "asset_id" [None],title => "title" [Some(None)],author => "author" [Some(None)]});
persistent_record!(UseEffects {can_sprint => "can_sprint" [Some(false)],interact_vibrations => "interact_vibrations" [Some(true)],speed_multiplier => "speed_multiplier" [Some(JavaF32(0.2)); |v: &JavaF32| float_range(v.0, 0.0, 1.0)]});
persistent_record!(FoodProperties {nutrition => "nutrition" [None; |v: &VarInt| v.0 >= 0],saturation => "saturation" [None],can_always_eat => "can_always_eat" [Some(false)]});
persistent_record!(Weapon {item_damage_per_attack => "item_damage_per_attack" [Some(VarInt(1)); |v: &VarInt| v.0 >= 0],disable_blocking_for_seconds => "disable_blocking_for_seconds" [Some(JavaF32(0.0)); nonnegative_float]});
persistent_record!(UseCooldown {seconds => "seconds" [None; positive_float],cooldown_group => "cooldown_group" [Some(None)]});
persistent_record!(AttackRange {min_reach => "min_reach" [Some(JavaF32(0.0)); |v: &JavaF32| float_range(v.0, 0.0, 64.0)],max_reach => "max_reach" [Some(JavaF32(3.0)); |v: &JavaF32| float_range(v.0, 0.0, 64.0)],min_creative_reach => "min_creative_reach" [Some(JavaF32(0.0)); |v: &JavaF32| float_range(v.0, 0.0, 64.0)],max_creative_reach => "max_creative_reach" [Some(JavaF32(5.0)); |v: &JavaF32| float_range(v.0, 0.0, 64.0)],hitbox_margin => "hitbox_margin" [Some(JavaF32(0.3)); |v: &JavaF32| float_range(v.0, 0.0, 1.0)],mob_factor => "mob_factor" [Some(JavaF32(1.0)); |v: &JavaF32| float_range(v.0, 0.0, 2.0)]});
persistent_record!(Consumable {consume_seconds => "consume_seconds" [Some(JavaF32(1.6)); nonnegative_float],animation => "animation" [Some(ItemUseAnimation::Eat)],sound => "sound" [Some(sound("entity.generic.eat"))],has_consume_particles => "has_consume_particles" [Some(true)],on_consume_effects => "on_consume_effects" [Some(vec![])]});
persistent_record!(DeathProtection {death_effects => "death_effects" [Some(vec![])]});
persistent_record!(Tool {rules => "rules" [None],default_mining_speed => "default_mining_speed" [Some(JavaF32(1.0))],damage_per_block => "damage_per_block" [Some(VarInt(1)); |v: &VarInt| v.0 >= 0],can_destroy_blocks_in_creative => "can_destroy_blocks_in_creative" [Some(true)]});
persistent_record!(ToolRule {blocks => "blocks" [None],speed => "speed" [Some(None); |v: &Option<JavaF32>| v.as_ref().is_none_or(positive_float)],correct_for_drops => "correct_for_drops" [Some(None)]});
persistent_record!(Equippable {slot => "slot" [None],equip_sound => "equip_sound" [Some(sound("item.armor.equip_generic"))],asset_id => "asset_id" [Some(None)],camera_overlay => "camera_overlay" [Some(None)],allowed_entities => "allowed_entities" [Some(None)],dispensable => "dispensable" [Some(true)],swappable => "swappable" [Some(true)],damage_on_hurt => "damage_on_hurt" [Some(true)],equip_on_interact => "equip_on_interact" [Some(false)],can_be_sheared => "can_be_sheared" [Some(false)],shearing_sound => "shearing_sound" [Some(sound("item.shears.snip"))]});
persistent_record!(BlocksAttacks {block_delay_seconds => "block_delay_seconds" [Some(JavaF32(0.0)); nonnegative_float],disable_cooldown_scale => "disable_cooldown_scale" [Some(JavaF32(1.0)); nonnegative_float],damage_reductions => "damage_reductions" [Some(vec![DamageReduction {horizontal_blocking_angle:JavaF32(90.0),types:None,base:JavaF32(0.0),factor:JavaF32(1.0)}])],item_damage => "item_damage" [Some(ItemDamageFunction {threshold:JavaF32(1.0),base:JavaF32(0.0),factor:JavaF32(1.0)})],bypassed_by => "bypassed_by" [Some(None)],block_sound => "block_sound" [Some(None)],disable_sound => "disabled_sound" [Some(None)]});
persistent_record!(DamageReduction {horizontal_blocking_angle => "horizontal_blocking_angle" [Some(JavaF32(90.0)); positive_float],types => "type" [Some(None)],base => "base" [None],factor => "factor" [None]});
persistent_record!(ItemDamageFunction {threshold => "threshold" [None; nonnegative_float],base => "base" [None],factor => "factor" [None]});
persistent_record!(PiercingWeapon {deals_knockback => "deals_knockback" [Some(true)],dismounts => "dismounts" [Some(false)],sound => "sound" [Some(None)],hit_sound => "hit_sound" [Some(None)]});
persistent_record!(KineticWeapon {contact_cooldown_ticks => "contact_cooldown_ticks" [Some(VarInt(10)); |v: &VarInt| v.0 >= 0],delay_ticks => "delay_ticks" [Some(VarInt(0)); |v: &VarInt| v.0 >= 0],dismount_conditions => "dismount_conditions" [Some(None)],knockback_conditions => "knockback_conditions" [Some(None)],damage_conditions => "damage_conditions" [Some(None)],forward_movement => "forward_movement" [Some(JavaF32(0.0))],damage_multiplier => "damage_multiplier" [Some(JavaF32(1.0))],sound => "sound" [Some(None)],hit_sound => "hit_sound" [Some(None)]});
persistent_record!(KineticCondition {max_duration_ticks => "max_duration_ticks" [None; |v: &VarInt| v.0 >= 0],min_speed => "min_speed" [Some(JavaF32(0.0))],min_relative_speed => "min_relative_speed" [Some(JavaF32(0.0))]});
persistent_record!(SwingAnimation {animation_type => "type" [Some(SwingAnimationType::Whack)],duration => "duration" [Some(VarInt(6)); |v: &VarInt| v.0 >= 0]});
persistent_record!(CustomModelData {floats => "floats" [Some(vec![])],flags => "flags" [Some(vec![])],strings => "strings" [Some(vec![])],colors => "colors" [Some(vec![])]});
persistent_record!(FireworkExplosion {shape => "shape" [None],colors => "colors" [Some(vec![])],fade_colors => "fade_colors" [Some(vec![])],has_trail => "has_trail" [Some(false)],has_twinkle => "has_twinkle" [Some(false)]});
impl Persistent for Fireworks {
    fn from_json(v: &Json) -> Result<Self> {
        let flight_duration = VarInt(field::<i32>(v, "flight_duration", Some(0))? as u8 as i32);
        Ok(Self {
            flight_duration,
            explosions: field(v, "explosions", Some(BoundedList(vec![])))?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let flight_duration = VarInt(field_nbt::<i32>(v, "flight_duration", Some(0))? as u8 as i32);
        Ok(Self {
            flight_duration,
            explosions: field_nbt(v, "explosions", Some(BoundedList(vec![])))?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        // ExtraCodecs.UNSIGNED_BYTE rejects values above 255 but narrows negative values.
        if self.flight_duration.0 > 255 {
            return Err(invalid("unsigned byte"));
        }
        let mut v = Vec::new();
        if self.flight_duration.0 != 0 {
            v.push((
                "flight_duration".into(),
                PersistentValue::Nbt(Tag::Byte(self.flight_duration.0 as i8)),
            ));
        }
        entry(
            &mut v,
            "explosions",
            &self.explosions,
            Some(BoundedList(vec![])),
        )?;
        Ok(PersistentValue::Map(v))
    }
}
persistent_record!(LodestoneTracker {target => "target" [Some(None)],tracked => "tracked" [Some(true)]});
persistent_record!(ArmorTrim {material => "material" [None],pattern => "pattern" [None]});
persistent_record!(BannerLayer {pattern => "pattern" [None],color => "color" [None]});
persistent_record!(PotionContents {potion => "potion" [Some(None)],custom_color => "custom_color" [Some(None)],custom_effects => "custom_effects" [Some(vec![])],custom_name => "custom_name" [Some(None)]});
persistent_record!(StewEffect {effect => "id" [None],duration => "duration" [Some(VarInt(160))]});
persistent_record!(WrittenBookContent {title => "title" [None],author => "author" [None],generation => "generation" [Some(VarInt(0)); |v: &VarInt| (0..=3).contains(&v.0)],pages => "pages" [Some(vec![])],resolved => "resolved" [Some(false)]});
persistent_record!(BeeOccupant {entity_data => "entity_data" [None],ticks_in_hive => "ticks_in_hive" [None],min_ticks_in_hive => "min_ticks_in_hive" [None]});
persistent_record!(PotDecorations {back => "back" [Some(None)],left => "left" [Some(None)],right => "right" [Some(None)],front => "front" [Some(None)]});
persistent_record!(Compostable {layers => "layers" [None]});
persistent_record!(CookingFuel {burn_time => "burn_time" [None],speed_multiplier => "speed_multiplier" [None]});
persistent_record!(BrewingFuel {uses => "uses" [None],speed_multiplier => "speed_multiplier" [None]});
persistent_record!(MobVisibility {targeting_entity_types => "targeting_entity_types" [None],visibility => "visibility" [None; |v: &JavaF32| float_range(v.0, 0.0, 10.0)]});
persistent_record!(TooltipDisplay {hide_tooltip => "hide_tooltip" [Some(false)],hidden_components => "hidden_components" [Some(ComponentTypeList {entries:vec![]})]});
persistent_record!(VillagerFood {nutrition => "nutrition" [None; |v: &VarInt| v.0 > 0]});
persistent_record!(SkinPatch {body => "texture" [Some(None)],cape => "cape" [Some(None)],elytra => "elytra" [Some(None)],model => "model" [Some(None)]});
persistent_record!(PropertyRange {min => "min" [Some(None)],max => "max" [Some(None)]});

fn sound(name: &str) -> Sound {
    Sound(Holder::Reference(registry(name)))
}
impl Persistent for PlayerModelType {
    fn from_json(v: &Json) -> Result<Self> {
        match v.as_str() {
            Some("slim") => Ok(Self::Slim),
            Some("wide") => Ok(Self::Wide),
            _ => Err(invalid("player model")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::Slim => "slim",
            Self::Wide => "wide",
        }
        .to_owned()
        .to_persistent()
    }
}
impl Persistent for ferraloom_data::Item {
    fn from_json(v: &Json) -> Result<Self> {
        let key = Identifier::from_json(v)?;
        Self::from_name(key.as_str()).ok_or_else(|| invalid("item"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.name().to_owned().to_persistent()
    }
}
impl Persistent for ComponentTypeId {
    fn from_json(v: &Json) -> Result<Self> {
        let key = Identifier::from_json(v)?;
        Self::from_name(key.as_str()).ok_or_else(|| invalid("component type"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.name().to_owned().to_persistent()
    }
}
impl Persistent for ComponentTypeList {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            entries: Vec::from_json(v)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.entries.to_persistent()
    }
}
impl Persistent for ItemEnchantments {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(
            object(v)?
                .iter()
                .map(|(k, v)| {
                    let level = i32::from_json(v)?;
                    if !(1..=255).contains(&level) {
                        return Err(invalid("enchantment level"));
                    }
                    Ok((Enchantment(registry(k)), VarInt(level)))
                })
                .collect::<Result<_>>()?,
        ))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(
            v.as_compound()
                .ok_or_else(|| invalid("component object"))?
                .iter()
                .map(|(k, v)| {
                    let level = i32::from_nbt(v)?;
                    if !(1..=255).contains(&level) {
                        return Err(invalid("enchantment level"));
                    }
                    Ok((Enchantment(registry(&k.to_string_lossy())), VarInt(level)))
                })
                .collect::<Result<_>>()?,
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(k, v)| {
                    if !(1..=255).contains(&v.0) {
                        return Err(invalid("enchantment level"));
                    }
                    Ok((k.0.to_string(), v.to_persistent()?))
                })
                .collect::<Result<_>>()?,
        ))
    }
}
impl Persistent for BlockItemStateProperties {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(
            object(v)?
                .iter()
                .map(|(k, v)| Ok((k.clone(), String::from_json(v)?)))
                .collect::<Result<_>>()?,
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.to_persistent()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
impl<T: Persistent + PartialEq> Persistent for Filterable<T> {
    fn from_json(v: &Json) -> Result<Self> {
        if v.get("raw").is_some() {
            Ok(Self {
                raw: field(v, "raw", None)?,
                filtered: field(v, "filtered", Some(None))?,
            })
        } else {
            Ok(Self {
                raw: T::from_json(v)?,
                filtered: None,
            })
        }
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        if v.as_compound().is_some_and(|v| v.contains_key("raw")) {
            Ok(Self {
                raw: field_nbt(v, "raw", None)?,
                filtered: field_nbt(v, "filtered", Some(None))?,
            })
        } else {
            Ok(Self {
                raw: T::from_nbt(v)?,
                filtered: None,
            })
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = vec![("raw".into(), self.raw.to_persistent()?)];
        entry(&mut v, "filtered", &self.filtered, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for ItemStackTemplate {
    fn from_json(v: &Json) -> Result<Self> {
        if v.is_string() {
            return Self::new(
                ferraloom_data::Item::from_json(v)?,
                1,
                DataComponentPatch::default(),
            );
        }
        let item = field(v, "id", None)?;
        let count = field::<i32>(v, "count", Some(1))?;
        if !(1..=99).contains(&count) {
            return Err(invalid("stack template count"));
        }
        Self::new(
            item,
            count,
            field(v, "components", Some(DataComponentPatch::default()))?,
        )
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        if matches!(v, Tag::String(_)) {
            return Self::new(
                ferraloom_data::Item::from_nbt(v)?,
                1,
                DataComponentPatch::default(),
            );
        }
        let count = field_nbt::<i32>(v, "count", Some(1))?;
        if !(1..=99).contains(&count) {
            return Err(invalid("stack template count"));
        }
        Self::new(
            field_nbt(v, "id", None)?,
            count,
            field_nbt(v, "components", Some(DataComponentPatch::default()))?,
        )
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if !(1..=99).contains(&self.count) || self.item == ferraloom_data::Item::AIR {
            return Err(invalid("item stack template"));
        }

        let mut v = vec![("id".into(), self.item.to_persistent()?)];
        entry(&mut v, "count", &self.count, Some(1))?;
        entry(
            &mut v,
            "components",
            &self.components,
            Some(DataComponentPatch::default()),
        )?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for DataComponentPatch {
    fn from_json(v: &Json) -> Result<Self> {
        let mut patch = Self::default();
        for (k, v) in object(v)? {
            if let Some(k) = k.strip_prefix('!') {
                let id = ComponentTypeId::from_name(registry(k).as_str())
                    .ok_or_else(|| invalid("component removal"))?;
                object(v)?;
                patch.remove(id);
            } else {
                patch.set(DataComponent::from_json(&registry(k), v)?);
            }
        }
        Ok(patch)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let mut patch = Self::default();
        for (k, v) in v
            .as_compound()
            .ok_or_else(|| invalid("component patch"))?
            .iter()
        {
            let key = k.to_string_lossy();
            if let Some(key) = key.strip_prefix('!') {
                Unit::from_nbt(v)?;
                patch.remove(
                    ComponentTypeId::from_name(registry(key).as_str())
                        .ok_or_else(|| invalid("component removal"))?,
                );
            } else {
                patch.set(DataComponent::from_nbt(&registry(&key), v)?);
            }
        }
        Ok(patch)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        for c in self.added.iter().filter(|v| !v.is_transient()) {
            v.push((c.component_type().name().into(), c.to_persistent()?));
        }
        for c in &self.removed {
            v.push((format!("!{}", c.name()), PersistentValue::Map(vec![])));
        }
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for GlobalPos {
    fn from_json(v: &Json) -> Result<Self> {
        let p = Vec::<i32>::from_json(v.get("pos").ok_or_else(|| invalid("position"))?)?;
        if p.len() != 3 {
            return Err(invalid("position"));
        }
        Ok(Self {
            dimension: field(v, "dimension", None)?,
            pos: BlockPos::new(p[0], p[1], p[2]),
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let p = Vec::<i32>::from_nbt(
            v.as_compound()
                .and_then(|v| v.get("pos"))
                .ok_or_else(|| invalid("position"))?,
        )?;
        if p.len() != 3 {
            return Err(invalid("position"));
        }
        Ok(Self {
            dimension: field_nbt(v, "dimension", None)?,
            pos: BlockPos::new(p[0], p[1], p[2]),
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(vec![
            ("dimension".into(), self.dimension.to_persistent()?),
            (
                "pos".into(),
                PersistentValue::Nbt(Tag::IntArray(vec![self.pos.x, self.pos.y, self.pos.z])),
            ),
        ]))
    }
}
impl Persistent for i64 {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(json_integer_bits(v)? as i64)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        nbt_number!(v, i64)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Nbt(Tag::Long(*self)))
    }
}
impl<T: Persistent, const N: usize> Persistent for [T; N] {
    fn from_json(v: &Json) -> Result<Self> {
        Vec::<T>::from_json(v)?
            .try_into()
            .map_err(|_| invalid("array length"))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Vec::<T>::from_nbt(v)?
            .try_into()
            .map_err(|_| invalid("array length"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::List(
            self.iter().map(T::to_persistent).collect::<Result<_>>()?,
        ))
    }
}
impl Persistent for Uuid {
    fn from_json(v: &Json) -> Result<Self> {
        let a = Vec::<i32>::from_json(v)?;
        if a.len() != 4 {
            return Err(invalid("UUID"));
        }
        Ok(Self::new(
            (i64::from(a[0]) << 32) | (a[1] as u32 as i64),
            (i64::from(a[2]) << 32) | (a[3] as u32 as i64),
        ))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let a = Vec::<i32>::from_nbt(v)?;
        if a.len() != 4 {
            return Err(invalid("UUID"));
        }
        Ok(Self::new(
            (i64::from(a[0]) << 32) | (a[1] as u32 as i64),
            (i64::from(a[2]) << 32) | (a[3] as u32 as i64),
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Nbt(Tag::IntArray(vec![
            (self.most >> 32) as i32,
            self.most as i32,
            (self.least >> 32) as i32,
            self.least as i32,
        ])))
    }
}
persistent_record!(ProfileProperty {name=>"name"[None; |v: &String| v.encode_utf16().count() <= 64],value=>"value"[None; |v: &String| v.encode_utf16().count() <= 32767],signature=>"signature"[Some(None); |v: &Option<String>| v.as_ref().is_none_or(|v| v.encode_utf16().count() <= 1024)]});
persistent_record!(GameProfile {id=>"id"[None],name=>"name"[None; |v: &BoundedString<16>| player_name(&v.0)],properties=>"properties"[Some(BoundedList(vec![]))]});
persistent_record!(PartialProfile {name=>"name"[Some(None); |v: &Option<BoundedString<16>>| v.as_ref().is_none_or(|v| player_name(&v.0))],id=>"id"[Some(None)],properties=>"properties"[Some(BoundedList(vec![]))]});
impl Persistent for ResolvableProfile {
    fn from_json(v: &Json) -> Result<Self> {
        if v.is_string() {
            return Ok(Self {
                profile: Either::Right(PartialProfile {
                    name: Some(BoundedString(String::from_json(v)?)),
                    id: None,
                    properties: BoundedList(vec![]),
                }),
                skin_patch: SkinPatch {
                    body: None,
                    cape: None,
                    elytra: None,
                    model: None,
                },
            });
        }
        let profile = if v.get("id").is_some() && v.get("name").is_some() {
            Either::Left(GameProfile::from_json(v)?)
        } else {
            Either::Right(PartialProfile::from_json(v)?)
        };
        Ok(Self {
            profile,
            skin_patch: SkinPatch::from_json(v)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        if matches!(v, Tag::String(_)) {
            return Ok(Self {
                profile: Either::Right(PartialProfile {
                    name: Some(BoundedString(String::from_nbt(v)?)),
                    id: None,
                    properties: BoundedList(vec![]),
                }),
                skin_patch: SkinPatch {
                    body: None,
                    cape: None,
                    elytra: None,
                    model: None,
                },
            });
        }
        let profile = if v
            .as_compound()
            .is_some_and(|v| v.get("id").is_some() && v.get("name").is_some())
        {
            Either::Left(GameProfile::from_nbt(v)?)
        } else {
            Either::Right(PartialProfile::from_nbt(v)?)
        };
        Ok(Self {
            profile,
            skin_patch: SkinPatch::from_nbt(v)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.profile.to_persistent()?)?;
        v.extend(map_entries(self.skin_patch.to_persistent()?)?);
        Ok(PersistentValue::Map(v))
    }
}
pub(super) fn map_entries(value: PersistentValue) -> Result<Vec<(String, PersistentValue)>> {
    match value {
        PersistentValue::Map(v) => Ok(v),
        _ => Err(invalid("map")),
    }
}
impl Persistent for AttributeModifier {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            id: field(v, "id", None)?,
            amount: field(v, "amount", None)?,
            operation: field(v, "operation", None)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            id: field_nbt(v, "id", None)?,
            amount: field_nbt(v, "amount", None)?,
            operation: field_nbt(v, "operation", None)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(vec![
            ("id".into(), self.id.to_persistent()?),
            ("amount".into(), self.amount.to_persistent()?),
            ("operation".into(), self.operation.to_persistent()?),
        ]))
    }
}
impl Persistent for AttributeDisplay {
    fn from_json(v: &Json) -> Result<Self> {
        match v.get("type").and_then(Json::as_str) {
            Some("default") => Ok(Self::Default),
            Some("hidden") => Ok(Self::Hidden),
            Some("override") => Ok(Self::Override(field(v, "value", None)?)),
            _ => Err(invalid("attribute display")),
        }
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        match v
            .as_compound()
            .and_then(|v| v.get("type"))
            .and_then(Tag::as_string)
            .map(|v| v.to_string_lossy())
            .as_deref()
        {
            Some("default") => Ok(Self::Default),
            Some("hidden") => Ok(Self::Hidden),
            Some("override") => Ok(Self::Override(field_nbt(v, "value", None)?)),
            _ => Err(invalid("attribute display")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let (name, extra) = match self {
            Self::Default => ("default", None),
            Self::Hidden => ("hidden", None),
            Self::Override(v) => ("override", Some(v.to_persistent()?)),
        };
        let mut v = vec![("type".into(), name.to_owned().to_persistent()?)];
        if let Some(extra) = extra {
            v.push(("value".into(), extra));
        }
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for AttributeEntry {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            attribute: field(v, "type", None)?,
            modifier: AttributeModifier::from_json(v)?,
            slot: field(v, "slot", Some(EquipmentSlotGroup::Any))?,
            display: field(v, "display", Some(AttributeDisplay::Default))?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            attribute: field_nbt(v, "type", None)?,
            modifier: AttributeModifier::from_nbt(v)?,
            slot: field_nbt(v, "slot", Some(EquipmentSlotGroup::Any))?,
            display: field_nbt(v, "display", Some(AttributeDisplay::Default))?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.modifier.to_persistent()?)?;
        v.push(("type".into(), self.attribute.to_persistent()?));
        entry(&mut v, "slot", &self.slot, Some(EquipmentSlotGroup::Any))?;
        entry(
            &mut v,
            "display",
            &self.display,
            Some(AttributeDisplay::Default),
        )?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for MobEffectDetails {
    fn from_json(v: &Json) -> Result<Self> {
        let show_particles = field(v, "show_particles", Some(true))?;
        Ok(Self {
            amplifier: VarInt(field::<i32>(v, "amplifier", Some(0))? as u8 as i32),
            duration: field(v, "duration", Some(VarInt(0)))?,
            ambient: field(v, "ambient", Some(false))?,
            show_particles,
            show_icon: field(v, "show_icon", Some(show_particles))?,
            hidden_effect: field(v, "hidden_effect", Some(None))?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let fields = v
            .as_compound()
            .ok_or_else(|| invalid("mob effect details map"))?;
        // Details.MAP_CODEC's optional fields retain defaults on malformed
        // values; show_icon falls back to the decoded show_particles value.
        let boolean = |name: &str, default| {
            fields
                .get(name)
                .and_then(|value| bool::from_nbt(value).ok())
                .unwrap_or(default)
        };
        let show_particles = boolean("show_particles", true);
        Ok(Self {
            amplifier: VarInt(
                fields
                    .get("amplifier")
                    .and_then(|value| i32::from_nbt(value).ok())
                    .unwrap_or(0) as u8 as i32,
            ),
            duration: fields
                .get("duration")
                .and_then(|value| VarInt::from_nbt(value).ok())
                .unwrap_or(VarInt(0)),
            ambient: boolean("ambient", false),
            show_particles,
            show_icon: boolean("show_icon", show_particles),
            hidden_effect: fields
                .get("hidden_effect")
                .and_then(|value| Self::from_nbt(value).ok())
                .map(Box::new),
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        // ExtraCodecs.UNSIGNED_BYTE rejects values above 255 but narrows negative values.
        if self.amplifier.0 > 255 {
            return Err(invalid("unsigned byte"));
        }
        let mut v = Vec::new();
        if self.amplifier.0 != 0 {
            v.push((
                "amplifier".into(),
                PersistentValue::Nbt(Tag::Byte(self.amplifier.0 as i8)),
            ));
        }
        entry(&mut v, "duration", &self.duration, Some(VarInt(0)))?;
        entry(&mut v, "ambient", &self.ambient, Some(false))?;
        entry(&mut v, "show_particles", &self.show_particles, Some(true))?;
        v.push(("show_icon".into(), self.show_icon.to_persistent()?));
        entry(&mut v, "hidden_effect", &self.hidden_effect, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for MobEffectInstance {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            effect: field(v, "id", None)?,
            details: MobEffectDetails::from_json(v)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            effect: field_nbt(v, "id", None)?,
            details: MobEffectDetails::from_nbt(v)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.details.to_persistent()?)?;
        v.push(("id".into(), self.effect.to_persistent()?));
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for ConsumeEffect {
    fn from_json(v: &Json) -> Result<Self> {
        match v.get("type").and_then(Json::as_str) {
            Some("minecraft:apply_effects" | "apply_effects") => Ok(Self::ApplyEffects {
                effects: field(v, "effects", None)?,
                probability: field(v, "probability", Some(JavaF32(1.0)))?,
            }),
            Some("minecraft:remove_effects" | "remove_effects") => {
                Ok(Self::RemoveEffects(field(v, "effects", None)?))
            }
            Some("minecraft:clear_all_effects" | "clear_all_effects") => Ok(Self::ClearAllEffects),
            Some("minecraft:teleport_randomly" | "teleport_randomly") => {
                Ok(Self::TeleportRandomly {
                    diameter: field(v, "diameter", Some(JavaF32(16.0)))?,
                    directional_particles: field(v, "directional_particles", Some(true))?,
                })
            }
            Some("minecraft:play_sound" | "play_sound") => {
                Ok(Self::PlaySound(field(v, "sound", None)?))
            }
            _ => Err(invalid("consume effect")),
        }
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        match field_nbt::<String>(v, "type", None)?.as_str() {
            "minecraft:apply_effects" | "apply_effects" => Ok(Self::ApplyEffects {
                effects: field_nbt(v, "effects", None)?,
                probability: field_nbt(v, "probability", Some(JavaF32(1.0)))?,
            }),
            "minecraft:remove_effects" | "remove_effects" => {
                Ok(Self::RemoveEffects(field_nbt(v, "effects", None)?))
            }
            "minecraft:clear_all_effects" | "clear_all_effects" => Ok(Self::ClearAllEffects),
            "minecraft:teleport_randomly" | "teleport_randomly" => Ok(Self::TeleportRandomly {
                diameter: field_nbt(v, "diameter", Some(JavaF32(16.0)))?,
                directional_particles: field_nbt(v, "directional_particles", Some(true))?,
            }),
            "minecraft:play_sound" | "play_sound" => {
                Ok(Self::PlaySound(field_nbt(v, "sound", None)?))
            }
            _ => Err(invalid("consume effect")),
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        let name = match self {
            Self::ApplyEffects {
                effects,
                probability,
            } => {
                if !float_range(probability.0, 0.0, 1.0) {
                    return Err(invalid("effect probability"));
                }
                v.push(("effects".into(), effects.to_persistent()?));
                entry(&mut v, "probability", probability, Some(JavaF32(1.0)))?;
                "apply_effects"
            }
            Self::RemoveEffects(e) => {
                v.push(("effects".into(), e.to_persistent()?));
                "remove_effects"
            }
            Self::ClearAllEffects => "clear_all_effects",
            Self::TeleportRandomly {
                diameter,
                directional_particles,
            } => {
                if !positive_float(diameter) {
                    return Err(invalid("teleport diameter"));
                }
                entry(&mut v, "diameter", diameter, Some(JavaF32(16.0)))?;
                entry(
                    &mut v,
                    "directional_particles",
                    directional_particles,
                    Some(true),
                )?;
                "teleport_randomly"
            }
            Self::PlaySound(s) => {
                v.push(("sound".into(), s.to_persistent()?));
                "play_sound"
            }
        };
        v.push(("type".into(), format!("minecraft:{name}").to_persistent()?));
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for TypedEntityData {
    fn from_json(v: &Json) -> Result<Self> {
        Self::from_nbt(&CustomData::from_json(v)?.0)
    }

    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut tag = self.tag.0.clone();
        tag.as_compound_mut()
            .ok_or_else(|| invalid("compound tag"))?
            .insert("id", self.entity_type.to_nbt()?);
        Ok(PersistentValue::Nbt(tag))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let mut tag = CustomData::from_nbt(v)?.0;
        let id = tag
            .as_compound_mut()
            .ok_or_else(|| invalid("entity data"))?
            .remove("id")
            .ok_or_else(|| invalid("entity type"))?;
        Ok(Self {
            entity_type: EntityType::from_nbt(&id)?,
            tag: CustomData(tag),
        })
    }
}
impl Persistent for TypedBlockEntityData {
    fn from_json(v: &Json) -> Result<Self> {
        Self::from_nbt(&CustomData::from_json(v)?.0)
    }

    fn from_nbt(v: &Tag) -> Result<Self> {
        let mut tag = CustomData::from_nbt(v)?.0;
        let id = tag
            .as_compound_mut()
            .ok_or_else(|| invalid("block entity data"))?
            .remove("id")
            .ok_or_else(|| invalid("block entity type"))?;
        Ok(Self {
            entity_type: BlockEntityType::from_nbt(&id)?,
            tag: CustomData(tag),
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut tag = self.tag.0.clone();
        tag.as_compound_mut()
            .ok_or_else(|| invalid("compound tag"))?
            .insert("id", self.entity_type.to_nbt()?);
        Ok(PersistentValue::Nbt(tag))
    }
}
persistent_record!(SignText {messages=>"messages"[None],filtered_messages=>"filtered_messages"[Some(None)],color=>"color"[None],has_glowing_text=>"has_glowing_text"[None]});
impl Persistent for PropertyMatcher {
    fn from_json(_: &Json) -> Result<Self> {
        Err(invalid("named property matcher"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.matcher.to_persistent()
    }
}
impl Persistent for DataComponentMatchers {
    fn from_json(v: &Json) -> Result<Self> {
        let exact = if let Some(v) = v.get("components") {
            let patch = DataComponentPatch::from_json(v)?;
            if !patch.removed.is_empty() {
                return Err(invalid("component predicate removal"));
            }
            patch.added
        } else {
            vec![]
        };
        let mut partial = Vec::new();
        if let Some(v) = v.get("predicates") {
            for (k, v) in object(v)? {
                partial.push(ComponentPredicate::from_key(&registry(k), v)?);
            }
        }
        Ok(Self {
            exact,
            partial: BoundedList(partial),
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let exact = if let Some(v) = v.as_compound().and_then(|v| v.get("components")) {
            DataComponentPatch::from_nbt(v)?.added
        } else {
            vec![]
        };
        let mut partial = Vec::new();
        if let Some(v) = v.as_compound().and_then(|v| v.get("predicates")) {
            for (k, v) in v.as_compound().ok_or_else(|| invalid("predicates"))?.iter() {
                partial.push(ComponentPredicate::from_key_nbt(
                    &registry(&k.to_string_lossy()),
                    v,
                )?);
            }
        }
        Ok(Self {
            exact,
            partial: BoundedList(partial),
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        if !self.exact.is_empty() {
            v.push((
                "components".into(),
                PersistentValue::Map(
                    self.exact
                        .iter()
                        .map(|c| Ok((c.component_type().name().into(), c.to_persistent()?)))
                        .collect::<Result<_>>()?,
                ),
            ));
        }
        if !self.partial.0.is_empty() {
            v.push((
                "predicates".into(),
                PersistentValue::Map(
                    self.partial
                        .0
                        .iter()
                        .map(|p| Ok((p.key().to_string(), p.to_persistent()?)))
                        .collect::<Result<_>>()?,
                ),
            ));
        }
        Ok(PersistentValue::Map(v))
    }
}

impl Persistent for IntBounds {
    fn from_json(v: &Json) -> Result<Self> {
        let result = if v.is_number() {
            let n = i32::from_json(v)?;
            Self {
                min: Some(n),
                max: Some(n),
            }
        } else {
            object(v)?;
            Self {
                min: field(v, "min", Some(None))?,
                max: field(v, "max", Some(None))?,
            }
        };
        result.to_persistent()?;
        Ok(result)
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let result = if matches!(
            v,
            Tag::Byte(_)
                | Tag::Short(_)
                | Tag::Int(_)
                | Tag::Long(_)
                | Tag::Float(_)
                | Tag::Double(_)
        ) {
            let n = i32::from_nbt(v)?;
            Self {
                min: Some(n),
                max: Some(n),
            }
        } else {
            if v.as_compound().is_none() {
                return Err(invalid("integer bounds"));
            }
            Self {
                min: field_nbt(v, "min", Some(None))?,
                max: field_nbt(v, "max", Some(None))?,
            }
        };
        result.to_persistent()?;
        Ok(result)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min > max
        {
            return Err(invalid("swapped bounds"));
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min == max
        {
            return min.to_persistent();
        }
        let mut v = Vec::new();
        entry(&mut v, "min", &self.min, Some(None))?;
        entry(&mut v, "max", &self.max, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for LockCode {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            items: field(v, "items", Some(None))?,
            count: field(v, "count", Some(IntBounds::default()))?,
            components: DataComponentMatchers::from_json(v)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            items: field_nbt(v, "items", Some(None))?,
            count: field_nbt(v, "count", Some(IntBounds::default()))?,
            components: DataComponentMatchers::from_nbt(v)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.components.to_persistent()?)?;
        entry(&mut v, "items", &self.items, Some(None))?;
        entry(&mut v, "count", &self.count, Some(IntBounds::default()))?;
        Ok(PersistentValue::Map(v))
    }
}
persistent_record!(MapDecoration {decoration_type=>"type"[None],x=>"x"[None],z=>"z"[None],rotation=>"rotation"[None]});
persistent_record!(SeededContainerLoot {loot_table=>"loot_table"[None],seed=>"seed"[Some(0i64)]});
impl Persistent for MapDecorations {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(
            object(v)?
                .iter()
                .map(|(k, v)| Ok((k.clone(), MapDecoration::from_json(v)?)))
                .collect::<Result<_>>()?,
        ))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(
            v.as_compound()
                .ok_or_else(|| invalid("component object"))?
                .iter()
                .map(|(k, v)| Ok((k.to_string_lossy(), MapDecoration::from_nbt(v)?)))
                .collect::<Result<_>>()?,
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.to_persistent()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
impl Persistent for DebugStickState {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(
            object(v)?
                .iter()
                .map(|(k, v)| {
                    Ok((
                        ferraloom_data::Block::from_name(registry(k).as_str())
                            .ok_or_else(|| invalid("debug stick block"))?,
                        String::from_json(v)?,
                    ))
                })
                .collect::<Result<_>>()?,
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(k, v)| Ok((k.name().into(), v.to_persistent()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
impl Persistent for Recipes {
    fn from_json(v: &Json) -> Result<Self> {
        Vec::<Identifier>::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for AdventureModePredicate {
    fn from_json(v: &Json) -> Result<Self> {
        let predicates = if v.is_array() {
            Vec::<BlockPredicate>::from_json(v)?
        } else {
            vec![BlockPredicate::from_json(v)?]
        };
        if predicates.is_empty() {
            return Err(invalid("empty adventure predicate"));
        }
        Ok(Self { predicates })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let predicates = if matches!(v, Tag::List(_)) {
            Vec::<BlockPredicate>::from_nbt(v)?
        } else {
            vec![BlockPredicate::from_nbt(v)?]
        };
        if predicates.is_empty() {
            return Err(invalid("empty adventure predicate"));
        }
        Ok(Self { predicates })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if self.predicates.len() == 1 {
            self.predicates[0].to_persistent()
        } else {
            self.predicates.to_persistent()
        }
    }
}
impl Persistent for BlockPredicate {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            blocks: field(v, "blocks", Some(None))?,
            properties: field(v, "state", Some(None))?,
            nbt: field(v, "nbt", Some(None))?,
            components: DataComponentMatchers::from_json(v)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            blocks: field_nbt(v, "blocks", Some(None))?,
            properties: field_nbt(v, "state", Some(None))?,
            nbt: field_nbt(v, "nbt", Some(None))?,
            components: DataComponentMatchers::from_nbt(v)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.components.to_persistent()?)?;
        entry(&mut v, "blocks", &self.blocks, Some(None))?;
        entry(&mut v, "state", &self.properties, Some(None))?;
        entry(&mut v, "nbt", &self.nbt, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for StatePropertiesPredicate {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            properties: object(v)?
                .iter()
                .map(|(k, v)| {
                    Ok(PropertyMatcher {
                        name: k.clone(),
                        matcher: Either::from_json(v)?,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.properties
                .iter()
                .map(|v| Ok((v.name.clone(), v.matcher.to_persistent()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
fn container_from_json(v: &Json) -> Result<BoundedList<Option<ItemStackTemplate>, 256>> {
    let slots = v.as_array().ok_or_else(|| invalid("container slots"))?;
    if slots.len() > 256 {
        return Err(invalid("container length"));
    }
    let mut items = Vec::new();
    for v in slots {
        let slot = field::<i32>(v, "slot", None)?;
        if !(0..256).contains(&slot) {
            return Err(invalid("container slot"));
        }
        let slot = slot as usize;
        if items.len() <= slot {
            items.resize_with(slot + 1, || None);
        }
        items[slot] = Some(field(v, "item", None)?);
    }
    Ok(BoundedList(items))
}
fn container_value(v: &BoundedList<Option<ItemStackTemplate>, 256>) -> Result<PersistentValue> {
    if v.0.len() > 256 {
        return Err(invalid("container length"));
    }
    Ok(PersistentValue::List(
        v.0.iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
            .map(|(i, v)| {
                Ok(PersistentValue::Map(vec![
                    ("slot".into(), PersistentValue::Int(i as i32)),
                    ("item".into(), v.to_persistent()?),
                ]))
            })
            .collect::<Result<_>>()?,
    ))
}

pub(super) fn component_from_json(key: &Identifier, value: &Json) -> Result<DataComponent> {
    let value = match key.as_str() {
        "minecraft:custom_data" => {
            DataComponent::CustomData(<CustomData as Persistent>::from_json(value)?)
        }
        "minecraft:max_stack_size" => {
            DataComponent::MaxStackSize(<VarInt as Persistent>::from_json(value)?)
        }
        "minecraft:max_damage" => {
            DataComponent::MaxDamage(<VarInt as Persistent>::from_json(value)?)
        }
        "minecraft:damage" => DataComponent::Damage(<VarInt as Persistent>::from_json(value)?),
        "minecraft:unbreakable" => {
            DataComponent::Unbreakable(<Unit as Persistent>::from_json(value)?)
        }
        "minecraft:use_effects" => {
            DataComponent::UseEffects(<UseEffects as Persistent>::from_json(value)?)
        }
        "minecraft:custom_name" => {
            DataComponent::CustomName(<Box<Component> as Persistent>::from_json(value)?)
        }
        "minecraft:minimum_attack_charge" => {
            DataComponent::MinimumAttackCharge(<JavaF32 as Persistent>::from_json(value)?)
        }
        "minecraft:damage_type" => {
            DataComponent::DamageType(<DamageType as Persistent>::from_json(value)?)
        }
        "minecraft:item_name" => {
            DataComponent::ItemName(<Box<Component> as Persistent>::from_json(value)?)
        }
        "minecraft:item_model" => {
            DataComponent::ItemModel(<Identifier as Persistent>::from_json(value)?)
        }
        "minecraft:lore" => DataComponent::Lore(
            <BoundedList<Component, 256> as Persistent>::from_json(value)?,
        ),
        "minecraft:rarity" => DataComponent::Rarity(<Rarity as Persistent>::from_json(value)?),
        "minecraft:enchantments" => {
            DataComponent::Enchantments(<ItemEnchantments as Persistent>::from_json(value)?)
        }
        "minecraft:can_place_on" => {
            DataComponent::CanPlaceOn(<AdventureModePredicate as Persistent>::from_json(value)?)
        }
        "minecraft:can_break" => {
            DataComponent::CanBreak(<AdventureModePredicate as Persistent>::from_json(value)?)
        }
        "minecraft:attribute_modifiers" => DataComponent::AttributeModifiers(
            <Vec<AttributeEntry> as Persistent>::from_json(value)?,
        ),
        "minecraft:custom_model_data" => {
            DataComponent::CustomModelData(<CustomModelData as Persistent>::from_json(value)?)
        }
        "minecraft:tooltip_display" => {
            DataComponent::TooltipDisplay(<TooltipDisplay as Persistent>::from_json(value)?)
        }
        "minecraft:repair_cost" => {
            DataComponent::RepairCost(<VarInt as Persistent>::from_json(value)?)
        }
        "minecraft:creative_slot_lock" => return Err(invalid("transient component")),
        "minecraft:enchantment_glint_override" => {
            DataComponent::EnchantmentGlintOverride(<bool as Persistent>::from_json(value)?)
        }
        "minecraft:intangible_projectile" => {
            DataComponent::IntangibleProjectile(<Unit as Persistent>::from_json(value)?)
        }
        "minecraft:food" => DataComponent::Food(<FoodProperties as Persistent>::from_json(value)?),
        "minecraft:consumable" => {
            DataComponent::Consumable(<Consumable as Persistent>::from_json(value)?)
        }
        "minecraft:use_remainder" => {
            DataComponent::UseRemainder(<ItemStackTemplate as Persistent>::from_json(value)?)
        }
        "minecraft:use_cooldown" => {
            DataComponent::UseCooldown(<UseCooldown as Persistent>::from_json(value)?)
        }
        "minecraft:damage_resistant" => {
            DataComponent::DamageResistant(field(value, "types", None)?)
        }
        "minecraft:tool" => DataComponent::Tool(<Tool as Persistent>::from_json(value)?),
        "minecraft:weapon" => DataComponent::Weapon(<Weapon as Persistent>::from_json(value)?),
        "minecraft:attack_range" => {
            DataComponent::AttackRange(<AttackRange as Persistent>::from_json(value)?)
        }
        "minecraft:enchantable" => DataComponent::Enchantable(field(value, "value", None)?),
        "minecraft:equippable" => {
            DataComponent::Equippable(<Equippable as Persistent>::from_json(value)?)
        }
        "minecraft:repairable" => DataComponent::Repairable(field(value, "items", None)?),
        "minecraft:glider" => DataComponent::Glider(<Unit as Persistent>::from_json(value)?),
        "minecraft:tooltip_style" => {
            DataComponent::TooltipStyle(<Identifier as Persistent>::from_json(value)?)
        }
        "minecraft:death_protection" => {
            DataComponent::DeathProtection(<DeathProtection as Persistent>::from_json(value)?)
        }
        "minecraft:blocks_attacks" => {
            DataComponent::BlocksAttacks(<BlocksAttacks as Persistent>::from_json(value)?)
        }
        "minecraft:piercing_weapon" => {
            DataComponent::PiercingWeapon(<PiercingWeapon as Persistent>::from_json(value)?)
        }
        "minecraft:kinetic_weapon" => {
            DataComponent::KineticWeapon(<KineticWeapon as Persistent>::from_json(value)?)
        }
        "minecraft:attack_animation" => {
            DataComponent::AttackAnimation(<SwingAnimation as Persistent>::from_json(value)?)
        }
        "minecraft:interact_animation" => {
            DataComponent::InteractAnimation(<SwingAnimation as Persistent>::from_json(value)?)
        }
        "minecraft:additional_trade_cost" => return Err(invalid("transient component")),
        "minecraft:block_transformer" => DataComponent::BlockTransformer(
            <BlockTransformerHolder as Persistent>::from_json(value)?,
        ),
        "minecraft:villager_food" => {
            DataComponent::VillagerFood(<VillagerFood as Persistent>::from_json(value)?)
        }
        "minecraft:stored_enchantments" => {
            DataComponent::StoredEnchantments(<ItemEnchantments as Persistent>::from_json(value)?)
        }
        "minecraft:dye" => DataComponent::Dye(<DyeColor as Persistent>::from_json(value)?),
        "minecraft:dyed_color" => DataComponent::DyedColor(<i32 as Persistent>::from_json(value)?),
        "minecraft:map_id" => DataComponent::MapId(<VarInt as Persistent>::from_json(value)?),
        "minecraft:map_decorations" => {
            DataComponent::MapDecorations(<MapDecorations as Persistent>::from_json(value)?)
        }
        "minecraft:map_post_processing" => return Err(invalid("transient component")),
        "minecraft:charged_projectiles" => {
            DataComponent::ChargedProjectiles(
                <BoundedList<ItemStackTemplate, 1024> as Persistent>::from_json(value)?,
            )
        }
        "minecraft:bundle_contents" => {
            DataComponent::BundleContents(<Vec<ItemStackTemplate> as Persistent>::from_json(value)?)
        }
        "minecraft:potion_contents" => {
            DataComponent::PotionContents(<PotionContents as Persistent>::from_json(value)?)
        }
        "minecraft:potion_duration_scale" => {
            DataComponent::PotionDurationScale(<JavaF32 as Persistent>::from_json(value)?)
        }
        "minecraft:suspicious_stew_effects" => {
            DataComponent::SuspiciousStewEffects(<Vec<StewEffect> as Persistent>::from_json(value)?)
        }
        "minecraft:writable_book_content" => {
            DataComponent::WritableBookContent(field(value, "pages", Some(BoundedList(vec![])))?)
        }
        "minecraft:written_book_content" => {
            DataComponent::WrittenBookContent(<WrittenBookContent as Persistent>::from_json(value)?)
        }
        "minecraft:trim" => DataComponent::Trim(<ArmorTrim as Persistent>::from_json(value)?),
        "minecraft:debug_stick_state" => {
            DataComponent::DebugStickState(<DebugStickState as Persistent>::from_json(value)?)
        }
        "minecraft:entity_data" => {
            DataComponent::EntityData(<TypedEntityData as Persistent>::from_json(value)?)
        }
        "minecraft:bucket_entity_data" => {
            DataComponent::BucketEntityData(<CustomData as Persistent>::from_json(value)?)
        }
        "minecraft:block_entity_data" => {
            DataComponent::BlockEntityData(<TypedBlockEntityData as Persistent>::from_json(value)?)
        }
        "minecraft:instrument" => {
            DataComponent::Instrument(<InstrumentHolder as Persistent>::from_json(value)?)
        }
        "minecraft:provides_trim_material" => DataComponent::ProvidesTrimMaterial(
            <TrimMaterialHolder as Persistent>::from_json(value)?,
        ),
        "minecraft:ominous_bottle_amplifier" => {
            DataComponent::OminousBottleAmplifier(<VarInt as Persistent>::from_json(value)?)
        }
        "minecraft:jukebox_playable" => {
            DataComponent::JukeboxPlayable(<JukeboxSongHolder as Persistent>::from_json(value)?)
        }
        "minecraft:provides_banner_patterns" => DataComponent::ProvidesBannerPatterns(
            <BannerPatternSet as Persistent>::from_json(value)?,
        ),
        "minecraft:recipes" => DataComponent::Recipes(<Recipes as Persistent>::from_json(value)?),
        "minecraft:lodestone_tracker" => {
            DataComponent::LodestoneTracker(<LodestoneTracker as Persistent>::from_json(value)?)
        }
        "minecraft:firework_explosion" => {
            DataComponent::FireworkExplosion(<FireworkExplosion as Persistent>::from_json(value)?)
        }
        "minecraft:fireworks" => {
            DataComponent::Fireworks(<Fireworks as Persistent>::from_json(value)?)
        }
        "minecraft:profile" => {
            DataComponent::Profile(<ResolvableProfile as Persistent>::from_json(value)?)
        }
        "minecraft:note_block_sound" => {
            DataComponent::NoteBlockSound(<Identifier as Persistent>::from_json(value)?)
        }
        "minecraft:banner_patterns" => {
            DataComponent::BannerPatterns(<Vec<BannerLayer> as Persistent>::from_json(value)?)
        }
        "minecraft:base_color" => {
            DataComponent::BaseColor(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:pot_decorations" => {
            DataComponent::PotDecorations(<PotDecorations as Persistent>::from_json(value)?)
        }
        "minecraft:container" => DataComponent::Container(container_from_json(value)?),
        "minecraft:block_state" => {
            DataComponent::BlockState(<BlockItemStateProperties as Persistent>::from_json(value)?)
        }
        "minecraft:bees" => {
            DataComponent::Bees(<Vec<BeeOccupant> as Persistent>::from_json(value)?)
        }
        "minecraft:sulfur_cube_content" => {
            DataComponent::SulfurCubeContent(<ItemStackTemplate as Persistent>::from_json(value)?)
        }
        "minecraft:lock" => DataComponent::Lock(<LockCode as Persistent>::from_json(value)?),
        "minecraft:container_loot" => {
            DataComponent::ContainerLoot(<SeededContainerLoot as Persistent>::from_json(value)?)
        }
        "minecraft:break_sound" => {
            DataComponent::BreakSound(<Sound as Persistent>::from_json(value)?)
        }
        "minecraft:compostable" => {
            DataComponent::Compostable(<Compostable as Persistent>::from_json(value)?)
        }
        "minecraft:cooking_fuel" => {
            DataComponent::CookingFuel(<CookingFuel as Persistent>::from_json(value)?)
        }
        "minecraft:brewing_fuel" => {
            DataComponent::BrewingFuel(<BrewingFuel as Persistent>::from_json(value)?)
        }
        "minecraft:mob_visibility" => {
            DataComponent::MobVisibility(<MobVisibility as Persistent>::from_json(value)?)
        }
        "minecraft:villager/variant" => {
            DataComponent::VillagerVariant(<VillagerType as Persistent>::from_json(value)?)
        }
        "minecraft:wolf/variant" => {
            DataComponent::WolfVariant(<WolfVariant as Persistent>::from_json(value)?)
        }
        "minecraft:wolf/sound_variant" => {
            DataComponent::WolfSoundVariant(<WolfSoundVariant as Persistent>::from_json(value)?)
        }
        "minecraft:wolf/collar" => {
            DataComponent::WolfCollar(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:fox/variant" => {
            DataComponent::FoxVariant(<FoxVariant as Persistent>::from_json(value)?)
        }
        "minecraft:salmon/size" => {
            DataComponent::SalmonSize(<SalmonVariant as Persistent>::from_json(value)?)
        }
        "minecraft:parrot/variant" => {
            DataComponent::ParrotVariant(<ParrotVariant as Persistent>::from_json(value)?)
        }
        "minecraft:tropical_fish/pattern" => DataComponent::TropicalFishPattern(
            <TropicalFishPattern as Persistent>::from_json(value)?,
        ),
        "minecraft:tropical_fish/base_color" => {
            DataComponent::TropicalFishBaseColor(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:tropical_fish/pattern_color" => {
            DataComponent::TropicalFishPatternColor(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:mooshroom/variant" => {
            DataComponent::MooshroomVariant(<MooshroomVariant as Persistent>::from_json(value)?)
        }
        "minecraft:rabbit/variant" => {
            DataComponent::RabbitVariant(<RabbitVariant as Persistent>::from_json(value)?)
        }
        "minecraft:pig/variant" => {
            DataComponent::PigVariant(<PigVariant as Persistent>::from_json(value)?)
        }
        "minecraft:pig/sound_variant" => {
            DataComponent::PigSoundVariant(<PigSoundVariant as Persistent>::from_json(value)?)
        }
        "minecraft:cow/variant" => {
            DataComponent::CowVariant(<CowVariant as Persistent>::from_json(value)?)
        }
        "minecraft:cow/sound_variant" => {
            DataComponent::CowSoundVariant(<CowSoundVariant as Persistent>::from_json(value)?)
        }
        "minecraft:chicken/variant" => {
            DataComponent::ChickenVariant(<ChickenVariant as Persistent>::from_json(value)?)
        }
        "minecraft:chicken/sound_variant" => DataComponent::ChickenSoundVariant(
            <ChickenSoundVariant as Persistent>::from_json(value)?,
        ),
        "minecraft:zombie_nautilus/variant" => DataComponent::ZombieNautilusVariant(
            <ZombieNautilusVariant as Persistent>::from_json(value)?,
        ),
        "minecraft:frog/variant" => {
            DataComponent::FrogVariant(<FrogVariant as Persistent>::from_json(value)?)
        }
        "minecraft:horse/variant" => {
            DataComponent::HorseVariant(<HorseVariant as Persistent>::from_json(value)?)
        }
        "minecraft:painting/variant" => {
            DataComponent::PaintingVariant(<PaintingVariantHolder as Persistent>::from_json(value)?)
        }
        "minecraft:llama/variant" => {
            DataComponent::LlamaVariant(<LlamaVariant as Persistent>::from_json(value)?)
        }
        "minecraft:axolotl/variant" => {
            DataComponent::AxolotlVariant(<AxolotlVariant as Persistent>::from_json(value)?)
        }
        "minecraft:cat/variant" => {
            DataComponent::CatVariant(<CatVariant as Persistent>::from_json(value)?)
        }
        "minecraft:cat/sound_variant" => {
            DataComponent::CatSoundVariant(<CatSoundVariant as Persistent>::from_json(value)?)
        }
        "minecraft:cat/collar" => {
            DataComponent::CatCollar(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:sheep/color" => {
            DataComponent::SheepColor(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:shulker/color" => {
            DataComponent::ShulkerColor(<DyeColor as Persistent>::from_json(value)?)
        }
        "minecraft:provides_pottery_pattern" => DataComponent::ProvidesPotteryPattern(
            <DecoratedPotPattern as Persistent>::from_json(value)?,
        ),
        "minecraft:sign_text_front" => {
            DataComponent::SignTextFront(<Box<SignText> as Persistent>::from_json(value)?)
        }
        "minecraft:sign_text_back" => {
            DataComponent::SignTextBack(<Box<SignText> as Persistent>::from_json(value)?)
        }
        "minecraft:waxed" => DataComponent::Waxed(<Unit as Persistent>::from_json(value)?),
        "minecraft:cushion/color" => {
            DataComponent::CushionColor(<DyeColor as Persistent>::from_json(value)?)
        }
        _ => return Err(Error::invalid("component", key.to_string())),
    };
    validate_component(&value)?;
    Ok(value)
}
pub(super) fn component_from_nbt(key: &Identifier, value: &Tag) -> Result<DataComponent> {
    Ok(match key.as_str() {
        "minecraft:custom_data" => {
            DataComponent::CustomData(<CustomData as Persistent>::from_nbt(value)?)
        }
        "minecraft:max_stack_size" => {
            DataComponent::MaxStackSize(<VarInt as Persistent>::from_nbt(value)?)
        }
        "minecraft:max_damage" => {
            DataComponent::MaxDamage(<VarInt as Persistent>::from_nbt(value)?)
        }
        "minecraft:damage" => DataComponent::Damage(<VarInt as Persistent>::from_nbt(value)?),
        "minecraft:unbreakable" => {
            DataComponent::Unbreakable(<Unit as Persistent>::from_nbt(value)?)
        }
        "minecraft:use_effects" => {
            DataComponent::UseEffects(<UseEffects as Persistent>::from_nbt(value)?)
        }
        "minecraft:custom_name" => {
            DataComponent::CustomName(<Box<Component> as Persistent>::from_nbt(value)?)
        }
        "minecraft:minimum_attack_charge" => {
            DataComponent::MinimumAttackCharge(<JavaF32 as Persistent>::from_nbt(value)?)
        }
        "minecraft:damage_type" => {
            DataComponent::DamageType(<DamageType as Persistent>::from_nbt(value)?)
        }
        "minecraft:item_name" => {
            DataComponent::ItemName(<Box<Component> as Persistent>::from_nbt(value)?)
        }
        "minecraft:item_model" => {
            DataComponent::ItemModel(<Identifier as Persistent>::from_nbt(value)?)
        }
        "minecraft:lore" => DataComponent::Lore(
            <BoundedList<Component, 256> as Persistent>::from_nbt(value)?,
        ),
        "minecraft:rarity" => DataComponent::Rarity(<Rarity as Persistent>::from_nbt(value)?),
        "minecraft:enchantments" => {
            DataComponent::Enchantments(<ItemEnchantments as Persistent>::from_nbt(value)?)
        }
        "minecraft:can_place_on" => {
            DataComponent::CanPlaceOn(<AdventureModePredicate as Persistent>::from_nbt(value)?)
        }
        "minecraft:can_break" => {
            DataComponent::CanBreak(<AdventureModePredicate as Persistent>::from_nbt(value)?)
        }
        "minecraft:attribute_modifiers" => {
            DataComponent::AttributeModifiers(<Vec<AttributeEntry> as Persistent>::from_nbt(value)?)
        }
        "minecraft:custom_model_data" => {
            DataComponent::CustomModelData(<CustomModelData as Persistent>::from_nbt(value)?)
        }
        "minecraft:tooltip_display" => {
            DataComponent::TooltipDisplay(<TooltipDisplay as Persistent>::from_nbt(value)?)
        }
        "minecraft:repair_cost" => {
            DataComponent::RepairCost(<VarInt as Persistent>::from_nbt(value)?)
        }
        "minecraft:creative_slot_lock" => return Err(invalid("transient component")),
        "minecraft:enchantment_glint_override" => {
            DataComponent::EnchantmentGlintOverride(<bool as Persistent>::from_nbt(value)?)
        }
        "minecraft:intangible_projectile" => {
            DataComponent::IntangibleProjectile(<Unit as Persistent>::from_nbt(value)?)
        }
        "minecraft:food" => DataComponent::Food(<FoodProperties as Persistent>::from_nbt(value)?),
        "minecraft:consumable" => {
            DataComponent::Consumable(<Consumable as Persistent>::from_nbt(value)?)
        }
        "minecraft:use_remainder" => {
            DataComponent::UseRemainder(<ItemStackTemplate as Persistent>::from_nbt(value)?)
        }
        "minecraft:use_cooldown" => {
            DataComponent::UseCooldown(<UseCooldown as Persistent>::from_nbt(value)?)
        }
        "minecraft:damage_resistant" => {
            DataComponent::DamageResistant(field_nbt(value, "types", None)?)
        }
        "minecraft:tool" => DataComponent::Tool(<Tool as Persistent>::from_nbt(value)?),
        "minecraft:weapon" => DataComponent::Weapon(<Weapon as Persistent>::from_nbt(value)?),
        "minecraft:attack_range" => {
            DataComponent::AttackRange(<AttackRange as Persistent>::from_nbt(value)?)
        }
        "minecraft:enchantable" => DataComponent::Enchantable(field_nbt(value, "value", None)?),
        "minecraft:equippable" => {
            DataComponent::Equippable(<Equippable as Persistent>::from_nbt(value)?)
        }
        "minecraft:repairable" => DataComponent::Repairable(field_nbt(value, "items", None)?),
        "minecraft:glider" => DataComponent::Glider(<Unit as Persistent>::from_nbt(value)?),
        "minecraft:tooltip_style" => {
            DataComponent::TooltipStyle(<Identifier as Persistent>::from_nbt(value)?)
        }
        "minecraft:death_protection" => {
            DataComponent::DeathProtection(<DeathProtection as Persistent>::from_nbt(value)?)
        }
        "minecraft:blocks_attacks" => {
            DataComponent::BlocksAttacks(<BlocksAttacks as Persistent>::from_nbt(value)?)
        }
        "minecraft:piercing_weapon" => {
            DataComponent::PiercingWeapon(<PiercingWeapon as Persistent>::from_nbt(value)?)
        }
        "minecraft:kinetic_weapon" => {
            DataComponent::KineticWeapon(<KineticWeapon as Persistent>::from_nbt(value)?)
        }
        "minecraft:attack_animation" => {
            DataComponent::AttackAnimation(<SwingAnimation as Persistent>::from_nbt(value)?)
        }
        "minecraft:interact_animation" => {
            DataComponent::InteractAnimation(<SwingAnimation as Persistent>::from_nbt(value)?)
        }
        "minecraft:additional_trade_cost" => return Err(invalid("transient component")),
        "minecraft:block_transformer" => DataComponent::BlockTransformer(
            <BlockTransformerHolder as Persistent>::from_nbt(value)?,
        ),
        "minecraft:villager_food" => {
            DataComponent::VillagerFood(<VillagerFood as Persistent>::from_nbt(value)?)
        }
        "minecraft:stored_enchantments" => {
            DataComponent::StoredEnchantments(<ItemEnchantments as Persistent>::from_nbt(value)?)
        }
        "minecraft:dye" => DataComponent::Dye(<DyeColor as Persistent>::from_nbt(value)?),
        "minecraft:dyed_color" => DataComponent::DyedColor(<i32 as Persistent>::from_nbt(value)?),
        "minecraft:map_id" => DataComponent::MapId(<VarInt as Persistent>::from_nbt(value)?),
        "minecraft:map_decorations" => {
            DataComponent::MapDecorations(<MapDecorations as Persistent>::from_nbt(value)?)
        }
        "minecraft:map_post_processing" => return Err(invalid("transient component")),
        "minecraft:charged_projectiles" => {
            DataComponent::ChargedProjectiles(
                <BoundedList<ItemStackTemplate, 1024> as Persistent>::from_nbt(value)?,
            )
        }
        "minecraft:bundle_contents" => {
            DataComponent::BundleContents(<Vec<ItemStackTemplate> as Persistent>::from_nbt(value)?)
        }
        "minecraft:potion_contents" => {
            DataComponent::PotionContents(<PotionContents as Persistent>::from_nbt(value)?)
        }
        "minecraft:potion_duration_scale" => {
            DataComponent::PotionDurationScale(<JavaF32 as Persistent>::from_nbt(value)?)
        }
        "minecraft:suspicious_stew_effects" => {
            DataComponent::SuspiciousStewEffects(<Vec<StewEffect> as Persistent>::from_nbt(value)?)
        }
        "minecraft:writable_book_content" => DataComponent::WritableBookContent(field_nbt(
            value,
            "pages",
            Some(BoundedList(vec![])),
        )?),
        "minecraft:written_book_content" => {
            DataComponent::WrittenBookContent(<WrittenBookContent as Persistent>::from_nbt(value)?)
        }
        "minecraft:trim" => DataComponent::Trim(<ArmorTrim as Persistent>::from_nbt(value)?),
        "minecraft:debug_stick_state" => {
            DataComponent::DebugStickState(<DebugStickState as Persistent>::from_nbt(value)?)
        }
        "minecraft:entity_data" => {
            DataComponent::EntityData(<TypedEntityData as Persistent>::from_nbt(value)?)
        }
        "minecraft:bucket_entity_data" => {
            DataComponent::BucketEntityData(<CustomData as Persistent>::from_nbt(value)?)
        }
        "minecraft:block_entity_data" => {
            DataComponent::BlockEntityData(<TypedBlockEntityData as Persistent>::from_nbt(value)?)
        }
        "minecraft:instrument" => {
            DataComponent::Instrument(<InstrumentHolder as Persistent>::from_nbt(value)?)
        }
        "minecraft:provides_trim_material" => DataComponent::ProvidesTrimMaterial(
            <TrimMaterialHolder as Persistent>::from_nbt(value)?,
        ),
        "minecraft:ominous_bottle_amplifier" => {
            DataComponent::OminousBottleAmplifier(<VarInt as Persistent>::from_nbt(value)?)
        }
        "minecraft:jukebox_playable" => {
            DataComponent::JukeboxPlayable(<JukeboxSongHolder as Persistent>::from_nbt(value)?)
        }
        "minecraft:provides_banner_patterns" => DataComponent::ProvidesBannerPatterns(
            <BannerPatternSet as Persistent>::from_nbt(value)?,
        ),
        "minecraft:recipes" => DataComponent::Recipes(<Recipes as Persistent>::from_nbt(value)?),
        "minecraft:lodestone_tracker" => {
            DataComponent::LodestoneTracker(<LodestoneTracker as Persistent>::from_nbt(value)?)
        }
        "minecraft:firework_explosion" => {
            DataComponent::FireworkExplosion(<FireworkExplosion as Persistent>::from_nbt(value)?)
        }
        "minecraft:fireworks" => {
            DataComponent::Fireworks(<Fireworks as Persistent>::from_nbt(value)?)
        }
        "minecraft:profile" => {
            DataComponent::Profile(<ResolvableProfile as Persistent>::from_nbt(value)?)
        }
        "minecraft:note_block_sound" => {
            DataComponent::NoteBlockSound(<Identifier as Persistent>::from_nbt(value)?)
        }
        "minecraft:banner_patterns" => {
            DataComponent::BannerPatterns(<Vec<BannerLayer> as Persistent>::from_nbt(value)?)
        }
        "minecraft:base_color" => {
            DataComponent::BaseColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:pot_decorations" => {
            DataComponent::PotDecorations(<PotDecorations as Persistent>::from_nbt(value)?)
        }
        "minecraft:container" => DataComponent::Container(container_from_nbt(value)?),
        "minecraft:block_state" => {
            DataComponent::BlockState(<BlockItemStateProperties as Persistent>::from_nbt(value)?)
        }
        "minecraft:bees" => DataComponent::Bees(<Vec<BeeOccupant> as Persistent>::from_nbt(value)?),
        "minecraft:sulfur_cube_content" => {
            DataComponent::SulfurCubeContent(<ItemStackTemplate as Persistent>::from_nbt(value)?)
        }
        "minecraft:lock" => DataComponent::Lock(<LockCode as Persistent>::from_nbt(value)?),
        "minecraft:container_loot" => {
            DataComponent::ContainerLoot(<SeededContainerLoot as Persistent>::from_nbt(value)?)
        }
        "minecraft:break_sound" => {
            DataComponent::BreakSound(<Sound as Persistent>::from_nbt(value)?)
        }
        "minecraft:compostable" => {
            DataComponent::Compostable(<Compostable as Persistent>::from_nbt(value)?)
        }
        "minecraft:cooking_fuel" => {
            DataComponent::CookingFuel(<CookingFuel as Persistent>::from_nbt(value)?)
        }
        "minecraft:brewing_fuel" => {
            DataComponent::BrewingFuel(<BrewingFuel as Persistent>::from_nbt(value)?)
        }
        "minecraft:mob_visibility" => {
            DataComponent::MobVisibility(<MobVisibility as Persistent>::from_nbt(value)?)
        }
        "minecraft:villager/variant" => {
            DataComponent::VillagerVariant(<VillagerType as Persistent>::from_nbt(value)?)
        }
        "minecraft:wolf/variant" => {
            DataComponent::WolfVariant(<WolfVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:wolf/sound_variant" => {
            DataComponent::WolfSoundVariant(<WolfSoundVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:wolf/collar" => {
            DataComponent::WolfCollar(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:fox/variant" => {
            DataComponent::FoxVariant(<FoxVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:salmon/size" => {
            DataComponent::SalmonSize(<SalmonVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:parrot/variant" => {
            DataComponent::ParrotVariant(<ParrotVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:tropical_fish/pattern" => DataComponent::TropicalFishPattern(
            <TropicalFishPattern as Persistent>::from_nbt(value)?,
        ),
        "minecraft:tropical_fish/base_color" => {
            DataComponent::TropicalFishBaseColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:tropical_fish/pattern_color" => {
            DataComponent::TropicalFishPatternColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:mooshroom/variant" => {
            DataComponent::MooshroomVariant(<MooshroomVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:rabbit/variant" => {
            DataComponent::RabbitVariant(<RabbitVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:pig/variant" => {
            DataComponent::PigVariant(<PigVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:pig/sound_variant" => {
            DataComponent::PigSoundVariant(<PigSoundVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:cow/variant" => {
            DataComponent::CowVariant(<CowVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:cow/sound_variant" => {
            DataComponent::CowSoundVariant(<CowSoundVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:chicken/variant" => {
            DataComponent::ChickenVariant(<ChickenVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:chicken/sound_variant" => DataComponent::ChickenSoundVariant(
            <ChickenSoundVariant as Persistent>::from_nbt(value)?,
        ),
        "minecraft:zombie_nautilus/variant" => DataComponent::ZombieNautilusVariant(
            <ZombieNautilusVariant as Persistent>::from_nbt(value)?,
        ),
        "minecraft:frog/variant" => {
            DataComponent::FrogVariant(<FrogVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:horse/variant" => {
            DataComponent::HorseVariant(<HorseVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:painting/variant" => {
            DataComponent::PaintingVariant(<PaintingVariantHolder as Persistent>::from_nbt(value)?)
        }
        "minecraft:llama/variant" => {
            DataComponent::LlamaVariant(<LlamaVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:axolotl/variant" => {
            DataComponent::AxolotlVariant(<AxolotlVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:cat/variant" => {
            DataComponent::CatVariant(<CatVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:cat/sound_variant" => {
            DataComponent::CatSoundVariant(<CatSoundVariant as Persistent>::from_nbt(value)?)
        }
        "minecraft:cat/collar" => {
            DataComponent::CatCollar(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:sheep/color" => {
            DataComponent::SheepColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:shulker/color" => {
            DataComponent::ShulkerColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        "minecraft:provides_pottery_pattern" => DataComponent::ProvidesPotteryPattern(
            <DecoratedPotPattern as Persistent>::from_nbt(value)?,
        ),
        "minecraft:sign_text_front" => {
            DataComponent::SignTextFront(<Box<SignText> as Persistent>::from_nbt(value)?)
        }
        "minecraft:sign_text_back" => {
            DataComponent::SignTextBack(<Box<SignText> as Persistent>::from_nbt(value)?)
        }
        "minecraft:waxed" => DataComponent::Waxed(<Unit as Persistent>::from_nbt(value)?),
        "minecraft:cushion/color" => {
            DataComponent::CushionColor(<DyeColor as Persistent>::from_nbt(value)?)
        }
        _ => return Err(invalid("component type")),
    })
}
pub(super) fn component_value(component: &DataComponent) -> Result<PersistentValue> {
    validate_component(component)?;
    match component {
        DataComponent::CustomData(value) => value.to_persistent(),
        DataComponent::MaxStackSize(value) => value.to_persistent(),
        DataComponent::MaxDamage(value) => value.to_persistent(),
        DataComponent::Damage(value) => value.to_persistent(),
        DataComponent::Unbreakable(value) => value.to_persistent(),
        DataComponent::UseEffects(value) => value.to_persistent(),
        DataComponent::CustomName(value) => value.to_persistent(),
        DataComponent::MinimumAttackCharge(value) => value.to_persistent(),
        DataComponent::DamageType(value) => value.to_persistent(),
        DataComponent::ItemName(value) => value.to_persistent(),
        DataComponent::ItemModel(value) => value.to_persistent(),
        DataComponent::Lore(value) => value.to_persistent(),
        DataComponent::Rarity(value) => value.to_persistent(),
        DataComponent::Enchantments(value) => value.to_persistent(),
        DataComponent::CanPlaceOn(value) => value.to_persistent(),
        DataComponent::CanBreak(value) => value.to_persistent(),
        DataComponent::AttributeModifiers(value) => value.to_persistent(),
        DataComponent::CustomModelData(value) => value.to_persistent(),
        DataComponent::TooltipDisplay(value) => value.to_persistent(),
        DataComponent::RepairCost(value) => value.to_persistent(),
        DataComponent::CreativeSlotLock(_) => Err(invalid("transient component")),
        DataComponent::EnchantmentGlintOverride(value) => value.to_persistent(),
        DataComponent::IntangibleProjectile(value) => value.to_persistent(),
        DataComponent::Food(value) => value.to_persistent(),
        DataComponent::Consumable(value) => value.to_persistent(),
        DataComponent::UseRemainder(value) => value.to_persistent(),
        DataComponent::UseCooldown(value) => value.to_persistent(),
        DataComponent::DamageResistant(value) => Ok(PersistentValue::Map(vec![(
            "types".into(),
            value.to_persistent()?,
        )])),
        DataComponent::Tool(value) => value.to_persistent(),
        DataComponent::Weapon(value) => value.to_persistent(),
        DataComponent::AttackRange(value) => value.to_persistent(),
        DataComponent::Enchantable(value) => Ok(PersistentValue::Map(vec![(
            "value".into(),
            value.to_persistent()?,
        )])),
        DataComponent::Equippable(value) => value.to_persistent(),
        DataComponent::Repairable(value) => Ok(PersistentValue::Map(vec![(
            "items".into(),
            value.to_persistent()?,
        )])),
        DataComponent::Glider(value) => value.to_persistent(),
        DataComponent::TooltipStyle(value) => value.to_persistent(),
        DataComponent::DeathProtection(value) => value.to_persistent(),
        DataComponent::BlocksAttacks(value) => value.to_persistent(),
        DataComponent::PiercingWeapon(value) => value.to_persistent(),
        DataComponent::KineticWeapon(value) => value.to_persistent(),
        DataComponent::AttackAnimation(value) => value.to_persistent(),
        DataComponent::InteractAnimation(value) => value.to_persistent(),
        DataComponent::AdditionalTradeCost(_) => Err(invalid("transient component")),
        DataComponent::BlockTransformer(value) => value.to_persistent(),
        DataComponent::VillagerFood(value) => value.to_persistent(),
        DataComponent::StoredEnchantments(value) => value.to_persistent(),
        DataComponent::Dye(value) => value.to_persistent(),
        DataComponent::DyedColor(value) => value.to_persistent(),
        DataComponent::MapId(value) => value.to_persistent(),
        DataComponent::MapDecorations(value) => value.to_persistent(),
        DataComponent::MapPostProcessing(_) => Err(invalid("transient component")),
        DataComponent::ChargedProjectiles(value) => value.to_persistent(),
        DataComponent::BundleContents(value) => value.to_persistent(),
        DataComponent::PotionContents(value) => value.to_persistent(),
        DataComponent::PotionDurationScale(value) => value.to_persistent(),
        DataComponent::SuspiciousStewEffects(value) => value.to_persistent(),
        DataComponent::WritableBookContent(value) => {
            if value.0.is_empty() {
                Ok(PersistentValue::Map(vec![]))
            } else {
                Ok(PersistentValue::Map(vec![(
                    "pages".into(),
                    value.to_persistent()?,
                )]))
            }
        }
        DataComponent::WrittenBookContent(value) => value.to_persistent(),
        DataComponent::Trim(value) => value.to_persistent(),
        DataComponent::DebugStickState(value) => value.to_persistent(),
        DataComponent::EntityData(value) => value.to_persistent(),
        DataComponent::BucketEntityData(value) => value.to_persistent(),
        DataComponent::BlockEntityData(value) => value.to_persistent(),
        DataComponent::Instrument(value) => value.to_persistent(),
        DataComponent::ProvidesTrimMaterial(value) => value.to_persistent(),
        DataComponent::OminousBottleAmplifier(value) => value.to_persistent(),
        DataComponent::JukeboxPlayable(value) => value.to_persistent(),
        DataComponent::ProvidesBannerPatterns(value) => value.to_persistent(),
        DataComponent::Recipes(value) => value.to_persistent(),
        DataComponent::LodestoneTracker(value) => value.to_persistent(),
        DataComponent::FireworkExplosion(value) => value.to_persistent(),
        DataComponent::Fireworks(value) => value.to_persistent(),
        DataComponent::Profile(value) => value.to_persistent(),
        DataComponent::NoteBlockSound(value) => value.to_persistent(),
        DataComponent::BannerPatterns(value) => value.to_persistent(),
        DataComponent::BaseColor(value) => value.to_persistent(),
        DataComponent::PotDecorations(value) => value.to_persistent(),
        DataComponent::Container(value) => container_value(value),
        DataComponent::BlockState(value) => value.to_persistent(),
        DataComponent::Bees(value) => value.to_persistent(),
        DataComponent::SulfurCubeContent(value) => value.to_persistent(),
        DataComponent::Lock(value) => value.to_persistent(),
        DataComponent::ContainerLoot(value) => value.to_persistent(),
        DataComponent::BreakSound(value) => value.to_persistent(),
        DataComponent::Compostable(value) => value.to_persistent(),
        DataComponent::CookingFuel(value) => value.to_persistent(),
        DataComponent::BrewingFuel(value) => value.to_persistent(),
        DataComponent::MobVisibility(value) => value.to_persistent(),
        DataComponent::VillagerVariant(value) => value.to_persistent(),
        DataComponent::WolfVariant(value) => value.to_persistent(),
        DataComponent::WolfSoundVariant(value) => value.to_persistent(),
        DataComponent::WolfCollar(value) => value.to_persistent(),
        DataComponent::FoxVariant(value) => value.to_persistent(),
        DataComponent::SalmonSize(value) => value.to_persistent(),
        DataComponent::ParrotVariant(value) => value.to_persistent(),
        DataComponent::TropicalFishPattern(value) => value.to_persistent(),
        DataComponent::TropicalFishBaseColor(value) => value.to_persistent(),
        DataComponent::TropicalFishPatternColor(value) => value.to_persistent(),
        DataComponent::MooshroomVariant(value) => value.to_persistent(),
        DataComponent::RabbitVariant(value) => value.to_persistent(),
        DataComponent::PigVariant(value) => value.to_persistent(),
        DataComponent::PigSoundVariant(value) => value.to_persistent(),
        DataComponent::CowVariant(value) => value.to_persistent(),
        DataComponent::CowSoundVariant(value) => value.to_persistent(),
        DataComponent::ChickenVariant(value) => value.to_persistent(),
        DataComponent::ChickenSoundVariant(value) => value.to_persistent(),
        DataComponent::ZombieNautilusVariant(value) => value.to_persistent(),
        DataComponent::FrogVariant(value) => value.to_persistent(),
        DataComponent::HorseVariant(value) => value.to_persistent(),
        DataComponent::PaintingVariant(value) => value.to_persistent(),
        DataComponent::LlamaVariant(value) => value.to_persistent(),
        DataComponent::AxolotlVariant(value) => value.to_persistent(),
        DataComponent::CatVariant(value) => value.to_persistent(),
        DataComponent::CatSoundVariant(value) => value.to_persistent(),
        DataComponent::CatCollar(value) => value.to_persistent(),
        DataComponent::SheepColor(value) => value.to_persistent(),
        DataComponent::ShulkerColor(value) => value.to_persistent(),
        DataComponent::ProvidesPotteryPattern(value) => value.to_persistent(),
        DataComponent::SignTextFront(value) => value.to_persistent(),
        DataComponent::SignTextBack(value) => value.to_persistent(),
        DataComponent::Waxed(value) => value.to_persistent(),
        DataComponent::CushionColor(value) => value.to_persistent(),
    }
}
fn validate_component(value: &DataComponent) -> Result<()> {
    match value {
        DataComponent::MaxStackSize(v) if !(1..=99).contains(&v.0) => {
            Err(invalid("max stack size"))
        }
        DataComponent::MaxDamage(v) | DataComponent::Enchantable(v) if v.0 <= 0 => {
            Err(invalid("positive component"))
        }
        DataComponent::Damage(v) | DataComponent::RepairCost(v) if v.0 < 0 => {
            Err(invalid("nonnegative component"))
        }
        DataComponent::MinimumAttackCharge(v) if !float_range(v.0, 0.0, 1.0) => {
            Err(invalid("minimum attack charge"))
        }
        DataComponent::PotionDurationScale(v) if !nonnegative_float(v) => {
            Err(invalid("potion duration scale"))
        }
        DataComponent::OminousBottleAmplifier(v) if !(0..=4).contains(&v.0) => {
            Err(invalid("ominous bottle amplifier"))
        }
        _ => Ok(()),
    }
}
impl Persistent for NbtPredicate {
    fn from_nbt(v: &Tag) -> Result<Self> {
        CustomData::from_nbt(v).map(Self)
    }
    fn from_json(v: &Json) -> Result<Self> {
        if let Some(s) = v.as_str() {
            let tag = ferraloom_nbt::parse_compound(s)
                .map_err(|e| Error::invalid("NBT predicate", e.to_string()))?;
            Ok(Self(CustomData(Tag::Compound(tag))))
        } else {
            CustomData::from_json(v).map(Self)
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.0.to_string().to_persistent()
    }
}

pub(super) use persistent_record;

fn container_from_nbt(v: &Tag) -> Result<BoundedList<Option<ItemStackTemplate>, 256>> {
    let slots = v.as_list().ok_or_else(|| invalid("container slots"))?;
    if slots.len() > 256 {
        return Err(invalid("container length"));
    }
    let mut items = Vec::new();
    for v in slots.iter() {
        let slot = field_nbt::<i32>(v, "slot", None)?;
        if !(0..256).contains(&slot) {
            return Err(invalid("container slot"));
        }
        let slot = slot as usize;
        if items.len() <= slot {
            items.resize_with(slot + 1, || None);
        }
        items[slot] = Some(field_nbt(v, "item", None)?);
    }
    Ok(BoundedList(items))
}
impl Persistent for ItemStack {
    fn from_json(value: &Json) -> Result<Self> {
        let item = field(value, "id", None)?;
        let count = field::<i32>(value, "count", Some(1))?;
        if !(1..=99).contains(&count) || item == ferraloom_data::Item::AIR {
            return Err(invalid("item stack"));
        }
        Ok(Self::new(
            item,
            count,
            field(value, "components", Some(DataComponentPatch::default()))?,
        ))
    }
    fn from_nbt(value: &Tag) -> Result<Self> {
        Self::decode_nbt_count(value, false)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let value = self
            .0
            .as_ref()
            .filter(|_| !self.is_empty())
            .ok_or_else(|| invalid("empty item stack"))?;
        if !(1..=99).contains(&value.count) {
            return Err(invalid("item stack count"));
        }
        let mut entries = vec![
            ("id".into(), value.item.to_persistent()?),
            ("count".into(), value.count.to_persistent()?),
        ];
        entry(
            &mut entries,
            "components",
            &value.components,
            Some(DataComponentPatch::default()),
        )?;
        Ok(PersistentValue::Map(entries))
    }
}

impl ItemStack {
    /// Port of TagValueInput#read(ItemStack.CODEC)'s count partial recovery.
    /// An invalid optional count has the MAP_CODEC partial value 1; strict
    /// Persistent decoding still rejects it. Item/components errors stay errors.
    pub fn from_nbt_with_partial(value: &Tag) -> Result<Self> {
        Self::decode_nbt_count(value, true)
    }

    fn decode_nbt_count(value: &Tag, recover_count: bool) -> Result<Self> {
        let item = field_nbt(value, "id", None)?;
        let count = match field_nbt::<i32>(value, "count", Some(1)) {
            Ok(count) if (1..=99).contains(&count) => count,
            _ if recover_count => 1,
            Err(error) => return Err(error),
            Ok(_) => return Err(invalid("item stack")),
        };
        if item == ferraloom_data::Item::AIR {
            return Err(invalid("item stack"));
        }
        Ok(Self::new(
            item,
            count,
            field_nbt(value, "components", Some(DataComponentPatch::default()))?,
        ))
    }
}

#[cfg(test)]
mod item_stack_partial_tests {
    use super::*;

    #[test]
    fn storage_count_partials_do_not_relax_strict_item_codecs() {
        for count in ["0", "100"] {
            let json: Json =
                serde_json::from_str(&format!("{{\"id\":\"stone\",\"count\":{count}}}")).unwrap();
            let tag = Tag::Compound(
                ferraloom_nbt::parse_compound(&format!("{{id:'stone',count:{count}}}")).unwrap(),
            );
            assert!(ItemStack::from_json(&json).is_err());
            assert!(ItemStack::from_nbt(&tag).is_err());
            assert_eq!(
                ItemStack::from_nbt_with_partial(&tag).unwrap(),
                ItemStack::new(
                    ferraloom_data::Item::STONE,
                    1,
                    DataComponentPatch::default()
                )
            );
        }
        for raw in [
            "{id:'missing_item',count:0}",
            "{id:'stone',count:0,components:{'minecraft:missing_component':1}}",
        ] {
            assert!(
                ItemStack::from_nbt_with_partial(&Tag::Compound(
                    ferraloom_nbt::parse_compound(raw).unwrap()
                ))
                .is_err()
            );
        }
    }
}
