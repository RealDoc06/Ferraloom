//! ChunkGenerator#applyBiomeDecoration's structure pass. Registry and reference
//! order are inputs; a job exclusively borrows its region and random stream.
use super::{
    BoundingBox,
    jigsaw::JigsawStart,
    rule::BlockTags,
    start::{StructurePiece, StructureStart},
    template::StructureTemplateManager,
};
use crate::{
    Error, Identifier,
    decoration::{GenerationStep, StructureDecoration},
    feature::{FeatureContext, FeatureRegistry},
    generator::{ChunkGeneratorAccess, NoiseBasedChunkGenerator},
    region::{ChunkStatus, WorldGenRegion},
};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::WorldgenRandom;
use std::collections::BTreeMap;

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

impl StructureStart {
    /// Port of StructureStart#placeInChunk. Starts are exclusively owned while
    /// postProcess may change piece bounds, chest flags or entity-spawn flags.
    pub fn place_in_chunk(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let Some(first) = self.pieces.first() else {
            return Ok(());
        };
        let first = first.bounds();
        let [x, _, z] = first.center();
        let reference = [x, first.min[1], z];
        for piece in &mut self.pieces {
            if !piece.bounds().intersects(bounds) {
                continue;
            }
            match piece {
                StructurePiece::Igloo(piece) => {
                    piece.post_process(context, templates, tags, bounds, reference)?
                }
                StructurePiece::BuriedTreasure(piece) => piece.post_process(context, bounds)?,
                StructurePiece::Jigsaw(piece) => {
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
        }
        // JigsawStructure inherits the empty Structure#afterPlace method.
        Ok(())
    }
}

/// StructureManager-backed decoration using the live region's references and
/// starts. Reference-set iteration order survives save/reload. A start is taken
/// from its origin chunk only for its synchronous postProcess call.
pub struct RegionStructureDecoration<'a> {
    pub registry: &'a [StructureStep],
    pub templates: &'a StructureTemplateManager,
    pub tags: &'a BlockTags,
    pub generator: &'a dyn ChunkGeneratorAccess,
    pub features: &'a FeatureRegistry,
}
impl StructureDecoration for RegionStructureDecoration<'_> {
    fn place_step(
        &mut self,
        step: GenerationStep,
        region: &mut WorldGenRegion<'_>,
        random: &mut WorldgenRandom,
        decoration_seed: i64,
    ) -> Result<(), Error> {
        let chunk = region.center();
        let [x, z] = chunk.map(|p| p.wrapping_mul(16));
        let bounds = BoundingBox::new(
            [x, region.min_y() + 1, z],
            [x + 15, region.min_y() + region.height() - 1, z + 15],
        );
        for (index, structure) in self.registry.iter().filter(|s| s.step == step).enumerate() {
            random.set_feature_seed(decoration_seed, index as i32, step as i32);
            let references = region
                .chunk(chunk, ChunkStatus::StructureReferences)?
                .structures
                .references
                .get(&structure.id)
                .map(|r| r.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for source in references {
                let source = ferraloom_util::ChunkPos::unpack(source);
                let origin = [source.x, source.z];
                let Some(mut start) = region.take_structure_start(origin, &structure.id)? else {
                    continue;
                };
                let result = if start.is_valid() {
                    start.place_in_chunk(
                        &mut FeatureContext {
                            region,
                            random,
                            origin: [x, bounds.min[1], z],
                            generator: self.generator,
                            registry: self.features,
                        },
                        self.templates,
                        self.tags,
                        bounds,
                    )
                } else {
                    Ok(())
                };
                // Restore even when a game-backed operation returns Unsupported.
                region.restore_structure_start(origin, structure.id.clone(), start)?;
                result?;
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
