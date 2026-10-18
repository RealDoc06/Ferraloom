//! Vanilla structure generation. Registries and starts are immutable inputs;
//! each generation job owns its random streams and mutable neighborhood.
pub mod alias;
pub mod beardifier;
pub mod bounding_box;
pub mod check;
pub mod decoration;
pub mod generation;
pub mod jigsaw;
pub mod jigsaw_structure;
pub mod locate;
mod nbt;
pub mod piece;
pub mod pipeline;
pub mod placement;
pub mod pool;
pub mod processor;
pub mod references;
pub mod rule;
pub mod scattered_piece;
pub mod start;
pub mod structures;
pub mod template;
pub mod template_piece;

pub use beardifier::{
    Beardifier, JigsawJunction, Projection, Rigid, TerrainAdjustment, TerrainPiece,
};
pub use bounding_box::BoundingBox;
