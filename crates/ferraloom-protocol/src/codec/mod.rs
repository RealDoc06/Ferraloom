//! Port of FriendlyByteBuf, RegistryFriendlyByteBuf, StreamCodec and ByteBufCodecs.
//!
//! [`Wire`] implements a type's default representation. Use explicit reader and
//! writer methods for alternate representations (for example, VarInt vs i32).
//! Reader slices and valid UTF-8 strings borrow the input without allocation.
mod buffer;
mod types;
pub use buffer::{Reader, Writer};
use std::fmt;
pub use types::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    UnexpectedEof,
    VarIntTooBig,
    VarLongTooBig,
    Length { length: i64, max: usize },
    Invalid { kind: &'static str, detail: String },
    Nbt(ferraloom_nbt::NbtError),
    Io(std::io::Error),
}
impl Error {
    pub fn invalid(kind: &'static str, detail: impl Into<String>) -> Self {
        Self::Invalid {
            kind,
            detail: detail.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("Not enough bytes in buffer"),
            Self::VarIntTooBig => f.write_str("VarInt too big"),
            Self::VarLongTooBig => f.write_str("VarLong too big"),
            Self::Length { length, max } => write!(f, "Invalid length {length}, maximum {max}"),
            Self::Invalid { kind, detail } => write!(f, "{kind}: {detail}"),
            Self::Nbt(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Nbt(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<ferraloom_nbt::NbtError> for Error {
    fn from(e: ferraloom_nbt::NbtError) -> Self {
        Self::Nbt(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Port of StreamCodec for a type's canonical wire representation.
pub trait Wire: Sized {
    fn decode(input: &mut Reader<'_>) -> Result<Self>;
    fn encode(&self, output: &mut Writer<'_>) -> Result<()>;

    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut output = Writer::new();
        self.encode(&mut output)?;
        Ok(output.into_inner())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut input = Reader::new(bytes);
        let value = Self::decode(&mut input)?;
        input.finish()?;
        Ok(value)
    }
}

/// A codec can select an alternate representation without changing its value type.
pub trait StreamCodec<T> {
    fn decode(&self, input: &mut Reader<'_>) -> Result<T>;
    fn encode(&self, output: &mut Writer<'_>, value: &T) -> Result<()>;
}

/// Stateless StreamCodec backed by functions; custom stateful codecs implement the trait.
pub struct Codec<T> {
    pub read: fn(&mut Reader<'_>) -> Result<T>,
    pub write: fn(&mut Writer<'_>, &T) -> Result<()>,
}
impl<T> StreamCodec<T> for Codec<T> {
    fn decode(&self, input: &mut Reader<'_>) -> Result<T> {
        (self.read)(input)
    }
    fn encode(&self, output: &mut Writer<'_>, value: &T) -> Result<()> {
        (self.write)(output, value)
    }
}

pub const VAR_INT: Codec<i32> = Codec {
    read: Reader::var_int,
    write: |w, v| {
        w.var_int(*v);
        Ok(())
    },
};
pub const VAR_LONG: Codec<i64> = Codec {
    read: Reader::var_long,
    write: |w, v| {
        w.var_long(*v);
        Ok(())
    },
};

/// The only dynamic registry service required by the wire layer.
///
/// IDs must match the connection's synchronized registry order. Return `None`
/// for missing registries, unknown IDs/keys, and unbound tags. Implementations
/// own the data; codecs never mutate it or assume a server's global registries.
/// Context is needed only when a codec actually reads/writes registry references.
pub trait RegistryContext: Send + Sync {
    fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32>;
    fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier>;
    fn has_tag(&self, registry: &Identifier, tag: &Identifier) -> bool;
}

#[cfg(test)]
mod tests;
