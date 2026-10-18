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

/// Region-only placement inputs shared by structure pieces. Features reborrow
/// this context through their region/live boundary without changing either RNG.
pub struct StructurePlacementContext<'a, 'r> {
    pub region: &'a mut crate::region::WorldGenRegion<'r>,
    pub random: &'a mut ferraloom_util::random::WorldgenRandom,
    pub origin: [i32; 3],
    pub generator: &'a dyn crate::generator::ChunkGeneratorAccess,
    pub registry: &'a crate::feature::FeatureRegistry,
}
impl<'r> StructurePlacementContext<'_, 'r> {
    pub fn reborrow_feature(&mut self) -> crate::feature::FeatureContext<'_, 'r> {
        crate::feature::FeatureContext {
            level: crate::feature::WorldGenLevel::Region(self.region),
            random: crate::feature::FeatureRandomSelector::External(
                crate::feature::FeatureRandom::Region(self.random),
            ),
            origin: self.origin,
            generator: self.generator,
            registry: self.registry,
        }
    }
}
