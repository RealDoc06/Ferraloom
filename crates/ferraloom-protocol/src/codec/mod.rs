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
    read: |r| r.var_int(),
    write: |w, v| {
        w.var_int(*v);
        Ok(())
    },
};
pub const VAR_LONG: Codec<i64> = Codec {
    read: |r| r.var_long(),
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
    /// Cold HolderLookup value read from the owning datapack snapshot. Metadata-
    /// only wire contexts have no values; unknown or unpublished keys return None.
    fn entry_value(&self, _registry: &Identifier, _key: &Identifier) -> Option<serde_json::Value> {
        None
    }
}

#[cfg(test)]
mod tests;

// Packet fields normally use their Wire representation; unusual fields are
// written out explicitly in their packet implementation.
macro_rules! wire_struct {
    ($(#[$meta:meta])* $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        impl $crate::Wire for $name {
            fn decode(r: &mut $crate::Reader<'_>) -> $crate::Result<Self> {
                let _ = &r;
                Ok(Self { $($field: <$ty as $crate::Wire>::decode(r)?),* })
            }
            fn encode(&self, w: &mut $crate::Writer<'_>) -> $crate::Result<()> {
                let _ = &w;
                $(<$ty as $crate::Wire>::encode(&self.$field, w)?;)*
                Ok(())
            }
        }
    };
}
pub(crate) use wire_struct;

macro_rules! state_packets {
    ($name:ident, $ids:path, { $($variant:ident($ty:ty) => $id:ident $( [ $decode:path, $encode:path ] )?),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum $name { $($variant($ty)),* }
        impl $name {
            pub fn packet_id(&self) -> u16 {
                use $ids as Id;
                match self { $(Self::$variant(_) => Id::$id.protocol_id()),* }
            }
        }
        impl $crate::Wire for $name {
            fn decode(r: &mut $crate::Reader<'_>) -> $crate::Result<Self> {
                use $ids as Id;
                let raw = r.var_int()?;
                let id = u16::try_from(raw).ok().and_then(Id::from_protocol_id)
                    .ok_or_else(|| $crate::Error::invalid("packet id",raw.to_string()))?;
                match id { $(Id::$id => Ok(Self::$variant(state_packets!(@read r, $ty $(, $decode)?)))),* }
            }
            fn encode(&self, w: &mut $crate::Writer<'_>) -> $crate::Result<()> {
                w.var_int(i32::from(self.packet_id()));
                match self { $(Self::$variant(value) => state_packets!(@write w, value $(, $encode)?)),* }
            }
        }
    };
    (@read $r:ident, $ty:ty) => { <$ty as $crate::Wire>::decode($r)? };
    (@read $r:ident, $ty:ty, $decode:path) => { $decode($r)? };
    (@write $w:ident, $value:ident) => { $crate::Wire::encode($value,$w) };
    (@write $w:ident, $value:ident, $encode:path) => { $encode($value,$w) };
}
pub(crate) use state_packets;

#[cfg(test)]
mod packet_tests;
