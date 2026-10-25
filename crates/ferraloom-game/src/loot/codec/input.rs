//! Read-side DynamicOps for the shared loot and predicate decode schemas.
use super::{Result, error};
use ferraloom_nbt::{NbtString, Tag};
use ferraloom_protocol::component::Persistent;
use serde_json::Value;
use std::borrow::Cow;

/// Format-specific reads used by one loot/predicate schema. Scalar and component
/// coercions delegate to the canonical persistent codec for the original ops.
pub trait DecodeInput: Sized {
    fn is_object(&self) -> bool;
    fn is_number(&self) -> bool;
    fn is_string(&self) -> bool;
    fn is_boolean(&self) -> bool;
    fn as_bool(&self) -> Option<bool>;
    fn as_str(&self) -> Option<&str>;
    fn as_array(&self) -> Option<&[Self]>;
    fn is_array(&self) -> bool {
        self.as_array().is_some()
    }
    fn get(&self, key: &str) -> Option<&Self>;
    fn entries(&self) -> Result<Vec<(&str, &Self)>>;
    fn read<T: Persistent>(&self) -> ferraloom_protocol::Result<T>;
    fn read_wrapped<T: Persistent>(&self, key: &str) -> ferraloom_protocol::Result<T>;
}

/// JsonOps ingress retains the existing JSON scalar/component readers unchanged.
impl DecodeInput for Value {
    fn is_object(&self) -> bool {
        Value::is_object(self)
    }
    fn is_number(&self) -> bool {
        Value::is_number(self)
    }
    fn is_string(&self) -> bool {
        Value::is_string(self)
    }
    fn is_boolean(&self) -> bool {
        Value::is_boolean(self)
    }
    fn as_bool(&self) -> Option<bool> {
        Value::as_bool(self)
    }
    fn as_str(&self) -> Option<&str> {
        Value::as_str(self)
    }
    fn as_array(&self) -> Option<&[Self]> {
        Value::as_array(self).map(Vec::as_slice)
    }
    fn get(&self, key: &str) -> Option<&Self> {
        Value::get(self, key)
    }
    fn entries(&self) -> Result<Vec<(&str, &Self)>> {
        self.as_object()
            .ok_or_else(|| error("expected object"))
            .map(|map| {
                map.iter()
                    .map(|(key, value)| (key.as_str(), value))
                    .collect()
            })
    }
    fn read<T: Persistent>(&self) -> ferraloom_protocol::Result<T> {
        T::from_json(self)
    }
    fn read_wrapped<T: Persistent>(&self, key: &str) -> ferraloom_protocol::Result<T> {
        let mut wrapped = serde_json::Map::new();
        wrapped.insert(key.into(), self.clone());
        T::from_json(&Value::Object(wrapped))
    }
}

/// NbtOps ingress. Cached child views allow the same borrowed schema to read
/// collection tags, including primitive arrays, without changing their NBT kinds.
/// Original tags remain available for lossless persistent component decoding.
pub struct NbtOps<'a> {
    tag: Cow<'a, Tag>,
    children: Children<'a>,
}
enum Children<'a> {
    Scalar,
    String(Option<String>),
    List(Vec<NbtOps<'a>>),
    Compound(Vec<Field<'a>>),
}
struct Field<'a> {
    key: &'a NbtString,
    utf8: Option<String>,
    value: NbtOps<'a>,
}
impl<'a> NbtOps<'a> {
    pub fn new(tag: &'a Tag) -> Self {
        let children = match tag {
            Tag::String(value) => Children::String(value.to_string_checked().ok()),
            Tag::Compound(value) => Children::Compound(
                value
                    .iter_java_order()
                    .map(|(key, value)| Field {
                        key,
                        utf8: key.to_string_checked().ok(),
                        value: Self::new(value),
                    })
                    .collect(),
            ),
            Tag::List(values) => Children::List(values.iter().map(Self::new).collect()),
            Tag::ByteArray(values) => Children::List(
                values
                    .iter()
                    .map(|value| Self::number(Tag::Byte(*value)))
                    .collect(),
            ),
            Tag::IntArray(values) => Children::List(
                values
                    .iter()
                    .map(|value| Self::number(Tag::Int(*value)))
                    .collect(),
            ),
            Tag::LongArray(values) => Children::List(
                values
                    .iter()
                    .map(|value| Self::number(Tag::Long(*value)))
                    .collect(),
            ),
            _ => Children::Scalar,
        };
        Self {
            tag: Cow::Borrowed(tag),
            children,
        }
    }
    fn number(tag: Tag) -> Self {
        Self {
            tag: Cow::Owned(tag),
            children: Children::Scalar,
        }
    }
}
impl DecodeInput for NbtOps<'_> {
    fn is_object(&self) -> bool {
        matches!(self.children, Children::Compound(_))
    }
    fn is_number(&self) -> bool {
        matches!(
            self.tag.as_ref(),
            Tag::Byte(_)
                | Tag::Short(_)
                | Tag::Int(_)
                | Tag::Long(_)
                | Tag::Float(_)
                | Tag::Double(_)
        )
    }
    fn is_string(&self) -> bool {
        matches!(self.children, Children::String(_))
    }
    fn is_boolean(&self) -> bool {
        self.is_number()
    }
    fn as_bool(&self) -> Option<bool> {
        bool::from_nbt(self.tag.as_ref()).ok()
    }
    fn as_str(&self) -> Option<&str> {
        match &self.children {
            Children::String(text) => text.as_deref(),
            _ => None,
        }
    }
    fn as_array(&self) -> Option<&[Self]> {
        match &self.children {
            Children::List(values) => Some(values),
            _ => None,
        }
    }
    fn get(&self, key: &str) -> Option<&Self> {
        let Children::Compound(fields) = &self.children else {
            return None;
        };
        fields
            .iter()
            .find(|field| field.key.as_utf16().iter().copied().eq(key.encode_utf16()))
            .map(|field| &field.value)
    }
    fn entries(&self) -> Result<Vec<(&str, &Self)>> {
        let Children::Compound(fields) = &self.children else {
            return Err(error("expected object"));
        };
        fields
            .iter()
            .map(|field| {
                let key = field
                    .utf8
                    .as_deref()
                    .ok_or_else(|| error("invalid UTF-16 field name"))?;
                Ok((key, &field.value))
            })
            .collect()
    }
    fn read<T: Persistent>(&self) -> ferraloom_protocol::Result<T> {
        T::from_nbt(self.tag.as_ref())
    }
    fn read_wrapped<T: Persistent>(&self, key: &str) -> ferraloom_protocol::Result<T> {
        let mut wrapped = ferraloom_nbt::CompoundTag::new();
        wrapped.insert(key, self.tag.as_ref().copy());
        T::from_nbt(&Tag::Compound(wrapped))
    }
}
