//! Horizon's own shader packs (`docs/api/shaders-reference.md`): WGSL written
//! against the renderer's stages, described by `horizon-shaders.toml` and
//! shipped as a folder or a `.hshader`. This crate reads and checks packs; the
//! mod index's check (`horizon-mods`, feature `shader-packs`) uses it. The server
//! engine draws nothing and loads no pack.
pub mod check;
pub mod inputs;
pub mod manifest;
pub mod package;
pub mod reference;
pub mod stages;

pub use check::{Problem, Report, check};
pub use inputs::Frame;
pub use manifest::{API, MANIFEST, Manifest, OptionKind, PackOption, SCHEMA, Stage};
pub use package::{EXTENSION, Pack};
pub use stages::{Composed, StageKind, compose};
