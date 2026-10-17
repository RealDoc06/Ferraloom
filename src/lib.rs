//! Ferraloom's independent Minecraft Java server engine.

mod configuration;
mod play;
pub mod protocol;
mod reference;
pub mod server;
pub mod world;

pub const MINECRAFT_VERSION: &str = "26.3";
pub const PROTOCOL_VERSION: i32 = 777;
