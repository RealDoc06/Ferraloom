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

use crate::{
    Identifier,
    feature::{FeatureContext, FeatureRegistry},
    generator::ChunkGeneratorAccess,
};
use ferraloom_util::random::{Random, XoroshiroRandomSource};
use std::collections::BTreeSet;

/// Precomputed global feature indices for one biome source. Immutable plans are
/// shared across generation jobs; only the region and RNG mutate per invocation.
#[derive(Clone, Debug)]
pub struct DecorationPlan {
    pub steps: Vec<sorter::StepFeatureData<Identifier>>,
    possible_biomes: BTreeSet<Identifier>,
}
impl DecorationPlan {
    pub fn new(registry: &FeatureRegistry, possible_biomes: &[Identifier]) -> Result<Self, Error> {
        let sources = possible_biomes
            .iter()
            .map(|id| {
                registry
                    .biome_features
                    .get(id)
                    .cloned()
                    .ok_or_else(|| Error(format!("missing biome generation settings {id}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            steps: sorter::build_features_per_step(&sources)?,
            possible_biomes: possible_biomes.iter().cloned().collect(),
        })
    }
    /// Port of ChunkGenerator#applyBiomeDecoration. Biomes come from all
    /// sections in the 3x3 neighborhood; features are unioned, sorted by global
    /// per-step indices, and seeded independently of structure count.
    pub fn apply_biome_decoration(
        &self,
        region: &mut WorldGenRegion<'_>,
        generator: &dyn ChunkGeneratorAccess,
        registry: &FeatureRegistry,
        structures: &mut impl StructureDecoration,
    ) -> Result<(), Error> {
        let center = region.center();
        let mut biomes = BTreeSet::new();
        for z in center[1] - 1..=center[1] + 1 {
            for x in center[0] - 1..=center[0] + 1 {
                let chunk = &region
                    .chunk([x, z], crate::region::ChunkStatus::Empty)?
                    .chunk;
                for section in &chunk.sections {
                    for x in 0..4 {
                        for y in 0..4 {
                            for z in 0..4 {
                                let id = section.biomes().get(x, y, z);
                                let name = &generator.data().biome_names[id.0 as usize];
                                if self.possible_biomes.contains(name) {
                                    biomes.insert(name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        self.apply_for_biomes(region, generator, registry, structures, &biomes)
    }

    /// The caller supplies exactly the possible-biome-filtered union of every
    /// section in the vanilla 3x3 neighborhood. Fresh flat chunks have a fixed
    /// biome, so local-only features can use that union without borrowing
    /// neighbors. General features still require the full owned write region.
    pub fn apply_for_biomes(
        &self,
        region: &mut WorldGenRegion<'_>,
        generator: &dyn ChunkGeneratorAccess,
        registry: &FeatureRegistry,
        structures: &mut impl StructureDecoration,
        biomes: &BTreeSet<Identifier>,
    ) -> Result<(), Error> {
        if !biomes.is_subset(&self.possible_biomes) {
            return Err(Error("decoration biome union exceeds biome source".into()));
        }
        region.set_block_tags(generator.data().block_tags.clone());
        region.set_fluid_tags(generator.data().fluid_tags.clone());
        let center = region.center();
        let origin = [center[0] * 16, region.min_y(), center[1] * 16];
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(0)));
        let decoration_seed = random.set_decoration_seed(region.seed, origin[0], origin[2]);
        for step in 0..self.steps.len().max(GenerationStep::ALL.len()) {
            if let Some(&generation_step) = GenerationStep::ALL.get(step) {
                structures.place_step(generation_step, region, &mut random, decoration_seed)?;
            }
            let Some(data) = self.steps.get(step) else {
                continue;
            };
            let mut features = BTreeSet::new();
            for biome in biomes {
                if let Some(entries) = registry
                    .biome_features
                    .get(biome)
                    .and_then(|steps| steps.get(step))
                {
                    for feature in entries {
                        features.insert(*data.indices.get(feature).ok_or_else(|| {
                            Error(format!("feature missing from sorted step: {feature}"))
                        })?);
                    }
                }
            }
            for index in features {
                let id = &data.features[index];
                let feature = registry
                    .placed
                    .get(id)
                    .ok_or_else(|| Error(format!("placed feature unavailable: {id}")))?;
                random.set_feature_seed(decoration_seed, index as i32, step as i32);
                feature.place(
                    &mut FeatureContext {
                        level: crate::feature::WorldGenLevel::Region(region),
                        random: crate::feature::FeatureRandomSelector::External(
                            crate::feature::FeatureRandom::Region(&mut random),
                        ),
                        origin,
                        generator,
                        registry,
                    },
                    true,
                )?;
            }
        }
        Ok(())
    }
}
