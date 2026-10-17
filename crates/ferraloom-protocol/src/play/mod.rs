//! Port of GameProtocols (Minecraft Java 26.3). Packet bodies are unframed.
//! Direction enums include the generated packet ID; individual structs do not.
use crate::chat::Component;
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

/// Tagged vanilla stream-codec dispatch with strongly typed variant payloads.
macro_rules! wire_union {
    ($name:ident : $tag:ident { $($variant:ident = $id:literal { $($field:ident : $ty:ty),* $(,)? }),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum $name { $($variant { $($field: $ty),* }),* }
        impl $name {
            pub fn tag(&self) -> i32 { match self { $(Self::$variant { .. } => $id),* } }
            pub fn decode_variant(r: &mut Reader<'_>, id: i32) -> Result<Self> {
                let _ = &r;
                match id { $($id => Ok(Self::$variant { $($field: <$ty>::decode(r)?),* }),)* id => Err(Error::invalid(stringify!($name), id.to_string())) }
            }
            pub fn encode_body(&self, w: &mut Writer<'_>) -> Result<()> {
                let _ = &w;
                match self { $(Self::$variant { $($field),* } => { $($field.encode(w)?;)* }),* }
                Ok(())
            }
        }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { let id = i32::from(r.$tag()?); Self::decode_variant(r,id) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { w.$tag(self.tag() as _); self.encode_body(w) }
        }
    };
}

mod chunk;
pub use chunk::*;
mod movement;
pub use movement::*;
mod chat;
pub use chat::*;
mod boss;
pub use boss::*;
mod player_info;
pub use player_info::*;
mod types;
pub use types::*;
pub mod clientbound;
pub mod serverbound;

#[cfg(test)]
mod tests;
