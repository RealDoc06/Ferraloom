use super::{Error, Reader, Result, Wire, Writer};
use ferraloom_nbt::Tag;
use ferraloom_util::{BlockPos, Uuid};
use std::{fmt, str::FromStr};

/// Port of Identifier. Canonical namespace:path spelling, validated on construction.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);
impl Identifier {
    pub fn parse(text: &str) -> Result<Self> {
        let (namespace, path) = match text.split_once(':') {
            Some(("", path)) => ("minecraft", path),
            Some(pair) => pair,
            None => ("minecraft", text),
        };
        let valid = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b);
        if !namespace.bytes().all(valid) || !path.bytes().all(|b| valid(b) || b == b'/') {
            return Err(Error::invalid("identifier", text));
        }
        Ok(Self(format!("{namespace}:{path}")))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn namespace(&self) -> &str {
        self.0.split_once(':').unwrap().0
    }
    pub fn path(&self) -> &str {
        self.0.split_once(':').unwrap().1
    }
}
impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl FromStr for Identifier {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}
impl Wire for Identifier {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Self::parse(&r.string(32767)?)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(self.as_str(), 32767)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VarInt(pub i32);
impl Wire for VarInt {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.var_int().map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.0);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VarLong(pub i64);
impl Wire for VarLong {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.var_long().map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_long(self.0);
        Ok(())
    }
}

macro_rules! primitive {
    ($($ty:ty => $method:ident),* $(,)?) => {$ (
        impl Wire for $ty {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { r.$method() }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { w.$method(*self); Ok(()) }
        }
    )*};
}
primitive!(bool => bool, u8 => u8, i8 => i8, u16 => u16, i16 => i16, i32 => i32, i64 => i64, f32 => f32, f64 => f64);
impl Wire for String {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.string(32767)?.into_owned())
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(self, 32767)
    }
}
impl Wire for Uuid {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self::new(r.i64()?, r.i64()?))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.i64(self.most);
        w.i64(self.least);
        Ok(())
    }
}
impl Wire for BlockPos {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self::of(r.i64()?))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.i64(self.as_long());
        Ok(())
    }
}
impl Wire for Tag {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.nbt(false)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.nbt(self)
    }
}
impl<T: Wire> Wire for Option<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.optional(T::decode)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.optional(self, |w, v| v.encode(w))
    }
}
impl<T: Wire> Wire for Vec<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.list(i32::MAX as usize, T::decode)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(self, i32::MAX as usize, |w, v| v.encode(w))
    }
}

/// Port of GlobalPos#STREAM_CODEC.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPos {
    pub dimension: Identifier,
    pub pos: BlockPos,
}
impl Wire for GlobalPos {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            dimension: r.read()?,
            pos: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.dimension)?;
        w.write(&self.pos)
    }
}

/// A packed angle; retain the exact byte for lossless packet forwarding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Angle(pub u8);
impl Angle {
    /// Port of Mth#packDegrees (floor and wrap).
    pub fn from_degrees(degrees: f32) -> Self {
        Self((degrees * 256.0 / 360.0).floor() as i32 as u8)
    }
    pub fn degrees(self) -> f32 {
        self.0 as i8 as f32 * 360.0 / 256.0
    }
}
impl Wire for Angle {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.u8().map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(self.0);
        Ok(())
    }
}

