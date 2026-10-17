//! Vanilla structure generation. Registries and starts are immutable inputs;
//! each generation job owns its random streams and mutable neighborhood.
pub mod beardifier;
pub mod bounding_box;
pub mod placement;
pub mod pool;
pub mod processor;
pub mod rule;
pub mod template;

pub use beardifier::{
    Beardifier, JigsawJunction, Projection, Rigid, TerrainAdjustment, TerrainPiece,
};
pub use bounding_box::BoundingBox;
