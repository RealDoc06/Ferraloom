use super::{Error, Identifier, RegistryContext, Result, Wire};
use ferraloom_nbt::{NbtAccounter, NbtIo, Tag};
use std::{borrow::Cow, io::Cursor};

/// Port of FriendlyByteBuf, borrowing the incoming packet's bytes.
pub struct Reader<'a> {
    bytes: &'a [u8],
    context: Option<&'a dyn RegistryContext>,
}
impl<'a> Reader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            context: None,
        }
    }
    pub const fn with_context(bytes: &'a [u8], context: &'a dyn RegistryContext) -> Self {
        Self {
            bytes,
            context: Some(context),
        }
    }
    pub fn context(&self) -> Result<&'a dyn RegistryContext> {
        self.context
            .ok_or_else(|| Error::invalid("registry", "RegistryContext is required"))
    }
    pub fn remaining(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn finish(&self) -> Result<()> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Error::invalid(
                "packet",
                format!("{} unread bytes", self.bytes.len()),
            ))
        }
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        if length > self.bytes.len() {
            return Err(Error::UnexpectedEof);
        }
        let (value, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(value)
    }
    pub fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.bytes)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.i32()? as u32))
    }
    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.i64()? as u64))
    }
    /// Port of VarInt#read. Noncanonical encodings and unused high bits are accepted.
    pub fn var_int(&mut self) -> Result<i32> {
        let mut value = 0u32;
        for index in 0..6 {
            let byte = self.u8()?;
            if index == 5 {
                return Err(Error::VarIntTooBig);
            }
            value |= u32::from(byte & 0x7f).wrapping_shl(index * 7);
            if byte & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        unreachable!()
    }
    /// Port of VarLong#read, including Java's truncation of the tenth byte.
    pub fn var_long(&mut self) -> Result<i64> {
        let mut value = 0u64;
        for index in 0..11 {
            let byte = self.u8()?;
            if index == 10 {
                return Err(Error::VarLongTooBig);
            }
            value |= u64::from(byte & 0x7f).wrapping_shl(index * 7);
            if byte & 0x80 == 0 {
                return Ok(value as i64);
            }
        }
        unreachable!()
    }
    pub fn count(&mut self, max: usize) -> Result<usize> {
        let length = self.var_int()?;
        if length < 0 || length as usize > max {
            return Err(Error::Length {
                length: length.into(),
                max,
            });
        }
        Ok(length as usize)
    }
    pub fn byte_array(&mut self, max: usize) -> Result<&'a [u8]> {
        let length = self.count(max)?;
        self.take(length)
    }
    /// Port of Utf8String#read. Malformed UTF-8 is replaced, as by Netty/Java.
    pub fn string(&mut self, max_utf16: usize) -> Result<Cow<'a, str>> {
        let bytes = self.byte_array(max_utf16.saturating_mul(3))?;
        let text = String::from_utf8_lossy(bytes);
        let length = text.encode_utf16().count();
        if length > max_utf16 {
            return Err(Error::Length {
                length: length as i64,
                max: max_utf16,
            });
        }
        Ok(text)
    }
    pub fn optional<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        if self.bool()? {
            Ok(Some(read(self)?))
        } else {
            Ok(None)
        }
    }
    pub fn list<T>(
        &mut self,
        max: usize,
        mut read: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let length = self.count(max)?;
        let mut values = Vec::with_capacity(length.min(65536));
        for _ in 0..length {
            values.push(read(self)?);
        }
        Ok(values)
    }
    pub fn by_id<T>(&mut self, lookup: impl FnOnce(i32) -> Option<T>) -> Result<T> {
        let id = self.var_int()?;
        lookup(id).ok_or_else(|| Error::invalid("id", id.to_string()))
    }
    /// Port of ByteBufCodecs.tagCodec; trusted removes the heap quota, not depth limits.
    pub fn nbt(&mut self, trusted: bool) -> Result<Tag> {
        let mut cursor = Cursor::new(self.bytes);
        let mut accounter = if trusted {
            NbtAccounter::unlimited()
        } else {
            NbtAccounter::default()
        };
        let value = NbtIo::read_any_tag(&mut cursor, &mut accounter);
        let consumed = cursor.position() as usize;
        self.take(consumed)?;
        let value = value?;
        if value == Tag::End {
            return Err(Error::invalid("NBT", "Expected non-null tag"));
        }
        Ok(value)
    }
    /// Optional network NBT is a TAG_End sentinel, not a boolean followed by a tag.
    pub fn optional_nbt(&mut self, trusted: bool) -> Result<Option<Tag>> {
        if self.bytes.first() == Some(&0) {
            self.take(1)?;
            Ok(None)
        } else {
            self.nbt(trusted).map(Some)
        }
    }
    pub fn registry(&mut self, registry: &Identifier) -> Result<Identifier> {
        let id = self.var_int()?;
        self.context()?
            .entry_key(registry, id)
            .ok_or_else(|| Error::invalid("registry id", format!("{registry}[{id}]")))
    }
    pub fn read<T: Wire>(&mut self) -> Result<T> {
        T::decode(self)
    }
}

