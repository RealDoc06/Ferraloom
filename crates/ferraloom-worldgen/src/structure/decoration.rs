//! ChunkGenerator#applyBiomeDecoration's structure pass. Registry and reference
//! order are inputs; a job exclusively borrows its region and random stream.
use super::{
    BoundingBox, jigsaw::JigsawStart, rule::BlockTags, template::StructureTemplateManager,
};
use crate::{
    Error, Identifier,
    decoration::{GenerationStep, StructureDecoration},
    feature::{FeatureContext, FeatureRegistry},
    generator::NoiseBasedChunkGenerator,
    region::WorldGenRegion,
};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::WorldgenRandom;
use std::collections::BTreeMap;

/// Port of WorldGenRegion's constructor RNG. Install this once on the region;
/// level.getRandom callers share it independently of structure/feature seeds.
pub fn region_random(
    compiler: &crate::density::DensityFunctionCompiler,
    chunk: [i32; 2],
) -> ferraloom_util::random::Random {
    compiler
        .positional_random(
            &"minecraft:worldgen_region_random"
                .parse()
                .expect("builtin random name"),
        )
        .at(chunk[0].wrapping_mul(16), 0, chunk[1].wrapping_mul(16))
}

/// One structure registry entry, including entries without any starts. Keeping
/// those entries is necessary for vanilla's per-step feature seed indices.
#[derive(Clone, Debug)]
pub struct StructureStep {
    pub id: Identifier,
    pub step: GenerationStep,
}
impl StructureStep {
    pub fn load(resources: &ResourceManager) -> Result<Vec<Self>, Error> {
        resources
            .registry("worldgen/structure")
            .into_iter()
            .map(|(id, resource)| {
                let value = resource.json()?;
                let name = value["step"]
                    .as_str()
                    .ok_or_else(|| Error(format!("missing structure generation step for {id}")))?;
                let step = match name {
                    "raw_generation" => GenerationStep::RawGeneration,
                    "lakes" => GenerationStep::Lakes,
                    "local_modifications" => GenerationStep::LocalModifications,
                    "underground_structures" => GenerationStep::UndergroundStructures,
                    "surface_structures" => GenerationStep::SurfaceStructures,
                    "strongholds" => GenerationStep::Strongholds,
                    "underground_ores" => GenerationStep::UndergroundOres,
                    "underground_decoration" => GenerationStep::UndergroundDecoration,
                    "fluid_springs" => GenerationStep::FluidSprings,
                    "vegetal_decoration" => GenerationStep::VegetalDecoration,
                    "top_layer_modification" => GenerationStep::TopLayerModification,
                    _ => return Err(Error(format!("unknown structure generation step {name}"))),
                };
                Ok(Self { id, step })
            })
            .collect()
    }
}

impl JigsawStart {
    /// Port of StructureStart#placeInChunk for pool pieces. Jigsaw structures
    /// inherit Structure#afterPlace's empty implementation.
    pub fn place_in_chunk(
        &self,
        context: &mut FeatureContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let Some(first) = self.pieces.first() else {
            return Ok(());
        };
        let [x, _, z] = first.bounds.center();
        let reference = [x, first.bounds.min[1], z];
        for piece in &self.pieces {
            if piece.bounds.intersects(bounds) {
                piece.element.place(
                    templates,
                    context,
                    piece.position,
                    reference,
                    piece.rotation,
                    bounds,
                    piece.liquid,
                    false,
                    tags,
                )?;
            }
        }
        Ok(())
    }
}

/// StructureDecoration implementation for jigsaw starts already resolved from
/// the decorating chunk's references. Each Vec must retain StructureManager's
/// reference order. Handwritten starts require their own postProcess ports.
pub struct JigsawStructureDecoration<'a> {
    pub registry: &'a [StructureStep],
    pub starts: &'a BTreeMap<Identifier, Vec<&'a JigsawStart>>,
    pub templates: &'a StructureTemplateManager,
    pub tags: &'a BlockTags,
    pub generator: &'a NoiseBasedChunkGenerator,
    pub features: &'a FeatureRegistry,
}
impl StructureDecoration for JigsawStructureDecoration<'_> {
    fn place_step(
        &mut self,
        step: GenerationStep,
        region: &mut WorldGenRegion<'_>,
        random: &mut WorldgenRandom,
        decoration_seed: i64,
    ) -> Result<(), Error> {
        let chunk = region.center();
        let min_x = chunk[0].wrapping_mul(16);
        let min_z = chunk[1].wrapping_mul(16);
        let bounds = BoundingBox::new(
            [min_x, region.min_y() + 1, min_z],
            [min_x + 15, region.min_y() + region.height() - 1, min_z + 15],
        );
        for (index, structure) in self.registry.iter().filter(|s| s.step == step).enumerate() {
            random.set_feature_seed(decoration_seed, index as i32, step as i32);
            if let Some(starts) = self.starts.get(&structure.id) {
                for start in starts {
                    start.place_in_chunk(
                        &mut FeatureContext {
                            region,
                            random,
                            origin: [min_x, bounds.min[1], min_z],
                            generator: self.generator,
                            registry: self.features,
                        },
                        self.templates,
                        self.tags,
                        bounds,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "decoration_tests.rs"]
mod tests;
