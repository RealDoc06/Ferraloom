//! Port of GameProtocols (Minecraft Java 26.3). Packet bodies are unframed.
//! Direction enums include the generated packet ID; individual structs do not.
use crate::codec::{Angle, BitSet, GlobalPos, VarInt, VarLong};
use crate::{BlockPos, Error, Identifier, Reader, Result, Uuid, Wire, Writer};

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

mod types;
pub use types::*;
pub mod clientbound;
pub mod serverbound;
