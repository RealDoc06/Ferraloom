//! Shared common, cookie, and ping packet bodies; independent of protocol state.
use crate::chat::Component;
use crate::codec::{Either, Holder, VarInt};
use crate::{Error, Identifier, Reader, Result, Uuid, Wire, Writer};

macro_rules! packet {
    ($(#[$meta:meta])* $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                let _ = &r;
                Ok(Self { $($field: <$ty>::decode(r)?),* })
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                let _ = &w;
                $(self.$field.encode(w)?;)*
                Ok(())
            }
        }
    };
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident = $id:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name { $($variant),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                match r.var_int()? { $($id => Ok(Self::$variant),)* id => Err(Error::invalid(stringify!($name), id.to_string())) }
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                w.var_int(match self { $(Self::$variant => $id),* }); Ok(())
            }
        }
    };
}

/// Port of ByteBufCodecs.stringUtf8(maxLength).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf8<const MAX: usize>(pub String);
impl<const MAX: usize> Wire for Utf8<MAX> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.string(MAX)?.into_owned()))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.0, MAX)
    }
}

/// A length-prefixed byte array, distinct from a list of structured elements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteArray<const MAX: usize = { i32::MAX as usize }>(pub Vec<u8>);
impl<const MAX: usize> Wire for ByteArray<MAX> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.byte_array(MAX)?.to_vec()))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.byte_array(&self.0, MAX)
    }
}

/// Port of ByteBufCodecs.optionalVarInt; zero is absent, present IDs are shifted by one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionalVarInt(pub Option<i32>);
impl Wire for OptionalVarInt {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?;
        Ok(Self((id != 0).then(|| id.wrapping_sub(1))))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.0.map_or(0, |id| id.wrapping_add(1)));
        Ok(())
    }
}

/// Port of FriendlyByteBuf's nullable network NBT (TAG_End means absent).
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalNbt(pub Option<ferraloom_nbt::Tag>);
impl Wire for OptionalNbt {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.optional_nbt(false).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.optional_nbt(self.0.as_ref())
    }
}

pub mod clientbound;
pub mod serverbound;

/// Port of BrandPayload plus the extension-channel payload envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum CustomPayload {
    Brand(String),
    Unknown { id: Identifier, data: Vec<u8> },
}
impl CustomPayload {
    fn decode(r: &mut Reader<'_>, max: usize) -> Result<Self> {
        let id: Identifier = r.read()?;
        if id.as_str() == "minecraft:brand" {
            Ok(Self::Brand(r.read()?))
        } else {
            let len = r.remaining().len();
            if len > max {
                return Err(Error::Length {
                    length: len as i64,
                    max,
                });
            }
            Ok(Self::Unknown {
                id,
                data: r.take(len)?.to_vec(),
            })
        }
    }
    fn encode(&self, w: &mut Writer<'_>, max: usize) -> Result<()> {
        match self {
            Self::Brand(name) => {
                Identifier::parse("minecraft:brand")?.encode(w)?;
                name.encode(w)
            }
            Self::Unknown { id, data } => {
                if data.len() > max {
                    return Err(Error::Length {
                        length: data.len() as i64,
                        max,
                    });
                }
                id.encode(w)?;
                w.extend(data);
                Ok(())
            }
        }
    }
}
