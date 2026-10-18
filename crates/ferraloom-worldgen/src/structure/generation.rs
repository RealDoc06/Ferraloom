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
    Error, Identifier,
    feature::PlacedFeature,
    generator::ChunkGeneratorAccess,
    placement::providers::{HeightProvider, WorldGenerationContext},
    region::Heightmap,
};
use ferraloom_data::{datapack::ResourceManager, registries::worldgen_structure_type};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
enum StructureGenerator {
    Jigsaw(Box<JigsawStructure>),
    BuriedTreasure,
    Igloo,
    EndCity,
    Shipwreck { beached: bool },
    NetherFossil { height: HeightProvider },
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
                    "minecraft:jigsaw" => StructureGenerator::Jigsaw(Box::new(
                        JigsawStructure::decode(&resource.json()?, resources, decode_feature)?,
                    )),
                    "minecraft:buried_treasure" => StructureGenerator::BuriedTreasure,
                    "minecraft:igloo" => StructureGenerator::Igloo,
                    "minecraft:end_city" => StructureGenerator::EndCity,
                    "minecraft:shipwreck" => StructureGenerator::Shipwreck {
                        beached: crate::codec::boolean(crate::codec::field(
                            &resource.json()?,
                            "is_beached",
                        )?)?,
                    },
                    "minecraft:nether_fossil" => StructureGenerator::NetherFossil {
                        height: HeightProvider::decode(crate::codec::field(
                            &resource.json()?,
                            "height",
                        )?)?,
                    },
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
            kind @ (StructureGenerator::BuriedTreasure
            | StructureGenerator::Igloo
            | StructureGenerator::Shipwreck { .. }) => {
                let [x, z] = context.chunk.map(|c| c.wrapping_mul(16).wrapping_add(8));
                if !could_exist(x, z, context.min_y - 1, context.min_y + context.height - 1)? {
                    return Ok(StructureStart::invalid());
                }
                let map = if matches!(
                    kind,
                    StructureGenerator::BuriedTreasure
                        | StructureGenerator::Shipwreck { beached: false }
                ) {
                    Heightmap::OceanFloorWg
                } else {
                    Heightmap::WorldSurfaceWg
                };
                let y = generator
                    .get_base_height(x, z, map, context.min_y, context.height)?
                    .wrapping_sub(1);
                let position = [x, y, z];
                if !valid(&(context.biome)(position.map(|v| v >> 2))?) {
                    return Ok(StructureStart::invalid());
                }
                let pieces = if matches!(kind, StructureGenerator::BuriedTreasure) {
                    vec![StructurePiece::BuriedTreasure(BuriedTreasurePiece::new([
                        x + 1,
                        90,
                        z + 1,
                    ]))]
                } else {
                    use ferraloom_util::{
                        Rotation,
                        random::{Algorithm, RandomSource, WorldgenRandom},
                    };
                    let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
                    random.set_large_feature_seed(context.seed, context.chunk[0], context.chunk[1]);
                    let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
                    match kind {
                        StructureGenerator::Igloo => super::structures::igloo::add_pieces(
                            context.templates,
                            [x - 8, 90, z - 8],
                            rotation,
                            &mut random,
                        )?
                        .into_iter()
                        .map(|p| StructurePiece::Igloo(Box::new(p)))
                        .collect(),
                        StructureGenerator::Shipwreck { beached } => {
                            let mut piece =
                                super::structures::shipwreck::ShipwreckPiece::add_random(
                                    context.templates,
                                    [x - 8, 90, z - 8],
                                    rotation,
                                    &mut random,
                                    *beached,
                                );
                            piece.adjust_generation_height(
                                context.templates,
                                generator,
                                context.min_y,
                                context.height,
                                &mut random,
                            )?;
                            vec![StructurePiece::Shipwreck(Box::new(piece))]
                        }
                        _ => unreachable!("non-template generators handled above"),
                    }
                };
                return Ok(StructureStart::new(
                    id.clone(),
                    context.chunk,
                    references,
                    settings.terrain,
                    pieces,
                ));
            }
            StructureGenerator::EndCity => {
                use ferraloom_util::{
                    Rotation,
                    random::{Algorithm, RandomSource, WorldgenRandom},
                };
                let [x, z] = context.chunk.map(|c| c.wrapping_mul(16).wrapping_add(7));
                if !could_exist(x, z, context.min_y - 1, context.min_y + context.height - 1)? {
                    return Ok(StructureStart::invalid());
                }
                let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
                random.set_large_feature_seed(context.seed, context.chunk[0], context.chunk[1]);
                let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
                let dx = if matches!(rotation, Rotation::Clockwise90 | Rotation::Clockwise180) {
                    -5
                } else {
                    5
                };
                let dz = if matches!(
                    rotation,
                    Rotation::Clockwise180 | Rotation::CounterClockwise90
                ) {
                    -5
                } else {
                    5
                };
                let mut y = i32::MAX;
                for [sx, sz] in [[x, z], [x, z + dz], [x + dx, z], [x + dx, z + dz]] {
                    y = y.min(
                        generator.get_base_height(
                            sx,
                            sz,
                            Heightmap::WorldSurfaceWg,
                            context.min_y,
                            context.height,
                        )? - 1,
                    );
                }
                if y < 60 {
                    return Ok(StructureStart::invalid());
                }
                let position = [x, y, z];
                if !valid(&(context.biome)(position.map(|v| v >> 2))?) {
                    return Ok(StructureStart::invalid());
                }
                let pieces = super::structures::end_city::start_house_tower(
                    context.templates,
                    position,
                    rotation,
                    &mut random,
                )
                .into_iter()
                .map(|p| StructurePiece::EndCity(Box::new(p)))
                .collect();
                return Ok(StructureStart::new(
                    id.clone(),
                    context.chunk,
                    references,
                    settings.terrain,
                    pieces,
                ));
            }
            StructureGenerator::NetherFossil { height } => {
                use ferraloom_data::Block;
                use ferraloom_util::{
                    Direction,
                    random::{Algorithm, RandomSource, WorldgenRandom},
                };
                let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
                random.set_large_feature_seed(context.seed, context.chunk[0], context.chunk[1]);
                let x = context.chunk[0].wrapping_mul(16) + random.next_int_bound(16);
                let z = context.chunk[1].wrapping_mul(16) + random.next_int_bound(16);
                let generation = WorldGenerationContext::new(
                    generator.min_y(),
                    generator.gen_depth(),
                    context.min_y,
                    context.height,
                    generator.sea_level(),
                );
                let mut y = height.sample(&mut random, generation);
                let column = generator
                    .get_base_column(x, z, context.min_y, context.height)?
                    .ok_or_else(|| Error("Unsupported nether fossil base column".into()))?;
                while y > generator.sea_level() {
                    let current = column.get_block(y);
                    y -= 1;
                    let below = column.get_block(y);
                    if current.is_air()
                        && (below.block() == Block::SOUL_SAND
                            || below.cached_sturdy_face(Direction::Up).ok_or_else(|| {
                                Error(
                                    "Unsupported nether fossil face against EmptyBlockGetter"
                                        .into(),
                                )
                            })?)
                    {
                        break;
                    }
                }
                if y <= generator.sea_level() {
                    return Ok(StructureStart::invalid());
                }
                let position = [x, y, z];
                if !valid(&(context.biome)(position.map(|v| v >> 2))?) {
                    return Ok(StructureStart::invalid());
                }
                let piece = super::structures::nether_fossil::NetherFossilPiece::add_random(
                    context.templates,
                    position,
                    &mut random,
                );
                return Ok(StructureStart::new(
                    id.clone(),
                    context.chunk,
                    references,
                    settings.terrain,
                    vec![StructurePiece::NetherFossil(Box::new(piece))],
                ));
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
        if pieces.is_empty() {
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
