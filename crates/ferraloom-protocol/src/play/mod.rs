//! Port of GameProtocols (Minecraft Java 26.3). Packet bodies are unframed.
//! Direction enums include the generated packet ID; individual structs do not.
use crate::chat::Component;
use crate::codec::{Angle, BitSet, GlobalPos, VarInt, VarLong};
use crate::{BlockPos, Error, Identifier, Reader, Result, Uuid, Wire, Writer};

macro_rules! packet {
    ($(#[$meta:meta])* $name:ident { $($field:ident: $ty:ty $(=> $decode:path)?),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                let _ = &r;
                Ok(Self { $($field: packet!(@read r, $ty $(, $decode)?)),* })
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                let _ = &w;
                $(self.$field.encode(w)?;)*
                Ok(())
            }
        }
    };
    (@read $r:ident, $ty:ty) => { <$ty as Wire>::decode($r)? };
    (@read $r:ident, $ty:ty, $decode:path) => { $decode($r)? };
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident = $id:literal),* $(,)? } $(, $mode:ident)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name { $($variant),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                match r.var_int()? { $($id => Ok(Self::$variant),)* id => { const VALUES: &[$name] = &[$($name::$variant),*]; match stringify!($($mode)?) { "wrapping" => Ok(VALUES[id.rem_euclid(VALUES.len() as i32) as usize]), "zero" => Ok(VALUES[0]), _ => Err(Error::invalid(stringify!($name), id.to_string())) } } }
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                w.var_int(match self { $(Self::$variant => $id),* }); Ok(())
            }
        }
    };
}

/// Tagged vanilla stream-codec dispatch with strongly typed variant payloads.
macro_rules! wire_union {
    ($name:ident : $tag:ident { $($variant:ident = $id:literal { $($field:ident : $ty:ty $(=> $decode:path)?),* $(,)? }),* $(,)? } $(, $validate:path)?) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum $name { $($variant { $($field: $ty),* }),* }
        impl $name {
            pub fn tag(&self) -> i32 { match self { $(Self::$variant { .. } => $id),* } }
            pub fn decode_variant(r: &mut Reader<'_>, id: i32) -> Result<Self> {
                let _ = &r;
                match id { $($id => Ok(Self::$variant { $($field: packet!(@read r, $ty $(, $decode)?)),* }),)* id => Err(Error::invalid(stringify!($name), id.to_string())) }
            }
            pub fn encode_body(&self, w: &mut Writer<'_>) -> Result<()> {
                let _ = &w;
                match self { $(Self::$variant { $($field),* } => { $($field.encode(w)?;)* }),* }
                Ok(())
            }
        }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { let id = i32::from(r.$tag()?); let value=Self::decode_variant(r,id)?; $($validate(&value)?;)? Ok(value) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { $($validate(self)?;)? w.$tag(self.tag() as _); self.encode_body(w) }
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
mod inventory;
pub use inventory::*;
mod recipes;
pub use recipes::*;
mod map;
pub use map::*;
mod waypoint;
pub use waypoint::*;
mod commands;
pub use commands::*;
mod scoreboard;
pub use scoreboard::*;
mod advancement;
pub use advancement::*;
mod debug;
pub use debug::*;
mod bundle;
pub use bundle::*;
mod types;
pub use types::*;
pub mod clientbound;
pub mod serverbound;
pub use clientbound::Packet as ClientboundPacket;
pub use serverbound::Packet as ServerboundPacket;

#[cfg(test)]
mod tests;

fn optional_trusted_component(r: &mut Reader<'_>) -> Result<Option<Component>> {
    r.optional(Component::decode_trusted)
}

fn optional_trusted_boxed_component(r: &mut Reader<'_>) -> Result<Option<Box<Component>>> {
    r.optional(|r| Component::decode_trusted(r).map(Box::new))
}
