//! Dedicated server runtime, separate from region-owned gameplay.
pub mod access;
pub mod assets;
pub mod chat;
pub mod commands;
mod datapacks;
mod generation;
#[cfg(test)]
mod generation_tests;
pub mod jobs;
pub mod network;
pub mod properties;
pub mod registries;
pub mod server;
pub mod world;
