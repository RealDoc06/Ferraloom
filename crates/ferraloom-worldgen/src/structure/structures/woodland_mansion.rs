//! Woodland mansion floor planning and template assembly.
pub mod grid;
mod placement;
use super::igloo::{read_rotation, rotation_name};
use crate::{
    Error,
    structure::StructurePlacementContext,
    structure::{
        BoundingBox,
        nbt::string,
        piece::StructurePieceData,
        processor::StructureProcessor,
        rule::BlockTags,
        template::{StructurePlaceSettings, StructureTemplateManager},
        template_piece::TemplateStructurePiece,
    },
};
use ferraloom_data::{Block, EntityType, properties};
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{Direction, Mirror, Rotation, random::RandomSource};
pub use placement::generate;

/// Port of WoodlandMansionPieces.WoodlandMansionPiece.
#[derive(Clone, Debug)]
pub struct WoodlandMansionPiece {
    pub template: TemplateStructurePiece,
}
impl WoodlandMansionPiece {
    pub fn new(
        templates: &StructureTemplateManager,
        name: &str,
        position: [i32; 3],
        rotation: Rotation,
        mirror: Mirror,
    ) -> Self {
        Self {
            template: TemplateStructurePiece::new(
                templates,
                name.into(),
                format!("minecraft:woodland_mansion/{name}")
                    .parse()
                    .expect("template id"),
                position,
                settings(rotation, mirror),
                0,
            ),
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:wmp".parse().expect("piece id"));
        tag.insert(
            "Rot",
            string(rotation_name(self.template.settings.rotation)),
        );
        tag.insert(
            "Mi",
            string(match self.template.settings.mirror {
                Mirror::None => "NONE",
                Mirror::LeftRight => "LEFT_RIGHT",
                Mirror::FrontBack => "FRONT_BACK",
            }),
        );
        tag
    }
    pub fn from_nbt(
        templates: &StructureTemplateManager,
        tag: &CompoundTag,
    ) -> Result<Self, Error> {
        let mirror = match tag.get_string("Mi").map(|s| s.to_string_lossy()).as_deref() {
            Some("NONE") => Mirror::None,
            Some("LEFT_RIGHT") => Mirror::LeftRight,
            Some("FRONT_BACK") => Mirror::FrontBack,
            _ => return Err(Error("invalid mansion mirror".into())),
        };
        let name = tag
            .get_string("Template")
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(
                templates,
                tag,
                format!("minecraft:woodland_mansion/{name}").parse()?,
                settings(read_rotation(tag)?, mirror),
            )?,
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        let rotation = self.template.settings.rotation;
        self.template.post_process(
            context,
            templates,
            tags,
            bounds,
            reference,
            &[StructureProcessor::BlockIgnore(vec![
                Block::STRUCTURE_BLOCK,
            ])],
            |marker, p, context| {
                if marker.starts_with("Chest") {
                    let mut state = Block::CHEST.default_state();
                    let facing = match marker {
                        "ChestWest" => Some(Direction::West),
                        "ChestEast" => Some(Direction::East),
                        "ChestSouth" => Some(Direction::South),
                        "ChestNorth" => Some(Direction::North),
                        _ => None,
                    };
                    if let Some(facing) = facing {
                        state = state
                            .with(
                                properties::FACING,
                                match rotation.rotate(facing) {
                                    Direction::West => properties::Facing::West,
                                    Direction::East => properties::Facing::East,
                                    Direction::South => properties::Facing::South,
                                    _ => properties::Facing::North,
                                },
                            )
                            .expect("chest facing");
                    }
                    StructurePieceData::create_chest_at(
                        context,
                        p,
                        bounds,
                        &"minecraft:chests/woodland_mansion".parse()?,
                        Some(state),
                    )?;
                } else if marker == "Group of Allays" {
                    let count = context.region.random_mut()?.next_int_bound(3) + 1;
                    context.region.spawn_mansion_allays(p, count)?;
                } else {
                    let entity = match marker {
                        "Mage" => EntityType::EVOKER,
                        "Warrior" => EntityType::VINDICATOR,
                        _ => return Ok(()),
                    };
                    if context.region.spawn_structure_entity(
                        entity,
                        [
                            f64::from(p[0]) + 0.5,
                            f64::from(p[1]),
                            f64::from(p[2]) + 0.5,
                        ],
                        Some(0.0),
                        crate::region::StructureEntitySpawnFlags {
                            persistent: true,
                            finalize_spawn: true,
                            with_passengers: true,
                        },
                    )? {
                        context.region.set_block(p, Block::AIR.default_state(), 2)?;
                    }
                }
                Ok(())
            },
        )
    }
}
fn settings(rotation: Rotation, mirror: Mirror) -> StructurePlaceSettings {
    StructurePlaceSettings {
        rotation,
        mirror,
        ignore_entities: true,
        ..Default::default()
    }
}
/// Port of WoodlandMansionStructure#afterPlace. The footprint uses the raw piece
/// bounds; support columns include liquids and stop strictly above the build floor.
pub fn after_place(
    context: &mut StructurePlacementContext<'_, '_>,
    pieces: &[crate::structure::start::StructurePiece],
    bounds: BoundingBox,
) -> Result<(), Error> {
    let Some(raw) = pieces
        .iter()
        .map(|p| p.bounds())
        .reduce(BoundingBox::encapsulating)
    else {
        return Ok(());
    };
    let min_y = context.region.min_y();
    let y = raw.min[1];
    for x in bounds.min[0]..=bounds.max[0] {
        for z in bounds.min[2]..=bounds.max[2] {
            let p = [x, y, z];
            if !context.region.block(p)?.is_air()
                && raw.contains(p)
                && pieces.iter().any(|piece| piece.bounds().contains(p))
            {
                for y in (min_y + 1..y).rev() {
                    let p = [x, y, z];
                    let state = context.region.block(p)?;
                    if !state.is_air() && !matches!(state.block(), Block::WATER | Block::LAVA) {
                        break;
                    }
                    context
                        .region
                        .set_block(p, Block::COBBLESTONE.default_state(), 2)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::LegacyRandomSource;
    use std::io::Read;
    #[test]
    fn mansion_template_room_wall_roof_order_bounds_and_rng_match_vanilla() {
        let (resources, _) = crate::structure::processor::tests::generator();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/mansion_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let rotation = Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize];
            let mut random = LegacyRandomSource::new(seed);
            let pieces = generate(&templates, [-17, 70, 31], rotation, &mut random);
            let expected = ferraloom_nbt::parse(case["nbt"].as_str().unwrap()).unwrap();
            let expected = expected.as_list().unwrap();
            assert_eq!(
                pieces.len(),
                expected.len(),
                "seed={seed} rotation={rotation:?}"
            );
            for (index, (piece, expected)) in pieces.iter().zip(expected.iter()).enumerate() {
                assert_eq!(
                    ferraloom_nbt::Tag::Compound(piece.to_nbt()),
                    *expected,
                    "seed={seed} rotation={rotation:?} piece={index}"
                );
                let loaded = WoodlandMansionPiece::from_nbt(&templates, &piece.to_nbt()).unwrap();
                assert_eq!(piece.to_nbt(), loaded.to_nbt());
            }
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "seed={seed} rotation={rotation:?}"
            );
        }
    }
}

#[cfg(test)]
mod foundation_tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::{
            processor::tests::{StaticBlocks, generator},
            start::StructurePiece,
            structures::buried_treasure::BuriedTreasurePiece,
        },
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use std::io::Read;
    fn initial(mode: i32, [x, y, z]: [i32; 3]) -> ferraloom_data::BlockState {
        if y == 50 {
            return if (x * 3 + z).rem_euclid(5) == 0 {
                Block::AIR
            } else {
                Block::DARK_OAK_PLANKS
            }
            .default_state();
        }
        if mode == 2 {
            return Block::AIR.default_state();
        }
        if mode == 3 && y == 44 {
            return Block::OAK_SLAB
                .default_state()
                .with(properties::WATERLOGGED, true)
                .unwrap();
        }
        let ground = 40 + (x + z * 3).rem_euclid(6);
        if y <= ground {
            Block::STONE
        } else if mode == 1 {
            Block::WATER
        } else {
            Block::AIR
        }
        .default_state()
    }
    #[test]
    fn mansion_raw_footprint_foundations_liquids_build_floor_and_clipping_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/mansion_foundations.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let mode = case["mode"].as_i64().unwrap() as i32;
            let mut chunk = generator.new_chunk([0, 0]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in -64..=61 {
                        chunk.set_block(x as u8, y, z as u8, initial(mode, [x, y, z]));
                    }
                }
            }
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
                0,
                63,
            )
            .unwrap();
            let pieces = case["pieces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    StructurePiece::BuriedTreasure(
                        BuriedTreasurePiece::from_nbt(
                            &ferraloom_nbt::parse_compound(s.as_str().unwrap()).unwrap(),
                        )
                        .unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                std::array::from_fn(|i| b[i].as_i64().unwrap() as i32),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            after_place(
                &mut StructurePlacementContext {
                    region: &mut region,
                    random: &mut random,
                    origin: [0; 3],
                    generator: &generator,
                    registry: &features,
                },
                &pieces,
                bounds,
            )
            .unwrap();
            let actual = region
                .chunk([0, 0], ChunkStatus::Terrain)
                .unwrap()
                .poi_changes
                .iter()
                .map(|(p, _, state)| (*p, state.id()))
                .collect::<Vec<_>>();
            let expected = case["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    (
                        std::array::from_fn(|i| w["pos"][i].as_i64().unwrap() as i32),
                        w["state"].as_u64().unwrap() as u16,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "mode={mode} bounds={bounds:?}");
            assert!(
                case["writes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v["flags"] == 2)
            );
        }
    }
}