/// Port of FriendlyByteBuf's write operations. The output is transport agnostic.
#[derive(Default)]
pub struct Writer<'a> {
    bytes: Vec<u8>,
    context: Option<&'a dyn RegistryContext>,
}
impl<'a> Writer<'a> {
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            context: None,
        }
    }
    pub const fn with_context(context: &'a dyn RegistryContext) -> Self {
        Self {
            bytes: Vec::new(),
            context: Some(context),
        }
    }
    pub fn context(&self) -> Result<&'a dyn RegistryContext> {
        self.context
            .ok_or_else(|| Error::invalid("registry", "RegistryContext is required"))
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
    pub fn extend(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn i8(&mut self, value: i8) {
        self.u8(value as u8);
    }
    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub fn u16(&mut self, value: u16) {
        self.extend(&value.to_be_bytes());
    }
    pub fn i16(&mut self, value: i16) {
        self.u16(value as u16);
    }
    pub fn i32(&mut self, value: i32) {
        self.extend(&value.to_be_bytes());
    }
    pub fn i64(&mut self, value: i64) {
        self.extend(&value.to_be_bytes());
    }
    pub fn f32(&mut self, value: f32) {
        self.i32(value.to_bits() as i32);
    }
    pub fn f64(&mut self, value: f64) {
        self.i64(value.to_bits() as i64);
    }
    pub fn var_int(&mut self, value: i32) {
        let mut value = value as u32;
        while value & !0x7f != 0 {
            self.u8((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.u8(value as u8);
    }
    pub fn var_long(&mut self, value: i64) {
        let mut value = value as u64;
        while value & !0x7f != 0 {
            self.u8((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.u8(value as u8);
    }
    pub fn count(&mut self, length: usize, max: usize) -> Result<()> {
        if length > max.min(i32::MAX as usize) {
            return Err(Error::Length {
                length: length as i64,
                max,
            });
        }
        self.var_int(length as i32);
        Ok(())
    }
    pub fn byte_array(&mut self, bytes: &[u8], max: usize) -> Result<()> {
        self.count(bytes.len(), max)?;
        self.extend(bytes);
        Ok(())
    }
    pub fn string(&mut self, text: &str, max_utf16: usize) -> Result<()> {
        let length = text.encode_utf16().count();
        if length > max_utf16 {
            return Err(Error::Length {
                length: length as i64,
                max: max_utf16,
            });
        }
        self.byte_array(text.as_bytes(), max_utf16.saturating_mul(3))
    }
    pub fn optional<T>(
        &mut self,
        value: &Option<T>,
        write: impl FnOnce(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        self.bool(value.is_some());
        if let Some(value) = value {
            write(self, value)?;
        }
        Ok(())
    }
    pub fn list<T>(
        &mut self,
        values: &[T],
        max: usize,
        mut write: impl FnMut(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        self.count(values.len(), max)?;
        for value in values {
            write(self, value)?;
        }
        Ok(())
    }
    pub fn nbt(&mut self, tag: &Tag) -> Result<()> {
        if tag == &Tag::End {
            return Err(Error::invalid("NBT", "Expected non-null tag"));
        }
        NbtIo::write_any_tag(&mut self.bytes, tag)?;
        Ok(())
    }
    pub fn optional_nbt(&mut self, tag: Option<&Tag>) -> Result<()> {
        match tag {
            Some(tag) => self.nbt(tag),
            None => {
                self.u8(0);
                Ok(())
            }
        }
    }
    pub fn registry(&mut self, registry: &Identifier, entry: &Identifier) -> Result<()> {
        let id = self
            .context()?
            .entry_id(registry, entry)
            .filter(|id| *id >= 0)
            .ok_or_else(|| Error::invalid("registry key", format!("{registry}/{entry}")))?;
        self.var_int(id);
        Ok(())
    }
    pub fn write<T: Wire>(&mut self, value: &T) -> Result<()> {
        value.encode(self)
    }
}
