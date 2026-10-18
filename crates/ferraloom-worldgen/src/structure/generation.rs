//! Structure.GenerationContext and the registered structure generation dispatch.
use super::{
    jigsaw_structure::JigsawStructure,
    pipeline::StructureRegistry,
    pool::StructurePools,
    start::{StructurePiece, StructureStart},
    structures::buried_treasure::BuriedTreasurePiece,
    template::StructureTemplateManager,
};
use crate::{
    Error, Identifier, feature::PlacedFeature, generator::ChunkGeneratorAccess,
    placement::providers::WorldGenerationContext, region::Heightmap,
};
use ferraloom_data::{datapack::ResourceManager, registries::worldgen_structure_type};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
enum StructureGenerator {
    Jigsaw(JigsawStructure),
    BuriedTreasure,
    Unsupported(worldgen_structure_type::Id),
}
/// Immutable decoded generation definitions, shared by independent chunk jobs.
#[derive(Clone, Debug)]
pub struct StructureGenerators {
    pub registry: StructureRegistry,
    generators: BTreeMap<Identifier, StructureGenerator>,
}
/// The biome resolver samples the source with the generation climate sampler,
/// independently of chunk biomes. Coordinates passed to it are quart positions.
/// All mutable sampler caches and RNGs are local to the generation job.
pub struct StructureGenerationContext<'a> {
    pub seed: i64,
    pub chunk: [i32; 2],
    pub min_y: i32,
    pub height: i32,
    pub generator: &'a dyn ChunkGeneratorAccess,
    pub templates: &'a StructureTemplateManager,
    pub pools: &'a StructurePools,
    pub biome: &'a mut dyn FnMut([i32; 3]) -> Result<Identifier, Error>,
}
impl StructureGenerators {
    pub fn load(
        resources: &ResourceManager,
        registry: StructureRegistry,
        decode_feature: &mut impl FnMut(&serde_json::Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let generators = resources
            .registry("worldgen/structure")
            .into_iter()
            .map(|(id, resource)| {
                let definition = registry
                    .structures
                    .get(&id)
                    .ok_or_else(|| Error(format!("missing structure settings {id}")))?;
                let generator = match definition.kind.name() {
                    "minecraft:jigsaw" => StructureGenerator::Jigsaw(JigsawStructure::decode(
                        &resource.json()?,
                        resources,
                        decode_feature,
                    )?),
                    "minecraft:buried_treasure" => StructureGenerator::BuriedTreasure,
                    _ => StructureGenerator::Unsupported(definition.kind),
                };
                Ok((id, generator))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            registry,
            generators,
        })
    }
    /// Port of Structure#generate, including the final generation-point biome
    /// test and retention of the reference count from a previous start.
    pub fn generate(
        &self,
        id: &Identifier,
        references: i32,
        context: &mut StructureGenerationContext<'_>,
    ) -> Result<StructureStart, Error> {
        let settings = self
            .registry
            .structures
            .get(id)
            .ok_or_else(|| Error(format!("unknown structure {id}")))?;
        let valid = |biome: &Identifier| settings.biomes.contains(biome);
        let biome = &mut context.biome;
        let mut could_exist = |x: i32, z: i32, min: i32, max: i32| {
            for y in (min >> 2)..=(max >> 2) {
                if valid(&biome([x >> 2, y, z >> 2])?) {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let generator = context.generator;
        let (position, pieces) = match self
            .generators
            .get(id)
            .ok_or_else(|| Error(format!("unknown structure {id}")))?
        {
            StructureGenerator::Jigsaw(structure) => {
                let result = structure.find_generation_point(
                    context.pools,
                    context.templates,
                    context.seed,
                    context.chunk,
                    WorldGenerationContext::new(
                        generator.min_y(),
                        generator.gen_depth(),
                        context.min_y,
                        context.height,
                        generator.sea_level(),
                    ),
                    context.min_y,
                    context.height,
                    &mut |x, z, map| {
                        generator.get_base_height(x, z, map, context.min_y, context.height)
                    },
                    &mut could_exist,
                )?;
                let Some(start) = result else {
                    return Ok(StructureStart::invalid());
                };
                (
                    start.position,
                    start
                        .pieces
                        .into_iter()
                        .map(StructurePiece::Jigsaw)
                        .collect(),
                )
            }
            StructureGenerator::BuriedTreasure => {
                let [x, z] = context.chunk.map(|c| c.wrapping_mul(16).wrapping_add(8));
                if !could_exist(x, z, context.min_y - 1, context.min_y + context.height - 1)? {
                    return Ok(StructureStart::invalid());
                }
                let y = generator
                    .get_base_height(x, z, Heightmap::OceanFloorWg, context.min_y, context.height)?
                    .wrapping_sub(1);
                let position = [x, y, z];
                (
                    position,
                    vec![StructurePiece::BuriedTreasure(BuriedTreasurePiece::new([
                        context.chunk[0].wrapping_mul(16).wrapping_add(9),
                        90,
                        context.chunk[1].wrapping_mul(16).wrapping_add(9),
                    ]))],
                )
            }
            StructureGenerator::Unsupported(kind) => {
                return Err(Error(format!(
                    "Unsupported structure generator {}",
                    kind.name()
                )));
            }
        };
        if !valid(&(context.biome)(position.map(|v| v >> 2))?) {
            return Ok(StructureStart::invalid());
        }
        Ok(StructureStart::new(
            id.clone(),
            context.chunk,
            references,
            settings.terrain,
            pieces,
        ))
    }
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