/// Port of Java BitSet. Words are little-endian in bit significance, big-endian on the wire.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BitSet {
    words: Vec<u64>,
}
impl BitSet {
    pub fn from_words(mut words: Vec<u64>) -> Self {
        while words.last() == Some(&0) {
            words.pop();
        }
        Self { words }
    }
    pub fn words(&self) -> &[u64] {
        &self.words
    }
    pub fn get(&self, bit: usize) -> bool {
        self.words
            .get(bit / 64)
            .is_some_and(|w| w & (1 << (bit % 64)) != 0)
    }
    pub fn bit_length(&self) -> usize {
        self.words.last().map_or(0, |w| {
            (self.words.len() - 1) * 64 + (64 - w.leading_zeros()) as usize
        })
    }
    pub fn decode_fixed(r: &mut Reader<'_>, bits: usize) -> Result<Self> {
        let bytes = r.take(bits.div_ceil(8))?;
        let mut words = vec![0; bytes.len().div_ceil(8)];
        for (i, byte) in bytes.iter().enumerate() {
            words[i / 8] |= u64::from(*byte) << (i % 8 * 8);
        }
        Ok(Self::from_words(words))
    }
    pub fn encode_fixed(&self, w: &mut Writer<'_>, bits: usize) -> Result<()> {
        if self.bit_length() > bits {
            return Err(Error::Length {
                length: self.bit_length() as i64,
                max: bits,
            });
        }
        for i in 0..bits.div_ceil(8) {
            w.u8((self.words.get(i / 8).copied().unwrap_or(0) >> (i % 8 * 8)) as u8);
        }
        Ok(())
    }
}
impl Wire for BitSet {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let max = r.remaining().len() / 8;
        Ok(Self::from_words(r.list(max, |r| Ok(r.i64()? as u64))?))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.words, i32::MAX as usize, |w, v| {
            w.i64(*v as i64);
            Ok(())
        })
    }
}

/// Port of ByteBufCodecs#either. True selects the left branch.
#[derive(Clone, Debug, PartialEq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}
impl<L: Wire, R: Wire> Wire for Either<L, R> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        if r.bool()? {
            Ok(Self::Left(r.read()?))
        } else {
            Ok(Self::Right(r.read()?))
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::Left(v) => {
                w.bool(true);
                v.encode(w)
            }
            Self::Right(v) => {
                w.bool(false);
                v.encode(w)
            }
        }
    }
}

/// Port of Holder. A reference is resolved against the connection's registry context.
#[derive(Clone, Debug, PartialEq)]
pub enum Holder<T> {
    Reference(Identifier),
    Direct(T),
}
impl<T: Wire> Holder<T> {
    pub fn decode(r: &mut Reader<'_>, registry: &Identifier) -> Result<Self> {
        let id = r.var_int()?;
        if id == 0 {
            Ok(Self::Direct(r.read()?))
        } else {
            r.context()?
                .entry_key(registry, id.wrapping_sub(1))
                .map(Self::Reference)
                .ok_or_else(|| Error::invalid("holder id", id.to_string()))
        }
    }
    pub fn encode(&self, w: &mut Writer<'_>, registry: &Identifier) -> Result<()> {
        match self {
            Self::Direct(value) => {
                w.var_int(0);
                w.write(value)
            }
            Self::Reference(key) => {
                let id = w
                    .context()?
                    .entry_id(registry, key)
                    .filter(|id| *id >= 0 && *id < i32::MAX)
                    .ok_or_else(|| Error::invalid("holder key", key.to_string()))?;
                w.var_int(id + 1);
                Ok(())
            }
        }
    }
}
/// Port of ByteBufCodecs#holderSet. Explicit entries use unshifted registry IDs.
#[derive(Clone, Debug, PartialEq)]
pub enum HolderSet {
    Named(Identifier),
    Direct(Vec<Identifier>),
}
impl HolderSet {
    pub fn decode(r: &mut Reader<'_>, registry: &Identifier) -> Result<Self> {
        let length = r.count(i32::MAX as usize)?;
        if length == 0 {
            let key = r.read()?;
            if !r.context()?.has_tag(registry, &key) {
                return Err(Error::invalid("registry tag", key.to_string()));
            }
            Ok(Self::Named(key))
        } else {
            let mut entries = Vec::with_capacity((length - 1).min(65536));
            for _ in 1..length {
                entries.push(r.registry(registry)?);
            }
            Ok(Self::Direct(entries))
        }
    }
    pub fn encode(&self, w: &mut Writer<'_>, registry: &Identifier) -> Result<()> {
        match self {
            Self::Named(key) => {
                if !w.context()?.has_tag(registry, key) {
                    return Err(Error::invalid("registry tag", key.to_string()));
                }
                w.var_int(0);
                w.write(key)
            }
            Self::Direct(entries) => {
                w.count(entries.len() + 1, i32::MAX as usize)?;
                for entry in entries {
                    w.registry(registry, entry)?;
                }
                Ok(())
            }
        }
    }
}
