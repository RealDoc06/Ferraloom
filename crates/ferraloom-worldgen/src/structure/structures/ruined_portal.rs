use super::igloo::rotation_name;
use crate::{
    Error, Identifier,
    biome_manager::BiomeTemperatures,
    codec,
    feature::FeatureContext,
    generator::ChunkGeneratorAccess,
    region::Heightmap,
    structure::{
        BoundingBox,
        processor::{BlockSet, ProcessorRule, StructureProcessor},
        rule::{BlockTags, PosRuleTest, RuleTest},
        template::{StructurePlaceSettings, StructureTemplateManager},
        template_piece::TemplateStructurePiece,
    },
};
use ferraloom_data::{Block, properties};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Direction, Mirror, Plane, Rotation, random::RandomSource};
use serde_json::Value;

/// Port of RuinedPortalPiece.VerticalPlacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerticalPlacement {
    OnLandSurface,
    PartlyBuried,
    OnOceanFloor,
    InMountain,
    Underground,
    InNether,
}
impl VerticalPlacement {
    pub fn name(self) -> &'static str {
        match self {
            Self::OnLandSurface => "on_land_surface",
            Self::PartlyBuried => "partly_buried",
            Self::OnOceanFloor => "on_ocean_floor",
            Self::InMountain => "in_mountain",
            Self::Underground => "underground",
            Self::InNether => "in_nether",
        }
    }
    fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "on_land_surface" => Ok(Self::OnLandSurface),
            "partly_buried" => Ok(Self::PartlyBuried),
            "on_ocean_floor" => Ok(Self::OnOceanFloor),
            "in_mountain" => Ok(Self::InMountain),
            "underground" => Ok(Self::Underground),
            "in_nether" => Ok(Self::InNether),
            _ => Err(Error(format!(
                "invalid ruined portal vertical placement {s}"
            ))),
        }
    }
    pub fn heightmap(self) -> Heightmap {
        if self == Self::OnOceanFloor {
            Heightmap::OceanFloorWg
        } else {
            Heightmap::WorldSurfaceWg
        }
    }
}
/// Port of RuinedPortalPiece.Properties.
#[derive(Clone, Copy, Debug)]
pub struct PortalProperties {
    pub cold: bool,
    pub mossiness: f32,
    pub air_pocket: bool,
    pub overgrown: bool,
    pub vines: bool,
    pub replace_with_blackstone: bool,
}
impl PortalProperties {
    fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        let boolean = |name| {
            tag.get_byte(name)
                .map(|b| b != 0)
                .ok_or_else(|| Error(format!("missing portal property {name}")))
        };
        Ok(Self {
            cold: boolean("cold")?,
            mossiness: tag
                .get("mossiness")
                .and_then(Tag::as_float)
                .ok_or_else(|| Error("missing portal mossiness".into()))?,
            air_pocket: boolean("air_pocket")?,
            overgrown: boolean("overgrown")?,
            vines: boolean("vines")?,
            replace_with_blackstone: boolean("replace_with_blackstone")?,
        })
    }
    fn to_nbt(self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for (key, b) in [
            ("cold", self.cold),
            ("air_pocket", self.air_pocket),
            ("overgrown", self.overgrown),
            ("vines", self.vines),
            ("replace_with_blackstone", self.replace_with_blackstone),
        ] {
            tag.insert(key, Tag::Byte(i8::from(b)));
        }
        tag.insert("mossiness", Tag::Float(self.mossiness));
        tag
    }
}
#[derive(Clone, Debug)]
struct PortalSetup {
    placement: VerticalPlacement,
    air_pocket_probability: f32,
    mossiness: f32,
    overgrown: bool,
    vines: bool,
    can_be_cold: bool,
    replace_with_blackstone: bool,
    weight: f32,
}
/// Port of RuinedPortalStructure's weighted setups and floor projection.
#[derive(Clone, Debug)]
pub struct RuinedPortalStructure {
    setups: Vec<PortalSetup>,
}
impl RuinedPortalStructure {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let setups = codec::list(codec::field(v, "setups")?)?
            .iter()
            .map(|s| {
                let weight = codec::float(codec::field(s, "weight")?)?;
                if weight <= 0.0 {
                    return Err(Error("portal setup requires a positive weight".into()));
                }
                Ok(PortalSetup {
                    placement: VerticalPlacement::parse(codec::string(codec::field(
                        s,
                        "placement",
                    )?)?)?,
                    air_pocket_probability: codec::float_range(
                        codec::field(s, "air_pocket_probability")?,
                        0.0,
                        1.0,
                    )?,
                    mossiness: codec::float_range(codec::field(s, "mossiness")?, 0.0, 1.0)?,
                    overgrown: codec::boolean(codec::field(s, "overgrown")?)?,
                    vines: codec::boolean(codec::field(s, "vines")?)?,
                    can_be_cold: codec::boolean(codec::field(s, "can_be_cold")?)?,
                    replace_with_blackstone: codec::boolean(codec::field(
                        s,
                        "replace_with_blackstone",
                    )?)?,
                    weight,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if setups.is_empty() {
            return Err(Error("ruined portal setups cannot be empty".into()));
        }
        Ok(Self { setups })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        &self,
        templates: &StructureTemplateManager,
        temperatures: &BiomeTemperatures,
        generator: &dyn ChunkGeneratorAccess,
        random: &mut impl RandomSource,
        chunk: [i32; 2],
        min_y: i32,
        height: i32,
        biome: &mut dyn FnMut([i32; 3]) -> Result<Identifier, Error>,
        valid_biome: &dyn Fn(&Identifier) -> bool,
    ) -> Result<Option<RuinedPortalPiece>, Error> {
        let setup = if self.setups.len() == 1 {
            &self.setups[0]
        } else {
            let total = self.setups.iter().fold(0.0_f32, |sum, s| sum + s.weight);
            let mut pick = random.next_float();
            self.setups
                .iter()
                .find(|s| {
                    pick -= s.weight / total;
                    pick < 0.0
                })
                .ok_or_else(|| Error("ruined portal setup selection exhausted".into()))?
        };
        let air_pocket = setup.air_pocket_probability == 1.0
            || (setup.air_pocket_probability != 0.0
                && random.next_float() < setup.air_pocket_probability);
        let giant = random.next_float() < 0.05;
        let index = random.next_int_bound(if giant { 3 } else { 10 }) + 1;
        let location: Identifier = format!(
            "minecraft:ruined_portal/{}{index}",
            if giant { "giant_portal_" } else { "portal_" }
        )
        .parse()?;
        let template = templates.get_or_create(&location);
        let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
        let mirror = if random.next_float() < 0.5 {
            Mirror::None
        } else {
            Mirror::FrontBack
        };
        let settings = StructurePlaceSettings {
            rotation,
            mirror,
            pivot: [template.size[0] / 2, 0, template.size[2] / 2],
            ..Default::default()
        };
        let mut position = [chunk[0] * 16, 0, chunk[1] * 16];
        let bounds = template.bounding_box(&settings, position);
        let center = bounds.center();
        let surface = generator.get_base_height(
            center[0],
            center[2],
            setup.placement.heightmap(),
            min_y,
            height,
        )? - 1;
        let lowest = min_y + 15;

        let mut y = match setup.placement {
            VerticalPlacement::InNether => {
                if air_pocket {
                    32 + random.next_int_bound(69)
                } else if random.next_float() < 0.5 {
                    27 + random.next_int_bound(3)
                } else {
                    29 + random.next_int_bound(72)
                }
            }
            VerticalPlacement::InMountain => {
                random_within(random, 70, surface - (bounds.max[1] - bounds.min[1] + 1))
            }
            VerticalPlacement::Underground => random_within(
                random,
                lowest,
                surface - (bounds.max[1] - bounds.min[1] + 1),
            ),
            VerticalPlacement::PartlyBuried => {
                surface - (bounds.max[1] - bounds.min[1] + 1) + 2 + random.next_int_bound(7)
            }
            _ => surface,
        };
        let mut columns = Vec::new();
        for [x, z] in [
            [bounds.min[0], bounds.min[2]],
            [bounds.max[0], bounds.min[2]],
            [bounds.min[0], bounds.max[2]],
            [bounds.max[0], bounds.max[2]],
        ] {
            columns.push(
                generator
                    .get_base_column(x, z, min_y, height)?
                    .ok_or_else(|| Error("Unsupported: portal base column".into()))?,
            );
        }
        let map = setup.placement.heightmap();
        while y > lowest {
            let supported = columns
                .iter()
                .filter(|c| {
                    let state = c.get_block(y);
                    generator.data().heightmap_matches(map, state)
                })
                .count();
            if supported >= 3 {
                break;
            }
            y -= 1;
        }
        position[1] = y;
        if !valid_biome(&biome(position.map(|v| v >> 2))?) {
            return Ok(None);
        }
        let cold = if setup.can_be_cold {
            let id = biome(position.map(|v| v >> 2))?;
            if !temperatures.climates.contains_key(&id) {
                return Err(Error(format!("unknown biome climate {id}")));
            }
            temperatures.temperature(&id, position, generator.sea_level()) < 0.15
        } else {
            false
        };
        Ok(Some(RuinedPortalPiece {
            template: TemplateStructurePiece::new(
                templates,
                location.to_string(),
                location,
                position,
                settings,
                0,
            ),
            placement: setup.placement,
            properties: PortalProperties {
                cold,
                mossiness: setup.mossiness,
                air_pocket,
                overgrown: setup.overgrown,
                vines: setup.vines,
                replace_with_blackstone: setup.replace_with_blackstone,
            },
        }))
    }
}

/// Port of RuinedPortalPiece. Processor settings are rebuilt from persisted
/// properties; terrain spreading consumes the caller's decoration stream.
#[derive(Clone, Debug)]
pub struct RuinedPortalPiece {
    pub template: TemplateStructurePiece,
    pub placement: VerticalPlacement,
    pub properties: PortalProperties,
}
impl RuinedPortalPiece {
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:ruined_portal".parse().expect("builtin piece"));
        tag.insert(
            "Rotation",
            Tag::String(rotation_name(self.template.settings.rotation).into()),
        );
        tag.insert(
            "Mirror",
            Tag::String(
                match self.template.settings.mirror {
                    Mirror::None => "NONE",
                    Mirror::FrontBack => "FRONT_BACK",
                    Mirror::LeftRight => "LEFT_RIGHT",
                }
                .into(),
            ),
        );
        tag.insert(
            "VerticalPlacement",
            Tag::String(self.placement.name().into()),
        );
        tag.insert("Properties", Tag::Compound(self.properties.to_nbt()));
        tag
    }
    pub fn from_nbt(
        templates: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<Self, Error> {
        let text = |key| {
            tag.get_string(key)
                .map(|s| s.to_string_lossy())
                .ok_or_else(|| Error(format!("missing portal {key}")))
        };
        let location: Identifier = text("Template")?.parse()?;
        let template = templates.get_or_create(&location);
        let mirror = match text("Mirror")?.as_ref() {
            "NONE" => Mirror::None,
            "FRONT_BACK" => Mirror::FrontBack,
            "LEFT_RIGHT" => Mirror::LeftRight,
            _ => return Err(Error("invalid portal mirror".into())),
        };
        let rotation = match text("Rotation")?.as_ref() {
            "NONE" => Rotation::None,
            "CLOCKWISE_90" => Rotation::Clockwise90,
            "CLOCKWISE_180" => Rotation::Clockwise180,
            "COUNTERCLOCKWISE_90" => Rotation::CounterClockwise90,
            _ => return Err(Error("invalid portal rotation".into())),
        };
        let settings = StructurePlaceSettings {
            rotation,
            mirror,
            pivot: [template.size[0] / 2, 0, template.size[2] / 2],
            ..Default::default()
        };
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(templates, tag, location, settings)?,
            placement: VerticalPlacement::parse(&text("VerticalPlacement")?)?,
            properties: PortalProperties::from_nbt(
                tag.get_compound("Properties")
                    .ok_or_else(|| Error("missing portal Properties".into()))?,
            )?,
        })
    }
    fn processors(&self) -> Vec<StructureProcessor> {
        let replace = |input, block: Block| ProcessorRule {
            input,
            location: RuleTest::AlwaysTrue,
            position: PosRuleTest::AlwaysTrue,
            output: block.default_state(),
            block_entity_modifier: Default::default(),
        };
        let random_match = |block, probability| RuleTest::RandomBlockMatch { block, probability };
        let lava = if self.placement == VerticalPlacement::OnOceanFloor {
            replace(RuleTest::BlockMatch(Block::LAVA), Block::MAGMA_BLOCK)
        } else if self.properties.cold {
            replace(RuleTest::BlockMatch(Block::LAVA), Block::NETHERRACK)
        } else {
            replace(random_match(Block::LAVA, 0.2), Block::MAGMA_BLOCK)
        };
        let mut rules = vec![
            replace(random_match(Block::GOLD_BLOCK, 0.3), Block::AIR),
            lava,
        ];
        if !self.properties.cold {
            rules.push(replace(
                random_match(Block::NETHERRACK, 0.07),
                Block::MAGMA_BLOCK,
            ));
        }
        let mut ignored = vec![Block::STRUCTURE_BLOCK];
        if !self.properties.air_pocket {
            ignored.push(Block::AIR);
        }
        let mut processors = vec![
            StructureProcessor::BlockIgnore(ignored),
            StructureProcessor::Rule(rules),
            StructureProcessor::BlockAge {
                mossiness: self.properties.mossiness,
            },
            StructureProcessor::ProtectedBlocks(BlockSet::Tag(
                "minecraft:features_cannot_replace"
                    .parse()
                    .expect("builtin tag"),
            )),
            StructureProcessor::LavaSubmergedBlock,
        ];
        if self.properties.replace_with_blackstone {
            processors.push(StructureProcessor::BlackstoneReplace);
        }
        processors
    }
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: &mut BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        let b = templates
            .get_or_create(&self.template.location)
            .bounding_box(&self.template.settings, self.template.position);
        if !bounds.contains(b.center()) {
            return Ok(());
        }
        *bounds = bounds.encapsulating(b);
        let processors = self.processors();
        self.template.post_process(
            context,
            templates,
            tags,
            *bounds,
            reference,
            &processors,
            |_, _, _| Ok(()),
        )?;
        self.spread_netherrack(context, tags)?;
        let b = self.template.data.bounds;
        for x in (b.min[0] + 1)..b.max[0] {
            for z in (b.min[2] + 1)..b.max[2] {
                if context.region.block([x, b.min[1], z])?.block() == Block::NETHERRACK {
                    self.drip_column(context, tags, [x, b.min[1] - 1, z])?;
                }
            }
        }
        if self.properties.vines || self.properties.overgrown {
            for z in b.min[2]..=b.max[2] {
                for y in b.min[1]..=b.max[1] {
                    for x in b.min[0]..=b.max[0] {
                        let p = [x, y, z];
                        if self.properties.vines {
                            self.maybe_vines(context, p)?;
                        }
                        if self.properties.overgrown {
                            self.maybe_leaves(context, p)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn replaceable(&self, state: ferraloom_data::BlockState, tags: &BlockTags) -> bool {
        !matches!(state.block(), Block::AIR | Block::OBSIDIAN)
            && !tags.contains(
                &"minecraft:features_cannot_replace"
                    .parse()
                    .expect("builtin tag"),
                state.block(),
            )
            && (self.placement == VerticalPlacement::InNether || state.block() != Block::LAVA)
    }
    fn place_netherrack(
        &self,
        context: &mut FeatureContext<'_, '_>,
        tags: &BlockTags,
        p: [i32; 3],
    ) -> Result<(), Error> {
        if self.replaceable(context.region.block(p)?, tags) {
            let block = if !self.properties.cold && context.random.next_float() < 0.07 {
                Block::MAGMA_BLOCK
            } else {
                Block::NETHERRACK
            };
            context.region.set_block(p, block.default_state(), 3)?;
        }
        Ok(())
    }
    fn drip_column(
        &self,
        context: &mut FeatureContext<'_, '_>,
        tags: &BlockTags,
        mut p: [i32; 3],
    ) -> Result<(), Error> {
        self.place_netherrack(context, tags, p)?;
        for _ in 0..8 {
            if context.random.next_float() >= 0.5 {
                break;
            }
            p[1] -= 1;
            self.place_netherrack(context, tags, p)?;
        }
        Ok(())
    }
    fn maybe_leaves(&self, context: &mut FeatureContext<'_, '_>, p: [i32; 3]) -> Result<(), Error> {
        let above = [p[0], p[1] + 1, p[2]];
        if context.random.next_float() < 0.5
            && context.region.block(p)?.block() == Block::NETHERRACK
            && context.region.block(above)?.is_air()
        {
            context.region.set_block(
                above,
                Block::JUNGLE_LEAVES
                    .default_state()
                    .with(properties::PERSISTENT, true)
                    .expect("leaf persistence"),
                3,
            )?;
        }
        Ok(())
    }
    fn maybe_vines(&self, context: &mut FeatureContext<'_, '_>, p: [i32; 3]) -> Result<(), Error> {
        let state = context.region.block(p)?;
        if state.is_air() || state.block() == Block::VINE {
            return Ok(());
        }
        let direction = Plane::Horizontal.faces()[context.random.next_int_bound(4) as usize];
        let step = direction.step();
        let step = [step.x, step.y, step.z];
        let neighbor = std::array::from_fn(|i| p[i] + step[i]);
        if context.region.block(neighbor)?.is_air()
            && context.region.is_collision_face_full(state, p, direction)?
        {
            let state = match direction.opposite() {
                Direction::North => Block::VINE
                    .default_state()
                    .with(properties::NORTH, properties::North::True),
                Direction::South => Block::VINE
                    .default_state()
                    .with(properties::SOUTH, properties::South::True),
                Direction::East => Block::VINE
                    .default_state()
                    .with(properties::EAST, properties::East::True),
                Direction::West => Block::VINE
                    .default_state()
                    .with(properties::WEST, properties::West::True),
                _ => unreachable!("horizontal vine direction"),
            }
            .expect("vine face");
            context.region.set_block(neighbor, state, 3)?;
        }
        Ok(())
    }
    fn spread_netherrack(
        &self,
        context: &mut FeatureContext<'_, '_>,
        tags: &BlockTags,
    ) -> Result<(), Error> {
        const PROBABILITIES: [f32; 14] = [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.9, 0.8, 0.7, 0.6, 0.4, 0.2,
        ];
        let b = self.template.data.bounds;
        let center = b.center();
        let width = (b.max[0] - b.min[0] + 1 + b.max[2] - b.min[2] + 1) / 2;
        let adjustment = context.random.next_int_bound((8 - width / 2).max(1));
        let follow = matches!(
            self.placement,
            VerticalPlacement::OnLandSurface | VerticalPlacement::OnOceanFloor
        );
        for x in (center[0] - 14)..=(center[0] + 14) {
            for z in (center[2] - 14)..=(center[2] + 14) {
                let distance =
                    ((x - center[0]).abs() + (z - center[2]).abs() + adjustment).max(0) as usize;
                if distance >= 14
                    || context.random.next_double() >= f64::from(PROBABILITIES[distance])
                {
                    continue;
                }
                let surface = context.region.height_at(self.placement.heightmap(), x, z)? - 1;
                let y = if follow {
                    surface
                } else {
                    b.min[1].min(surface)
                };
                let p = [x, y, z];
                if (y - b.min[1]).abs() <= 3 && self.replaceable(context.region.block(p)?, tags) {
                    self.place_netherrack(context, tags, p)?;
                    if self.properties.overgrown {
                        self.maybe_leaves(context, p)?;
                    }
                    self.drip_column(context, tags, [x, y - 1, z])?;
                }
            }
        }
        Ok(())
    }
}

fn random_within(random: &mut impl RandomSource, min: i32, max: i32) -> i32 {
    if min < max {
        min + random.next_int_bound(max - min + 1)
    } else {
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generator::flat::{FlatLevelGeneratorSettings, FlatLevelSource},
        structure::processor::tests::generator,
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use std::io::Read;
    #[test]
    fn portal_setups_projection_properties_processors_and_rng_match_vanilla() {
        let (resources, noise) = generator();
        let flat = FlatLevelSource::new(
            noise.data.clone(),
            FlatLevelGeneratorSettings::default_settings(),
        )
        .unwrap();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let temperatures = BiomeTemperatures::load(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/ruined_portal_pieces.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let id: Identifier = case["id"].as_str().unwrap().parse().unwrap();
            let resource = id
                .with_path(format!("worldgen/structure/{}.json", id.path()))
                .unwrap();
            let structure = RuinedPortalStructure::decode(
                &resources.require(&resource).unwrap().json().unwrap(),
            )
            .unwrap();
            let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
            let seed = case["seed"].as_i64().unwrap();
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
            random.set_large_feature_seed(seed, chunk[0], chunk[1]);
            let mut biome = |_p| Ok(case["biome"].as_str().unwrap().parse().unwrap());
            let piece = structure
                .generate(
                    &templates,
                    &temperatures,
                    &flat,
                    &mut random,
                    chunk,
                    -64,
                    384,
                    &mut biome,
                    &|_| true,
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                piece.template.position,
                std::array::from_fn(|i| case["position"][i].as_i64().unwrap() as i32),
                "{id} {seed}"
            );
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "{id} {seed}"
            );
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{id} {seed}"
            );
            let loaded = RuinedPortalPiece::from_nbt(&templates, &piece.to_nbt()).unwrap();
            assert_eq!(
                loaded.to_nbt(),
                ferraloom_nbt::parse_compound(case["loaded"].as_str().unwrap()).unwrap(),
                "{id} {seed}"
            );
            let expected = case["processors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| ferraloom_nbt::parse(v.as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            let actual = piece
                .processors()
                .iter()
                .map(StructureProcessor::to_nbt)
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{id} {seed}");
        }
    }
    #[test]
    fn portal_terrain_spread_drips_leaves_and_following_rng_match_vanilla() {
        use crate::{
            feature::FeatureRegistry,
            region::{ChunkStatus, RegionChunk, WorldGenRegion},
            structure::processor::tests::StaticBlocks,
        };
        let (resources, generator) = generator();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let tags = BlockTags::load(&resources).unwrap();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/ruined_portal_terrain.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let mode = case["mode"].as_i64().unwrap();
            let piece = RuinedPortalPiece::from_nbt(
                &templates,
                &ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            let mut chunks = Vec::new();
            for cz in -1..=1 {
                for cx in -1..=1 {
                    let mut chunk = generator.new_chunk([cx, cz]);
                    for z in 0..16 {
                        for x in 0..16 {
                            let wx = cx * 16 + x;
                            let wz = cz * 16 + z;
                            let ground = if mode == 0 {
                                63
                            } else {
                                60 + (wx * 3 + wz * 5).rem_euclid(8)
                            };
                            for y in -64..=ground {
                                let block = if mode == 2 && y == ground {
                                    match (wx * 7 + wz * 11).rem_euclid(5) {
                                        0 => Block::LAVA,
                                        1 => Block::WATER,
                                        2 => Block::OBSIDIAN,
                                        3 => Block::BEDROCK,
                                        _ => Block::NETHERRACK,
                                    }
                                } else {
                                    Block::STONE
                                };
                                chunk.set_block(x as u8, y, z as u8, block.default_state());
                            }
                        }
                    }
                    chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                }
            }
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain; 2],
                1,
                None,
                &StaticBlocks,
                seed,
                63,
            )
            .unwrap();
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(seed));
            let mut context = FeatureContext {
                region: &mut region,
                random: &mut random,
                origin: [0; 3],
                generator: &generator,
                registry: &features,
            };
            piece.spread_netherrack(&mut context, &tags).unwrap();
            let b = piece.template.data.bounds;
            for x in (b.min[0] + 1)..b.max[0] {
                for z in (b.min[2] + 1)..b.max[2] {
                    if context.region.block([x, b.min[1], z]).unwrap().block() == Block::NETHERRACK
                    {
                        piece
                            .drip_column(&mut context, &tags, [x, b.min[1] - 1, z])
                            .unwrap();
                    }
                }
            }
            if piece.properties.overgrown {
                for z in b.min[2]..=b.max[2] {
                    for y in b.min[1]..=b.max[1] {
                        for x in b.min[0]..=b.max[0] {
                            piece.maybe_leaves(&mut context, [x, y, z]).unwrap();
                        }
                    }
                }
            }
            assert_eq!(
                context.random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "mode={mode} seed={seed} {:?}",
                piece.properties
            );
            for cz in -1..=1 {
                for cx in -1..=1 {
                    let expected = case["writes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| {
                            let p: [i32; 3] =
                                std::array::from_fn(|i| v["pos"][i].as_i64().unwrap() as i32);
                            (p, v["state"].as_u64().unwrap() as u16)
                        })
                        .filter(|(p, _)| p[0] >> 4 == cx && p[2] >> 4 == cz)
                        .collect::<Vec<_>>();
                    let actual = context
                        .region
                        .chunk([cx, cz], ChunkStatus::Terrain)
                        .unwrap()
                        .poi_changes
                        .iter()
                        .map(|(p, _, s)| (*p, s.id()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        actual.len(),
                        expected.len(),
                        "mode={mode} seed={seed} chunk={cx},{cz} {:?}",
                        piece.properties
                    );
                    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                        assert_eq!(
                            a, e,
                            "mode={mode} seed={seed} chunk={cx},{cz} write={i} {:?}",
                            piece.properties
                        );
                    }
                }
            }
        }
    }
}
