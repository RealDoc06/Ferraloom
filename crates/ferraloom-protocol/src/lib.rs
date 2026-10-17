//! Minecraft Java 26.3 (protocol 777) wire codecs and transport primitives.
//!
//! Packet codecs operate on one unframed packet, including its VarInt packet ID.
//! Transport layers are applied in this order: packet, compression, frame, cipher.
pub mod chat;
pub mod codec;
pub mod common;
pub mod component;
pub mod entity_data;
pub mod play;
pub mod transport;
pub use codec::{Error, Identifier, Reader, Result, Wire, Writer};
pub use ferraloom_util::{BlockPos, Uuid};

pub const PROTOCOL_VERSION: i32 = 777;
