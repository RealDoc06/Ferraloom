//! Persistent component codecs and HashOps' typed CRC32C representation.
use super::*;
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
    Boolean(bool),
    String(NbtString),
    Int(i32),
    Float(f32),
    Double(f64),
    List(Vec<PersistentValue>),
    Map(Vec<(String, PersistentValue)>),
}
impl PersistentValue {
    pub fn to_json(&self) -> Json {
        match self {
            Self::Nbt(v) => tag_json(v),
            Self::Boolean(v) => (*v).into(),
            Self::String(v) => v.to_string_lossy().into(),
            Self::Int(v) => (*v).into(),
            Self::Float(v) => Json::from(*v),
            Self::Double(v) => Json::from(*v),
            Self::List(v) => Json::Array(v.iter().map(Self::to_json).collect()),
            Self::Map(v) => Json::Object(v.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()),
        }
    }
    pub fn to_nbt(&self) -> Tag {
        match self {
            Self::Nbt(v) => v.clone(),
            Self::Boolean(v) => Tag::Byte(i8::from(*v)),
            Self::String(v) => Tag::String(v.clone()),
            Self::Int(v) => Tag::Int(*v),
            Self::Float(v) => Tag::Float(*v),
            Self::Double(v) => Tag::Double(*v),
            Self::List(v) => Tag::List(ListTag(v.iter().map(Self::to_nbt).collect())),
            Self::Map(v) => {
                let mut map = CompoundTag::new();
                for (k, v) in v {
                    map.insert(k.as_str(), v.to_nbt());
                }
                Tag::Compound(map)
            }
        }
    }
    /// Port of HashOps.CRC32C_INSTANCE; Guava primitive sinks use little-endian bytes.
    pub fn hash(&self) -> i32 {
        match self {
            Self::Nbt(v) => hash_tag(v),
            Self::Boolean(v) => crc32c(&[13, u8::from(*v)]),
            Self::String(v) => hash_string(v),
            Self::Int(v) => tagged(8, &v.to_le_bytes()),
            Self::Float(v) => tagged(10, &v.to_le_bytes()),
            Self::Double(v) => tagged(11, &v.to_le_bytes()),
            Self::List(v) => hash_list(v.iter().map(Self::hash)),
            Self::Map(v) => hash_map(
                v.iter()
                    .map(|(k, v)| (hash_string(&k.as_str().into()), v.hash())),
            ),
        }
    }
}
/// Port of a component's persistent Codec, shared by item defaults, storage and commands.
pub trait Persistent: Sized {
    fn from_json(value: &Json) -> Result<Self>;
    fn to_persistent(&self) -> Result<PersistentValue>;
    fn to_json(&self) -> Result<Json> {
        Ok(self.to_persistent()?.to_json())
    }
    fn from_nbt(value: &Tag) -> Result<Self> {
        Self::from_json(&tag_json(value))
    }
    fn to_nbt(&self) -> Result<Tag> {
        Ok(self.to_persistent()?.to_nbt())
    }
    fn hash(&self) -> Result<i32> {
        Ok(self.to_persistent()?.hash())
    }
}
fn crc32c(bytes: &[u8]) -> i32 {
    let mut crc = !0u32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc as i32
}
fn tagged(tag: u8, bytes: &[u8]) -> i32 {
    let mut v = vec![tag];
    v.extend(bytes);
    crc32c(&v)
}
fn hash_string(value: &NbtString) -> i32 {
    let mut v = vec![12];
    v.extend((value.len() as i32).to_le_bytes());
    for c in value.as_utf16() {
        v.extend(c.to_le_bytes());
    }
    crc32c(&v)
}
fn hash_list(values: impl IntoIterator<Item = i32>) -> i32 {
    let mut v = vec![4];
    for hash in values {
        v.extend(hash.to_le_bytes());
    }
    v.push(5);
    crc32c(&v)
}
fn hash_map(values: impl IntoIterator<Item = (i32, i32)>) -> i32 {
    let mut pairs: Vec<_> = values.into_iter().collect();
    pairs.sort_by_key(|(k, v)| (*k as u32, *v as u32));
    let mut v = vec![2];
    for (key, value) in pairs {
        v.extend(key.to_le_bytes());
        v.extend(value.to_le_bytes());
    }
    v.push(3);
    crc32c(&v)
}
fn hash_tag(tag: &Tag) -> i32 {
    match tag {
        Tag::End => crc32c(&[1]),
        Tag::Byte(v) => tagged(6, &v.to_le_bytes()),
        Tag::Short(v) => tagged(7, &v.to_le_bytes()),
        Tag::Int(v) => tagged(8, &v.to_le_bytes()),
        Tag::Long(v) => tagged(9, &v.to_le_bytes()),
        Tag::Float(v) => tagged(10, &v.to_le_bytes()),
        Tag::Double(v) => tagged(11, &v.to_le_bytes()),
        Tag::String(v) => hash_string(v),
        Tag::List(v) => hash_list(v.iter().map(hash_tag)),
        Tag::Compound(v) => hash_map(v.iter().map(|(k, v)| (hash_string(k), hash_tag(v)))),
        Tag::ByteArray(a) => {
            let mut v = vec![14];
            v.extend(a.iter().map(|b| *b as u8));
            v.push(15);
            crc32c(&v)
        }
        Tag::IntArray(a) => {
            let mut v = vec![16];
            for n in a {
                v.extend(n.to_le_bytes());
            }
            v.push(17);
            crc32c(&v)
        }
        Tag::LongArray(a) => {
            let mut v = vec![18];
            for n in a {
                v.extend(n.to_le_bytes());
            }
            v.push(19);
            crc32c(&v)
        }
    }
}
fn object(value: &Json) -> Result<&Map<String, Json>> {
    value.as_object().ok_or_else(|| invalid("component object"))
}
fn field<T: Persistent>(value: &Json, name: &str, default: Option<T>) -> Result<T> {
    match value.get(name) {
        Some(v) => T::from_json(v),
        None => default.ok_or_else(|| Error::invalid("component field", name)),
    }
}
fn entry<T: Persistent + PartialEq>(
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
macro_rules! persistent_record {
    ($ty:ident { $($field:ident => $name:literal [$default:expr]),* $(,)? }) => {
        impl Persistent for $ty {
            fn from_json(value:&Json)->Result<Self> { object(value)?; Ok(Self { $($field:field(value,$name,$default)?),* }) }
            fn to_persistent(&self)->Result<PersistentValue> {
                let mut entries=Vec::new(); $(entry(&mut entries,$name,&self.$field,$default)?;)* Ok(PersistentValue::Map(entries))
            }
        }
    };
}
impl Persistent for bool {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_bool()
            .or_else(|| v.as_i64().map(|n| n != 0))
            .ok_or_else(|| invalid("boolean"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Boolean(*self))
    }
}
impl Persistent for i32 {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_i64()
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| invalid("integer"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Int(*self))
    }
}
impl Persistent for VarInt {
    fn from_json(v: &Json) -> Result<Self> {
        i32::from_json(v).map(Self)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
}
impl Persistent for f32 {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_f64().map(|v| v as f32).ok_or_else(|| invalid("float"))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Float(*self))
    }
}
impl Persistent for f64 {
    fn from_json(v: &Json) -> Result<Self> {
        v.as_f64().ok_or_else(|| invalid("double"))
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        (**self).to_persistent()
    }
}
impl<L: Persistent, R: Persistent> Persistent for Either<L, R> {
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Nbt(self.to_nbt()?))
    }
}
fn tag_json(v: &Tag) -> Json {
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
            if let Some(n) = v.as_i64() {
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
                let n = v.as_f64().ok_or_else(|| invalid("number"))?;
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
        object(v)?;
        Ok(Self(json_tag(v)?))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        self.0.to_persistent()
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        if !matches!(v, Tag::Compound(_)) {
            return Err(invalid("compound tag"));
        }
        Ok(Self(v.clone()))
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
impl Persistent for ComponentPredicateType {
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
            Holder::Direct(SoundEvent::from_json(v)?)
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
            Holder::Direct(TrimMaterial::from_json(v)?)
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
            Holder::Direct(TrimPattern::from_json(v)?)
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
            Holder::Direct(Instrument::from_json(v)?)
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
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(JukeboxSong::from_json(v)?)
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
        }
    }
}
impl Persistent for BannerPatternHolder {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(BannerPattern::from_json(v)?)
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
        Ok(Self(if v.is_string() {
            Holder::Reference(Identifier::from_json(v)?)
        } else {
            Holder::Direct(PaintingVariant::from_json(v)?)
        }))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        match &self.0 {
            Holder::Reference(v) => v.to_persistent(),
            Holder::Direct(v) => v.to_persistent(),
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

fn set_from_json(v: &Json) -> Result<HolderSet> {
    if let Some(s) = v.as_str() {
        if let Some(tag) = s.strip_prefix('#') {
            return Ok(HolderSet::Named(Identifier::parse(tag)?));
        }
        return Ok(HolderSet::Direct(vec![Identifier::parse(s)?]));
    }
    Ok(HolderSet::Direct(Vec::<Identifier>::from_json(v)?))
}
fn set_value(v: &HolderSet) -> Result<PersistentValue> {
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
persistent_record!(Instrument {sound_event => "sound_event" [None],use_duration => "use_duration" [None],range => "range" [None],durability_damage => "durability_damage" [Some(VarInt(0))],description => "description" [None]});
persistent_record!(JukeboxSong {sound_event => "sound_event" [None],description => "description" [None],length_in_seconds => "length_in_seconds" [None],comparator_output => "comparator_output" [None]});
persistent_record!(BannerPattern {asset_id => "asset_id" [None],translation_key => "translation_key" [None]});
persistent_record!(PaintingVariant {width => "width" [None],height => "height" [None],asset_id => "asset_id" [None],title => "title" [Some(None)],author => "author" [Some(None)]});
persistent_record!(UseEffects {can_sprint => "can_sprint" [Some(false)],interact_vibrations => "interact_vibrations" [Some(true)],speed_multiplier => "speed_multiplier" [Some(0.2f32)]});
persistent_record!(FoodProperties {nutrition => "nutrition" [None],saturation => "saturation" [None],can_always_eat => "can_always_eat" [Some(false)]});
persistent_record!(Weapon {item_damage_per_attack => "item_damage_per_attack" [Some(VarInt(1))],disable_blocking_for_seconds => "disable_blocking_for_seconds" [Some(0.0f32)]});
persistent_record!(UseCooldown {seconds => "seconds" [None],cooldown_group => "cooldown_group" [Some(None)]});
persistent_record!(AttackRange {min_reach => "min_reach" [Some(0.0f32)],max_reach => "max_reach" [Some(3.0f32)],min_creative_reach => "min_creative_reach" [Some(0.0f32)],max_creative_reach => "max_creative_reach" [Some(5.0f32)],hitbox_margin => "hitbox_margin" [Some(0.3f32)],mob_factor => "mob_factor" [Some(1.0f32)]});
persistent_record!(Consumable {consume_seconds => "consume_seconds" [Some(1.6f32)],animation => "animation" [Some(ItemUseAnimation::Eat)],sound => "sound" [Some(sound("entity.generic.eat"))],has_consume_particles => "has_consume_particles" [Some(true)],on_consume_effects => "on_consume_effects" [Some(vec![])]});
persistent_record!(DeathProtection {death_effects => "death_effects" [Some(vec![])]});
persistent_record!(Tool {rules => "rules" [None],default_mining_speed => "default_mining_speed" [Some(1.0f32)],damage_per_block => "damage_per_block" [Some(VarInt(1))],can_destroy_blocks_in_creative => "can_destroy_blocks_in_creative" [Some(true)]});
persistent_record!(ToolRule {blocks => "blocks" [None],speed => "speed" [Some(None)],correct_for_drops => "correct_for_drops" [Some(None)]});
persistent_record!(Equippable {slot => "slot" [None],equip_sound => "equip_sound" [Some(sound("item.armor.equip_generic"))],asset_id => "asset_id" [Some(None)],camera_overlay => "camera_overlay" [Some(None)],allowed_entities => "allowed_entities" [Some(None)],dispensable => "dispensable" [Some(true)],swappable => "swappable" [Some(true)],damage_on_hurt => "damage_on_hurt" [Some(true)],equip_on_interact => "equip_on_interact" [Some(false)],can_be_sheared => "can_be_sheared" [Some(false)],shearing_sound => "shearing_sound" [Some(sound("item.shears.snip"))]});
persistent_record!(BlocksAttacks {block_delay_seconds => "block_delay_seconds" [Some(0.0f32)],disable_cooldown_scale => "disable_cooldown_scale" [Some(1.0f32)],damage_reductions => "damage_reductions" [Some(vec![DamageReduction {horizontal_blocking_angle:90.0,types:None,base:0.0,factor:1.0}])],item_damage => "item_damage" [Some(ItemDamageFunction {threshold:1.0,base:0.0,factor:1.0})],bypassed_by => "bypassed_by" [Some(None)],block_sound => "block_sound" [Some(None)],disable_sound => "disabled_sound" [Some(None)]});
persistent_record!(DamageReduction {horizontal_blocking_angle => "horizontal_blocking_angle" [Some(90.0f32)],types => "type" [Some(None)],base => "base" [None],factor => "factor" [None]});
persistent_record!(ItemDamageFunction {threshold => "threshold" [None],base => "base" [None],factor => "factor" [None]});
persistent_record!(PiercingWeapon {deals_knockback => "deals_knockback" [Some(true)],dismounts => "dismounts" [Some(false)],sound => "sound" [Some(None)],hit_sound => "hit_sound" [Some(None)]});
persistent_record!(KineticWeapon {contact_cooldown_ticks => "contact_cooldown_ticks" [Some(VarInt(10))],delay_ticks => "delay_ticks" [Some(VarInt(0))],dismount_conditions => "dismount_conditions" [Some(None)],knockback_conditions => "knockback_conditions" [Some(None)],damage_conditions => "damage_conditions" [Some(None)],forward_movement => "forward_movement" [Some(0.0f32)],damage_multiplier => "damage_multiplier" [Some(1.0f32)],sound => "sound" [Some(None)],hit_sound => "hit_sound" [Some(None)]});
persistent_record!(KineticCondition {max_duration_ticks => "max_duration_ticks" [None],min_speed => "min_speed" [Some(0.0f32)],min_relative_speed => "min_relative_speed" [Some(0.0f32)]});
persistent_record!(SwingAnimation {animation_type => "type" [Some(SwingAnimationType::Whack)],duration => "duration" [Some(VarInt(6))]});
persistent_record!(CustomModelData {floats => "floats" [Some(vec![])],flags => "flags" [Some(vec![])],strings => "strings" [Some(vec![])],colors => "colors" [Some(vec![])]});
persistent_record!(FireworkExplosion {shape => "shape" [None],colors => "colors" [Some(vec![])],fade_colors => "fade_colors" [Some(vec![])],has_trail => "has_trail" [Some(false)],has_twinkle => "has_twinkle" [Some(false)]});
persistent_record!(Fireworks {flight_duration => "flight_duration" [Some(VarInt(0))],explosions => "explosions" [Some(BoundedList(vec![]))]});
persistent_record!(LodestoneTracker {target => "target" [Some(None)],tracked => "tracked" [Some(true)]});
persistent_record!(ArmorTrim {material => "material" [None],pattern => "pattern" [None]});
persistent_record!(BannerLayer {pattern => "pattern" [None],color => "color" [None]});
persistent_record!(PotionContents {potion => "potion" [Some(None)],custom_color => "custom_color" [Some(None)],custom_effects => "custom_effects" [Some(vec![])],custom_name => "custom_name" [Some(None)]});
persistent_record!(StewEffect {effect => "id" [None],duration => "duration" [Some(VarInt(160))]});
persistent_record!(WrittenBookContent {title => "title" [None],author => "author" [None],generation => "generation" [Some(VarInt(0))],pages => "pages" [Some(vec![])],resolved => "resolved" [Some(false)]});
persistent_record!(BeeOccupant {entity_data => "entity_data" [None],ticks_in_hive => "ticks_in_hive" [None],min_ticks_in_hive => "min_ticks_in_hive" [None]});
persistent_record!(PotDecorations {back => "back" [Some(None)],left => "left" [Some(None)],right => "right" [Some(None)],front => "front" [Some(None)]});
persistent_record!(Compostable {layers => "layers" [None]});
persistent_record!(CookingFuel {burn_time => "burn_time" [None],speed_multiplier => "speed_multiplier" [None]});
persistent_record!(BrewingFuel {uses => "uses" [None],speed_multiplier => "speed_multiplier" [None]});
persistent_record!(MobVisibility {targeting_entity_types => "targeting_entity_types" [None],visibility => "visibility" [None]});
persistent_record!(TooltipDisplay {hide_tooltip => "hide_tooltip" [Some(false)],hidden_components => "hidden_components" [Some(ComponentTypeList {entries:vec![]})]});
persistent_record!(VillagerFood {nutrition => "nutrition" [None]});
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(k, v)| Ok((k.0.to_string(), v.to_persistent()?)))
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
    fn to_persistent(&self) -> Result<PersistentValue> {
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        for c in &self.added {
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
        v.as_i64().ok_or_else(|| invalid("long"))
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Nbt(Tag::IntArray(vec![
            (self.most >> 32) as i32,
            self.most as i32,
            (self.least >> 32) as i32,
            self.least as i32,
        ])))
    }
}
persistent_record!(ProfileProperty {name=>"name"[None],value=>"value"[None],signature=>"signature"[Some(None)]});
persistent_record!(GameProfile {id=>"id"[None],name=>"name"[None],properties=>"properties"[Some(BoundedList(vec![]))]});
persistent_record!(PartialProfile {name=>"name"[Some(None)],id=>"id"[Some(None)],properties=>"properties"[Some(BoundedList(vec![]))]});
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
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = map_entries(self.profile.to_persistent()?)?;
        v.extend(map_entries(self.skin_patch.to_persistent()?)?);
        Ok(PersistentValue::Map(v))
    }
}
fn map_entries(value: PersistentValue) -> Result<Vec<(String, PersistentValue)>> {
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
            amplifier: field(v, "amplifier", Some(VarInt(0)))?,
            duration: field(v, "duration", Some(VarInt(0)))?,
            ambient: field(v, "ambient", Some(false))?,
            show_particles,
            show_icon: field(v, "show_icon", Some(show_particles))?,
            hidden_effect: field(v, "hidden_effect", Some(None))?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        entry(&mut v, "amplifier", &self.amplifier, Some(VarInt(0)))?;
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
                probability: field(v, "probability", Some(1.0))?,
            }),
            Some("minecraft:remove_effects" | "remove_effects") => {
                Ok(Self::RemoveEffects(field(v, "effects", None)?))
            }
            Some("minecraft:clear_all_effects" | "clear_all_effects") => Ok(Self::ClearAllEffects),
            Some("minecraft:teleport_randomly" | "teleport_randomly") => {
                Ok(Self::TeleportRandomly {
                    diameter: field(v, "diameter", Some(16.0))?,
                    directional_particles: field(v, "directional_particles", Some(true))?,
                })
            }
            Some("minecraft:play_sound" | "play_sound") => {
                Ok(Self::PlaySound(field(v, "sound", None)?))
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
                v.push(("effects".into(), effects.to_persistent()?));
                entry(&mut v, "probability", probability, Some(1.0))?;
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
                entry(&mut v, "diameter", diameter, Some(16.0))?;
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
        let mut v = object(v)?.clone();
        let id = v.remove("id").ok_or_else(|| invalid("entity type"))?;
        Ok(Self {
            entity_type: EntityType::from_json(&id)?,
            tag: CustomData::from_json(&Json::Object(v))?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut tag = self.tag.0.clone();
        tag.as_compound_mut()
            .ok_or_else(|| invalid("compound tag"))?
            .insert("id", self.entity_type.to_nbt()?);
        Ok(PersistentValue::Nbt(tag))
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        let mut tag = v.clone();
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
        let mut v = object(v)?.clone();
        let id = v.remove("id").ok_or_else(|| invalid("block entity type"))?;
        Ok(Self {
            entity_type: BlockEntityType::from_json(&id)?,
            tag: CustomData::from_json(&Json::Object(v))?,
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
                let key = registry(k);
                let ty = if ComponentTypeId::from_name(key.as_str()).is_some() {
                    Either::Right(ComponentTypeId::from_name(key.as_str()).unwrap())
                } else {
                    Either::Left(ComponentPredicateType(key))
                };
                partial.push(ComponentPredicate {
                    predicate_type: ty,
                    value: json_tag(v)?,
                });
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
                        .map(|p| {
                            let name = match &p.predicate_type {
                                Either::Left(t) => t.0.to_string(),
                                Either::Right(t) => t.name().to_owned(),
                            };
                            Ok((name, p.value.to_persistent()?))
                        })
                        .collect::<Result<_>>()?,
                ),
            ));
        }
        Ok(PersistentValue::Map(v))
    }
}
impl Persistent for IntBounds {
    fn from_json(v: &Json) -> Result<Self> {
        if v.is_number() {
            let n = i32::from_json(v)?;
            Ok(Self {
                min: Some(n),
                max: Some(n),
            })
        } else {
            Ok(Self {
                min: field(v, "min", Some(None))?,
                max: field(v, "max", Some(None))?,
            })
        }
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if let (Some(min), Some(max)) = (self.min, self.max) {
            if min == max {
                return min.to_persistent();
            }
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
            DataComponent::CustomName(<Component as Persistent>::from_json(value)?)
        }
        "minecraft:minimum_attack_charge" => {
            DataComponent::MinimumAttackCharge(<f32 as Persistent>::from_json(value)?)
        }
        "minecraft:damage_type" => {
            DataComponent::DamageType(<DamageType as Persistent>::from_json(value)?)
        }
        "minecraft:item_name" => {
            DataComponent::ItemName(<Component as Persistent>::from_json(value)?)
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
            DataComponent::PotionDurationScale(<f32 as Persistent>::from_json(value)?)
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
            DataComponent::SignTextFront(<SignText as Persistent>::from_json(value)?)
        }
        "minecraft:sign_text_back" => {
            DataComponent::SignTextBack(<SignText as Persistent>::from_json(value)?)
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
    match key.as_str() {
        "minecraft:custom_data" => Ok(DataComponent::CustomData(CustomData::from_nbt(value)?)),
        "minecraft:bucket_entity_data" => Ok(DataComponent::BucketEntityData(
            CustomData::from_nbt(value)?,
        )),
        "minecraft:entity_data" => Ok(DataComponent::EntityData(TypedEntityData::from_nbt(value)?)),
        _ => component_from_json(key, &tag_json(value)),
    }
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
        _ => Ok(()),
    }
}
