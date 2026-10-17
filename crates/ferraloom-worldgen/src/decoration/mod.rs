//! Vanilla decoration stage ordering and structure/feature interleaving.
use crate::{Error, region::WorldGenRegion};
use ferraloom_util::random::WorldgenRandom;

/// Port of GenerationStep.Decoration, in vanilla ordinal order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum GenerationStep {
    RawGeneration,
    Lakes,
    LocalModifications,
    UndergroundStructures,
    SurfaceStructures,
    Strongholds,
    UndergroundOres,
    UndergroundDecoration,
    FluidSprings,
    VegetalDecoration,
    TopLayerModification,
}
impl GenerationStep {
    pub const ALL: [Self; 11] = [
        Self::RawGeneration,
        Self::Lakes,
        Self::LocalModifications,
        Self::UndergroundStructures,
        Self::SurfaceStructures,
        Self::Strongholds,
        Self::UndergroundOres,
        Self::UndergroundDecoration,
        Self::FluidSprings,
        Self::VegetalDecoration,
        Self::TopLayerModification,
    ];
}

/// Called before biome features at every step. The structure adapter iterates
/// its registry in vanilla order and calls setFeatureSeed for each structure's
/// index within this step, even when there are no starts for that structure.
pub trait StructureDecoration {
    fn place_step(
        &mut self,
        step: GenerationStep,
        region: &mut WorldGenRegion<'_>,
        random: &mut WorldgenRandom,
        decoration_seed: i64,
    ) -> Result<(), Error>;
}

pub mod sorter;
