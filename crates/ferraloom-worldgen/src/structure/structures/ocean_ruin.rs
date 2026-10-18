use super::igloo::{read_rotation, rotation_name};
use crate::structure::{
    BoundingBox,
    nbt::string,
    processor::{ProcessorRule, RuleBlockEntityModifier, StructureProcessor},
    rule::{BlockTags, PosRuleTest, RuleTest},
    template::{StructurePlaceSettings, StructureTemplateManager, transform_block},
    template_piece::TemplateStructurePiece,
};
use crate::{
    Error, Identifier,
    codec::*,
    feature::FeatureContext,
    placement::providers::IntProvider,
    region::{Heightmap, StructureEntitySpawnFlags},
};
use ferraloom_data::{Block, properties, tags::FluidTags};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Mirror, Rotation, random::RandomSource};
use serde_json::Value;

/// Port of OceanRuinStructure.Type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OceanRuinType {
    Warm,
    Cold,
}
/// Port of OceanRuinStructure's configurable generation settings.
#[derive(Clone, Debug)]
pub struct OceanRuinStructure {
    pub biome_type: OceanRuinType,
    pub large_probability: f32,
    pub cluster_probability: f32,
}
impl OceanRuinStructure {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        Ok(Self {
            biome_type: match string(field(v, "biome_temp")?)? {
                "warm" => OceanRuinType::Warm,
                "cold" => OceanRuinType::Cold,
                _ => return Err(Error("invalid ocean ruin biome temperature".into())),
            },
            large_probability: float_range(field(v, "large_probability")?, 0.0, 1.0)?,
            cluster_probability: float_range(field(v, "cluster_probability")?, 0.0, 1.0)?,
        })
    }
    pub fn add_pieces(
        &self,
        templates: &StructureTemplateManager,
        position: [i32; 3],
        rotation: Rotation,
        random: &mut impl RandomSource,
    ) -> Vec<OceanRuinPiece> {
        let large = random.next_float() <= self.large_probability;
        let mut pieces = Vec::new();
        self.add(
            templates,
            position,
            rotation,
            random,
            large,
            if large { 0.9 } else { 0.8 },
            &mut pieces,
        );
        if large && random.next_float() <= self.cluster_probability {
            let parent = [position[0], 90, position[2]];
            let corner = transform_block([15, 0, 15], Mirror::None, rotation, [0; 3]);
            let corner = add(parent, corner);
            let bounds = BoundingBox::new(parent, corner);
            let origin = [parent[0].min(corner[0]), 90, parent[2].min(corner[2])];
            let mut next = |min: i32, max: i32| min + random.next_int_bound(max - min + 1);
            let offsets = [
                [-16 + next(1, 8), 0, 16 + next(1, 7)],
                [-16 + next(1, 8), 0, next(1, 7)],
                [-16 + next(1, 8), 0, -16 + next(4, 8)],
                [next(1, 7), 0, 16 + next(1, 7)],
                [next(1, 7), 0, -16 + next(4, 6)],
                [16 + next(1, 7), 0, 16 + next(3, 8)],
                [16 + next(1, 7), 0, next(1, 7)],
                [16 + next(1, 7), 0, -16 + next(4, 8)],
            ];
            let mut positions = offsets.map(|p| add(origin, p)).to_vec();
            let count = 4 + random.next_int_bound(5);
            for _ in 0..count {
                if positions.is_empty() {
                    continue;
                }
                let index = random.next_int_bound(positions.len() as i32) as usize;
                let p = positions.remove(index);
                let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
                let corner = add(
                    p,
                    transform_block([5, 0, 6], Mirror::None, rotation, [0; 3]),
                );
                if !BoundingBox::new(p, corner).intersects(bounds) {
                    self.add(templates, p, rotation, random, false, 0.8, &mut pieces);
                }
            }
        }
        pieces
    }
    #[allow(clippy::too_many_arguments)]
    fn add(
        &self,
        templates: &StructureTemplateManager,
        position: [i32; 3],
        rotation: Rotation,
        random: &mut impl RandomSource,
        large: bool,
        integrity: f32,
        pieces: &mut Vec<OceanRuinPiece>,
    ) {
        let (number, prefix) = if self.biome_type == OceanRuinType::Warm {
            (
                if large {
                    [4, 5, 6, 7][random.next_int_bound(4) as usize]
                } else {
                    1 + random.next_int_bound(8)
                },
                "warm",
            )
        } else {
            (
                if large {
                    [1, 2, 3, 8][random.next_int_bound(4) as usize]
                } else {
                    1 + random.next_int_bound(8)
                },
                "brick",
            )
        };
        let name = |material: &str| {
            format!(
                "minecraft:underwater_ruin/{}{material}_{number}",
                if large { "big_" } else { "" }
            )
        };
        pieces.push(OceanRuinPiece::new(
            templates,
            name(prefix).parse().expect("builtin ruin"),
            position,
            rotation,
            integrity,
            self.biome_type,
            large,
        ));
        if self.biome_type == OceanRuinType::Cold {
            for (material, integrity) in [("cracked", 0.7), ("mossy", 0.5)] {
                pieces.push(OceanRuinPiece::new(
                    templates,
                    name(material).parse().expect("builtin ruin"),
                    position,
                    rotation,
                    integrity,
                    self.biome_type,
                    large,
                ));
            }
        }
    }
}
fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}
/// Port of OceanRuinPieces.OceanRuinPiece.
#[derive(Clone, Debug)]
pub struct OceanRuinPiece {
    pub template: TemplateStructurePiece,
    pub biome_type: OceanRuinType,
    pub integrity: f32,
    pub large: bool,
}
impl OceanRuinPiece {
    pub fn new(
        templates: &StructureTemplateManager,
        location: Identifier,
        position: [i32; 3],
        rotation: Rotation,
        integrity: f32,
        biome_type: OceanRuinType,
        large: bool,
    ) -> Self {
        Self {
            template: TemplateStructurePiece::new(
                templates,
                location.to_string(),
                location,
                position,
                StructurePlaceSettings {
                    rotation,
                    ..Default::default()
                },
                0,
            ),
            biome_type,
            integrity,
            large,
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:orp".parse().expect("builtin piece id"));
        tag.insert(
            "Rot",
            string(rotation_name(self.template.settings.rotation)),
        );
        tag.insert("Integrity", Tag::Float(self.integrity));
        tag.insert(
            "BiomeType",
            string(if self.biome_type == OceanRuinType::Cold {
                "COLD"
            } else {
                "WARM"
            }),
        );
        tag.insert("IsLarge", Tag::Byte(i8::from(self.large)));
        tag
    }
    pub fn from_nbt(
        templates: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<Self, Error> {
        let name = tag
            .get_string("Template")
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let biome_type = match tag
            .get_string("BiomeType")
            .map(|s| s.to_string_lossy())
            .as_deref()
        {
            Some("COLD") => OceanRuinType::Cold,
            Some("WARM") => OceanRuinType::Warm,
            _ => return Err(Error("invalid ruin BiomeType".into())),
        };
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(
                templates,
                tag,
                name.parse()?,
                StructurePlaceSettings {
                    rotation: read_rotation(tag)?,
                    ..Default::default()
                },
            )?,
            biome_type,
            integrity: tag.get_float("Integrity").unwrap_or(0.0),
            large: tag.get_byte("IsLarge").unwrap_or(0) != 0,
        })
    }
    fn processors(&self) -> Vec<StructureProcessor> {
        let (block, replacement, loot) = if self.biome_type == OceanRuinType::Cold {
            (
                Block::GRAVEL,
                Block::SUSPICIOUS_GRAVEL,
                "minecraft:archaeology/ocean_ruin_cold",
            )
        } else {
            (
                Block::SAND,
                Block::SUSPICIOUS_SAND,
                "minecraft:archaeology/ocean_ruin_warm",
            )
        };
        vec![
            StructureProcessor::BlockRot {
                integrity: self.integrity,
                rottable: None,
            },
            StructureProcessor::BlockIgnore(vec![Block::STRUCTURE_BLOCK, Block::AIR]),
            StructureProcessor::Capped {
                delegate: Box::new(StructureProcessor::Rule(vec![ProcessorRule {
                    input: RuleTest::BlockMatch(block),
                    location: RuleTest::AlwaysTrue,
                    position: PosRuleTest::AlwaysTrue,
                    output: replacement.default_state(),
                    block_entity_modifier: RuleBlockEntityModifier::AppendLoot(
                        loot.parse().expect("builtin loot table"),
                    ),
                }])),
                limit: IntProvider::Constant(5),
            },
        ]
    }
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        let mut position = self.template.position;
        position[1] =
            context
                .region
                .height_at(Heightmap::OceanFloorWg, position[0], position[2])?;
        let size = templates.get_or_create(&self.template.location).size;
        let corner = add(
            position,
            transform_block(
                [size[0] - 1, 0, size[2] - 1],
                Mirror::None,
                self.template.settings.rotation,
                [0; 3],
            ),
        );
        let mut min_y = 512;
        let top = position[1] - 1;
        let mut area = 0;
        let ice = "minecraft:ice".parse()?;
        for z in position[2].min(corner[2])..=position[2].max(corner[2]) {
            for x in position[0].min(corner[0])..=position[0].max(corner[0]) {
                let mut y = position[1] - 1;
                loop {
                    let p = [x, y, z];
                    let state = context.region.block(p)?;
                    let water = context
                        .region
                        .fluid_tags()?
                        .contains_key(FluidTags::WATER, state.data().fluid.fluid_type);
                    if !(state.is_air() || water || tags.contains(&ice, state.block()))
                        || y <= context.region.min_y() + 1
                    {
                        break;
                    }
                    y -= 1;
                }
                min_y = min_y.min(y);
                if y < top - 2 {
                    area += 1;
                }
            }
        }
        let width = (position[0] - corner[0]).abs();
        if top - min_y > 2 && area > width - 2 {
            position[1] = min_y + 1;
        }
        self.template.position = position;
        let processors = self.processors();
        let large = self.large;
        self.template.post_process(
            context,
            templates,
            tags,
            bounds,
            reference,
            &processors,
            |marker, p, context| {
                if marker == "chest" {
                    let water = context
                        .region
                        .fluid_tags()?
                        .contains_key(FluidTags::WATER, context.region.fluid(p)?.fluid_type);
                    let chest = Block::CHEST
                        .default_state()
                        .with(properties::WATERLOGGED, water)
                        .expect("chest waterlogged");
                    context.region.set_block(p, chest, 2)?;
                    let loot = if large {
                        "minecraft:chests/underwater_ruin_big"
                    } else {
                        "minecraft:chests/underwater_ruin_small"
                    };
                    context
                        .region
                        .set_loot_table(p, &loot.parse()?, context.random)?;
                } else if marker == "drowned"
                    && context.region.spawn_structure_entity(
                        ferraloom_data::EntityType::DROWNED,
                        p.map(f64::from),
                        Some(0.0),
                        StructureEntitySpawnFlags {
                            persistent: true,
                            finalize_spawn: true,
                            with_passengers: true,
                        },
                    )?
                {
                    let state = if p[1] > context.region.sea_level {
                        Block::AIR
                    } else {
                        Block::WATER
                    };
                    context.region.set_block(p, state.default_state(), 2)?;
                }
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::processor::tests::generator;
    use ferraloom_util::random::LegacyRandomSource;
    use std::io::Read;
    #[test]
    fn ocean_ruin_cluster_candidates_layer_order_integrity_and_rng_match_vanilla() {
        let (resources, _) = generator();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/ocean_ruin_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let id: Identifier = case["id"].as_str().unwrap().parse().unwrap();
            let config = resources
                .require(
                    &id.with_path(format!("worldgen/structure/{}.json", id.path()))
                        .unwrap(),
                )
                .unwrap()
                .json()
                .unwrap();
            let structure = OceanRuinStructure::decode(&config).unwrap();
            let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
            let rotation = Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize];
            let pieces = structure.add_pieces(&templates, [-17, 90, 31], rotation, &mut random);
            let actual =
                crate::structure::nbt::sequence(pieces.iter().map(|p| Tag::Compound(p.to_nbt())));
            assert_eq!(
                actual,
                ferraloom_nbt::parse(case["nbt"].as_str().unwrap()).unwrap(),
                "{case}"
            );
            assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
            for piece in pieces {
                let tag = piece.to_nbt();
                assert_eq!(
                    OceanRuinPiece::from_nbt(&templates, &tag).unwrap().to_nbt(),
                    tag
                );
            }
        }
    }
}
