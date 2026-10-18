//! Vanilla structure generation. Registries and starts are immutable inputs;
//! each generation job owns its random streams and mutable neighborhood.
pub mod alias;
pub mod beardifier;
pub mod bounding_box;
pub mod check;
pub mod decoration;
pub mod jigsaw;
pub mod jigsaw_structure;
mod nbt;
pub mod pipeline;
pub mod placement;
pub mod pool;
pub mod processor;
pub mod references;
pub mod rule;
pub mod start;
pub mod template;

pub use beardifier::{
    Beardifier, JigsawJunction, Projection, Rigid, TerrainAdjustment, TerrainPiece,
};
pub use bounding_box::BoundingBox;
