use std::{collections::BTreeMap, fmt};

/// A Java UTF-16 string, including unpaired surrogates accepted by `DataInput.readUTF`.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtString(Vec<u16>);

impl NbtString {
    pub fn from_utf16(units: Vec<u16>) -> Self {
        Self(units)
    }
    pub fn as_utf16(&self) -> &[u16] {
        &self.0
    }
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
    pub fn to_string_checked(&self) -> Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.0)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl From<&str> for NbtString {
    fn from(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
}
impl From<String> for NbtString {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}
impl fmt::Debug for NbtString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string_lossy().fmt(f)
    }
}
impl fmt::Display for NbtString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

/// Port of the thirteen `Tag` types. Lists may contain different tag types in 26.3.
#[derive(Clone, Debug)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(NbtString),
    List(ListTag),
    Compound(CompoundTag),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::End, Self::End) => true,
            (Self::Byte(a), Self::Byte(b)) => a == b,
            (Self::Short(a), Self::Short(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Long(a), Self::Long(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            (Self::Double(a), Self::Double(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            (Self::ByteArray(a), Self::ByteArray(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Compound(a), Self::Compound(b)) => a == b,
            (Self::IntArray(a), Self::IntArray(b)) => a == b,
            (Self::LongArray(a), Self::LongArray(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for Tag {}

impl Tag {
    /// Port of Tag#copy. Container values are copied recursively.
    pub fn copy(&self) -> Self {
        match self {
            Self::Compound(c) => Self::Compound(c.copy()),
            Self::List(v) => Self::List(ListTag(v.iter().map(Self::copy).collect())),
            _ => self.clone(),
        }
    }
    /// Port of Tag#sizeInBytes: vanilla's heap allocation estimate.
    pub fn size_in_bytes(&self) -> usize {
        match self {
            Self::End => 8,
            Self::Byte(_) => 9,
            Self::Short(_) => 10,
            Self::Int(_) | Self::Float(_) => 12,
            Self::Long(_) | Self::Double(_) => 16,
            Self::ByteArray(v) => 24 + v.len(),
            Self::IntArray(v) => 24 + 4 * v.len(),
            Self::LongArray(v) => 24 + 8 * v.len(),
            Self::String(v) => 36 + 2 * v.len(),
            Self::List(v) => 36 + 4 * v.len() + v.iter().map(Self::size_in_bytes).sum::<usize>(),
            Self::Compound(c) => {
                48 + c
                    .iter()
                    .map(|(k, v)| 64 + 2 * k.len() + v.size_in_bytes())
                    .sum::<usize>()
            }
        }
    }
    pub fn id(&self) -> u8 {
        match self {
            Self::End => 0,
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::ByteArray(_) => 7,
            Self::String(_) => 8,
            Self::List(_) => 9,
            Self::Compound(_) => 10,
            Self::IntArray(_) => 11,
            Self::LongArray(_) => 12,
        }
    }
    pub fn as_int(&self) -> Option<i32> {
        Some(match self {
            Self::Byte(v) => i32::from(*v),
            Self::Short(v) => i32::from(*v),
            Self::Int(v) => *v,
            Self::Long(v) => *v as i32,
            Self::Float(v) => java_floor(f64::from(*v)),
            Self::Double(v) => java_floor(*v),
            _ => return None,
        })
    }
    pub fn as_byte(&self) -> Option<i8> {
        self.as_int().map(|v| v as i8)
    }
    pub fn as_short(&self) -> Option<i16> {
        self.as_int().map(|v| v as i16)
    }
    pub fn as_long(&self) -> Option<i64> {
        Some(match self {
            Self::Long(v) => *v,
            Self::Float(v) => *v as i64,
            Self::Double(v) => v.floor() as i64,
            _ => i64::from(self.as_int()?),
        })
    }
    pub fn as_double(&self) -> Option<f64> {
        Some(match self {
            Self::Long(v) => *v as f64,
            Self::Float(v) => f64::from(*v),
            Self::Double(v) => *v,
            _ => f64::from(self.as_int()?),
        })
    }
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Long(v) => Some(*v as f32),
            _ => self.as_double().map(|v| v as f32),
        }
    }
    pub fn as_boolean(&self) -> Option<bool> {
        self.as_byte().map(|v| v != 0)
    }
    pub fn as_string(&self) -> Option<&NbtString> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_compound(&self) -> Option<&CompoundTag> {
        if let Self::Compound(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_compound_mut(&mut self) -> Option<&mut CompoundTag> {
        if let Self::Compound(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_list_mut(&mut self) -> Option<&mut ListTag> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_list(&self) -> Option<&ListTag> {
        if let Self::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_byte_array(&self) -> Option<&[i8]> {
        if let Self::ByteArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_int_array(&self) -> Option<&[i32]> {
        if let Self::IntArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_long_array(&self) -> Option<&[i64]> {
        if let Self::LongArray(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn collection_len(&self) -> Option<usize> {
        match self {
            Self::List(v) => Some(v.len()),
            Self::ByteArray(v) => Some(v.len()),
            Self::IntArray(v) => Some(v.len()),
            Self::LongArray(v) => Some(v.len()),
            _ => None,
        }
    }
    pub fn collection_get(&self, index: usize) -> Option<Tag> {
        match self {
            Self::List(v) => v.get(index).cloned(),
            Self::ByteArray(v) => v.get(index).copied().map(Self::Byte),
            Self::IntArray(v) => v.get(index).copied().map(Self::Int),
            Self::LongArray(v) => v.get(index).copied().map(Self::Long),
            _ => None,
        }
    }
    pub(crate) fn collection_set(&mut self, index: usize, value: &Tag) -> bool {
        match self {
            Self::List(v) => {
                v.0[index] = value.clone();
                true
            }
            Self::ByteArray(v) => value.as_byte().map(|n| v[index] = n).is_some(),
            Self::IntArray(v) => value.as_int().map(|n| v[index] = n).is_some(),
            Self::LongArray(v) => value.as_long().map(|n| v[index] = n).is_some(),
            _ => false,
        }
    }
    pub(crate) fn collection_insert(&mut self, index: usize, value: &Tag) -> bool {
        match self {
            Self::List(v) => {
                v.0.insert(index, value.clone());
                true
            }
            Self::ByteArray(v) => value.as_byte().map(|n| v.insert(index, n)).is_some(),
            Self::IntArray(v) => value.as_int().map(|n| v.insert(index, n)).is_some(),
            Self::LongArray(v) => value.as_long().map(|n| v.insert(index, n)).is_some(),
            _ => false,
        }
    }
    pub(crate) fn collection_remove(&mut self, index: usize) {
        match self {
            Self::List(v) => {
                v.0.remove(index);
            }
            Self::ByteArray(v) => {
                v.remove(index);
            }
            Self::IntArray(v) => {
                v.remove(index);
            }
            Self::LongArray(v) => {
                v.remove(index);
            }
            _ => {}
        }
    }
    pub(crate) fn collection_clear(&mut self) {
        match self {
            Self::List(v) => v.0.clear(),
            Self::ByteArray(v) => v.clear(),
            Self::IntArray(v) => v.clear(),
            Self::LongArray(v) => v.clear(),
            _ => {}
        }
    }
}
// Port of Mth.floor in 26.3: floor first, then a saturating Java integer cast.
fn java_floor(v: f64) -> i32 {
    v.floor() as i32
}

/// Port of `ListTag`. Out-of-bounds typed getters return `None` or the supplied default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListTag(pub Vec<Tag>);
impl ListTag {
    pub fn new() -> Self {
        Self::default()
    }
}
impl std::ops::Deref for ListTag {
    type Target = Vec<Tag>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ListTag {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<Vec<Tag>> for ListTag {
    fn from(value: Vec<Tag>) -> Self {
        Self(value)
    }
}

/// Port of `CompoundTag`. Equality ignores insertion order; sorted visitors use UTF-16 key order.
#[derive(Clone, Debug, Default)]
pub struct CompoundTag {
    values: BTreeMap<NbtString, Tag>,
    order: crate::key_order::KeyOrder,
}
impl PartialEq for CompoundTag {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl Eq for CompoundTag {}
impl CompoundTag {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.values.get(&NbtString::from(key))
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
        self.values.get_mut(&NbtString::from(key))
    }
    pub fn get_key(&self, key: &NbtString) -> Option<&Tag> {
        self.values.get(key)
    }
    pub fn get_mut_key(&mut self, key: &NbtString) -> Option<&mut Tag> {
        self.values.get_mut(key)
    }
    pub fn remove_key(&mut self, key: &NbtString) -> Option<Tag> {
        self.order.remove(key);
        self.values.remove(key)
    }
    pub fn insert(&mut self, key: impl Into<NbtString>, value: Tag) -> Option<Tag> {
        let key = key.into();
        if !self.values.contains_key(&key) {
            self.order.insert(key.clone());
        }
        self.values.insert(key, value)
    }
    pub fn remove(&mut self, key: &str) -> Option<Tag> {
        let key = NbtString::from(key);
        self.order.remove(&key);
        self.values.remove(&key)
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&NbtString, &Tag)> {
        self.values.iter()
    }
    pub fn values(&self) -> impl Iterator<Item = &Tag> {
        self.values.values()
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Tag> {
        self.values.values_mut()
    }
    pub(crate) fn java_keys(&self) -> Vec<&NbtString> {
        self.order.keys()
    }
    /// Port of CompoundTag#copy, which constructs a fresh HashMap in source iteration order.
    pub fn copy(&self) -> Self {
        self.java_keys()
            .into_iter()
            .map(|k| (k.clone(), self.get_key(k).unwrap().copy()))
            .collect()
    }
    /// Port of `CompoundTag#merge`, recursively merging compounds and copying other values.
    pub fn merge(&mut self, source: &CompoundTag) {
        for k in source.java_keys() {
            let v = source.get_key(k).unwrap();
            if let (Some(Tag::Compound(a)), Tag::Compound(b)) = (self.values.get_mut(k), v) {
                a.merge(b);
            } else {
                self.insert(k.clone(), v.copy());
            }
        }
    }
}
impl<K: Into<NbtString>> FromIterator<(K, Tag)> for CompoundTag {
    fn from_iter<T: IntoIterator<Item = (K, Tag)>>(iter: T) -> Self {
        let mut result = Self::new();
        for (k, v) in iter {
            result.insert(k, v);
        }
        result
    }
}

macro_rules! getters {
    ($container:ty,$key:ty) => {
        impl $container {
            pub fn get_byte(&self, key: $key) -> Option<i8> {
                self.get(key).and_then(Tag::as_byte)
            }
            pub fn get_short(&self, key: $key) -> Option<i16> {
                self.get(key).and_then(Tag::as_short)
            }
            pub fn get_int(&self, key: $key) -> Option<i32> {
                self.get(key).and_then(Tag::as_int)
            }
            pub fn get_long(&self, key: $key) -> Option<i64> {
                self.get(key).and_then(Tag::as_long)
            }
            pub fn get_float(&self, key: $key) -> Option<f32> {
                self.get(key).and_then(Tag::as_float)
            }
            pub fn get_double(&self, key: $key) -> Option<f64> {
                self.get(key).and_then(Tag::as_double)
            }
            pub fn get_boolean(&self, key: $key) -> Option<bool> {
                self.get(key).and_then(Tag::as_boolean)
            }
            pub fn get_string(&self, key: $key) -> Option<&NbtString> {
                self.get(key).and_then(Tag::as_string)
            }
            pub fn get_compound(&self, key: $key) -> Option<&CompoundTag> {
                self.get(key).and_then(Tag::as_compound)
            }
            pub fn get_list(&self, key: $key) -> Option<&ListTag> {
                self.get(key).and_then(Tag::as_list)
            }
            pub fn get_compound_mut(&mut self, key: $key) -> Option<&mut CompoundTag> {
                self.get_mut(key).and_then(Tag::as_compound_mut)
            }
            pub fn get_list_mut(&mut self, key: $key) -> Option<&mut ListTag> {
                self.get_mut(key).and_then(Tag::as_list_mut)
            }
            pub fn get_byte_array(&self, key: $key) -> Option<&[i8]> {
                self.get(key).and_then(Tag::as_byte_array)
            }
            pub fn get_int_array(&self, key: $key) -> Option<&[i32]> {
                self.get(key).and_then(Tag::as_int_array)
            }
            pub fn get_long_array(&self, key: $key) -> Option<&[i64]> {
                self.get(key).and_then(Tag::as_long_array)
            }
            pub fn get_byte_or(&self, key: $key, default: i8) -> i8 {
                self.get_byte(key).unwrap_or(default)
            }
            pub fn get_short_or(&self, key: $key, default: i16) -> i16 {
                self.get_short(key).unwrap_or(default)
            }
            pub fn get_int_or(&self, key: $key, default: i32) -> i32 {
                self.get_int(key).unwrap_or(default)
            }
            pub fn get_long_or(&self, key: $key, default: i64) -> i64 {
                self.get_long(key).unwrap_or(default)
            }
            pub fn get_float_or(&self, key: $key, default: f32) -> f32 {
                self.get_float(key).unwrap_or(default)
            }
            pub fn get_double_or(&self, key: $key, default: f64) -> f64 {
                self.get_double(key).unwrap_or(default)
            }
            pub fn get_boolean_or(&self, key: $key, default: bool) -> bool {
                self.get_boolean(key).unwrap_or(default)
            }
            pub fn get_string_or<'a>(&'a self, key: $key, default: &'a NbtString) -> &'a NbtString {
                self.get_string(key).unwrap_or(default)
            }
            pub fn get_compound_or_empty(&self, key: $key) -> CompoundTag {
                self.get_compound(key).cloned().unwrap_or_default()
            }
            pub fn get_list_or_empty(&self, key: $key) -> ListTag {
                self.get_list(key).cloned().unwrap_or_default()
            }
        }
    };
}
getters!(CompoundTag, &str);
getters!(ListTag, usize);

/// Port of `NbtUtils#compareNbt`. A missing expected tag matches anything.
pub fn compare_nbt(expected: Option<&Tag>, actual: Option<&Tag>, partial_lists: bool) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let Some(actual) = actual else { return false };
    match (expected, actual) {
        (Tag::Compound(a), Tag::Compound(b)) => {
            a.len() <= b.len()
                && a.iter()
                    .all(|(k, v)| compare_nbt(Some(v), b.get_key(k), partial_lists))
        }
        (Tag::List(a), Tag::List(b)) if partial_lists => {
            if a.is_empty() {
                b.is_empty()
            } else {
                a.len() <= b.len()
                    && a.iter()
                        .all(|v| b.iter().any(|w| compare_nbt(Some(v), Some(w), true)))
            }
        }
        _ => expected == actual,
    }
}

macro_rules! from_tag {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(impl From<$ty> for Tag {fn from(value:$ty)->Self {Self::$variant(value)}})*};
}
from_tag!(
    i8 => Byte, i16 => Short, i32 => Int, i64 => Long,
    Vec<i8> => ByteArray, Vec<i32> => IntArray, Vec<i64> => LongArray,
    NbtString => String, ListTag => List, CompoundTag => Compound,
);
impl From<bool> for Tag {
    fn from(value: bool) -> Self {
        Self::Byte(i8::from(value))
    }
}
impl From<f32> for Tag {
    fn from(value: f32) -> Self {
        Self::Float(if value == 0.0 { 0.0 } else { value })
    }
}
impl From<f64> for Tag {
    fn from(value: f64) -> Self {
        Self::Double(if value == 0.0 { 0.0 } else { value })
    }
}
impl From<&str> for Tag {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}
impl From<String> for Tag {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}
