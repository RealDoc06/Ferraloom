use super::igloo::{read_rotation, rotation_name};
use crate::structure::{
    BoundingBox,
    nbt::string,
    processor::StructureProcessor,
    rule::BlockTags,
    template::{StructurePlaceSettings, StructureTemplateManager, transform_block},
    template_piece::TemplateStructurePiece,
};
use crate::{Error, feature::FeatureContext};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Direction, Rotation, random::RandomSource};

/// Port of EndCityPieces.EndCityPiece.
#[derive(Clone, Debug)]
pub struct EndCityPiece {
    pub template: TemplateStructurePiece,
    pub overwrite: bool,
}
impl EndCityPiece {
    pub fn new(
        templates: &StructureTemplateManager,
        name: &str,
        position: [i32; 3],
        rotation: Rotation,
        overwrite: bool,
    ) -> Self {
        Self {
            template: TemplateStructurePiece::new(
                templates,
                name.into(),
                format!("minecraft:end_city/{name}")
                    .parse()
                    .expect("template id"),
                position,
                StructurePlaceSettings {
                    rotation,
                    ignore_entities: true,
                    ..Default::default()
                },
                0,
            ),
            overwrite,
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:ecp".parse().expect("builtin piece id"));
        tag.insert(
            "Rot",
            string(rotation_name(self.template.settings.rotation)),
        );
        tag.insert("OW", Tag::Byte(i8::from(self.overwrite)));
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
        let overwrite = tag.get_byte("OW").unwrap_or(0) != 0;
        let settings = StructurePlaceSettings {
            rotation: read_rotation(tag)?,
            ignore_entities: true,
            ..Default::default()
        };
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(
                templates,
                tag,
                format!("minecraft:end_city/{name}").parse()?,
                settings,
            )?,
            overwrite,
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        let ignore = if self.overwrite {
            vec![Block::STRUCTURE_BLOCK]
        } else {
            vec![Block::STRUCTURE_BLOCK, Block::AIR]
        };
        let rotation = self.template.settings.rotation;
        self.template.post_process(
            context,
            templates,
            tags,
            bounds,
            reference,
            &[StructureProcessor::BlockIgnore(ignore)],
            |marker, p, context| {
                if marker.starts_with("Chest") {
                    let chest = [p[0], p[1] - 1, p[2]];
                    if bounds.contains(chest) {
                        context.region.set_loot_table(
                            chest,
                            &"minecraft:chests/end_city_treasure".parse()?,
                            context.random,
                        )?;
                    }
                } else if bounds.contains(p)
                    && p[0] >= -30000000
                    && p[2] >= -30000000
                    && p[0] < 30000000
                    && p[2] < 30000000
                    && p[1] >= -20000000
                    && p[1] < 20000000
                {
                    if marker.starts_with("Sentry") {
                        context.region.spawn_structure_entity(
                            ferraloom_data::EntityType::SHULKER,
                            [
                                f64::from(p[0]) + 0.5,
                                f64::from(p[1]),
                                f64::from(p[2]) + 0.5,
                            ],
                            None,
                            crate::region::StructureEntitySpawnFlags::default(),
                        )?;
                    } else if marker.starts_with("Elytra") {
                        context
                            .region
                            .spawn_end_city_elytra_frame(p, rotation.rotate(Direction::South))?;
                    }
                }
                Ok(())
            },
        )
    }
}
#[derive(Clone, Copy)]
enum Section {
    House,
    Tower,
    Bridge,
    FatTower,
}
const TOWER_BRIDGES: [(Rotation, [i32; 3]); 4] = [
    (Rotation::None, [1, -1, 0]),
    (Rotation::Clockwise90, [6, -1, 1]),
    (Rotation::CounterClockwise90, [0, -1, 5]),
    (Rotation::Clockwise180, [5, -1, 6]),
];
const FAT_BRIDGES: [(Rotation, [i32; 3]); 4] = [
    (Rotation::None, [4, -1, 0]),
    (Rotation::Clockwise90, [12, -1, 4]),
    (Rotation::CounterClockwise90, [0, -1, 8]),
    (Rotation::Clockwise180, [8, -1, 12]),
];
/// EndCityPieces' section-generator state is local to a single start. In
/// particular shipCreated survives rejected branches, but never leaks to a job.
struct Assembler<'a, R> {
    templates: &'a StructureTemplateManager,
    random: &'a mut R,
    ship_created: bool,
}
impl<R: RandomSource> Assembler<'_, R> {
    fn add(
        &self,
        pieces: &mut Vec<EndCityPiece>,
        parent: &EndCityPiece,
        offset: [i32; 3],
        name: &str,
        rotation: Rotation,
        overwrite: bool,
    ) -> EndCityPiece {
        let mut child = EndCityPiece::new(
            self.templates,
            name,
            parent.template.position,
            rotation,
            overwrite,
        );
        let delta = transform_block(
            offset,
            parent.template.settings.mirror,
            parent.template.settings.rotation,
            parent.template.settings.pivot,
        );
        let origin = transform_block(
            [0; 3],
            child.template.settings.mirror,
            child.template.settings.rotation,
            child.template.settings.pivot,
        );
        child
            .template
            .moved(std::array::from_fn(|i| delta[i] - origin[i]));
        pieces.push(child.clone());
        child
    }
    fn recurse(
        &mut self,
        section: Section,
        depth: i32,
        parent: &EndCityPiece,
        offset: Option<[i32; 3]>,
        pieces: &mut Vec<EndCityPiece>,
    ) -> bool {
        if depth > 8 {
            return false;
        }
        let mut children = Vec::new();
        if !self.generate(section, depth, parent, offset, &mut children) {
            return false;
        }
        let tag = self.random.next_int();
        for child in &mut children {
            child.template.data.gen_depth = tag;
            if pieces
                .iter()
                .find(|piece| {
                    piece
                        .template
                        .data
                        .bounds
                        .intersects(child.template.data.bounds)
                })
                .is_some_and(|piece| {
                    piece.template.data.gen_depth != parent.template.data.gen_depth
                })
            {
                return false;
            }
        }
        pieces.extend(children);
        true
    }
    fn generate(
        &mut self,
        section: Section,
        depth: i32,
        parent: &EndCityPiece,
        offset: Option<[i32; 3]>,
        pieces: &mut Vec<EndCityPiece>,
    ) -> bool {
        let rotation = parent.template.settings.rotation;
        match section {
            Section::House => {
                if depth > 8 {
                    return false;
                }
                let mut last = self.add(
                    pieces,
                    parent,
                    offset.expect("house attachment offset"),
                    "base_floor",
                    rotation,
                    true,
                );
                match self.random.next_int_bound(3) {
                    0 => {
                        self.add(pieces, &last, [-1, 4, -1], "base_roof", rotation, true);
                    }
                    floors => {
                        last = self.add(
                            pieces,
                            &last,
                            [-1, 0, -1],
                            "second_floor_2",
                            rotation,
                            false,
                        );
                        if floors == 1 {
                            last = self.add(
                                pieces,
                                &last,
                                [-1, 8, -1],
                                "second_roof",
                                rotation,
                                false,
                            );
                        } else {
                            last = self.add(
                                pieces,
                                &last,
                                [-1, 4, -1],
                                "third_floor_2",
                                rotation,
                                false,
                            );
                            last =
                                self.add(pieces, &last, [-1, 8, -1], "third_roof", rotation, true);
                        }
                        self.recurse(Section::Tower, depth + 1, &last, None, pieces);
                    }
                }
                true
            }
            Section::Tower => {
                let offset = [
                    3 + self.random.next_int_bound(2),
                    -3,
                    3 + self.random.next_int_bound(2),
                ];
                let mut last = self.add(pieces, parent, offset, "tower_base", rotation, true);
                last = self.add(pieces, &last, [0, 7, 0], "tower_piece", rotation, true);
                let mut bridge = if self.random.next_int_bound(3) == 0 {
                    Some(last.clone())
                } else {
                    None
                };
                let height = 1 + self.random.next_int_bound(3);
                for i in 0..height {
                    last = self.add(pieces, &last, [0, 4, 0], "tower_piece", rotation, true);
                    if i < height - 1 && self.random.next_boolean() {
                        bridge = Some(last.clone());
                    }
                }
                if let Some(bridge) = bridge {
                    for (turn, offset) in TOWER_BRIDGES {
                        if self.random.next_boolean() {
                            let start = self.add(
                                pieces,
                                &bridge,
                                offset,
                                "bridge_end",
                                rotation.rotated(turn),
                                true,
                            );
                            self.recurse(Section::Bridge, depth + 1, &start, None, pieces);
                        }
                    }
                    self.add(pieces, &last, [-1, 4, -1], "tower_top", rotation, true);
                } else {
                    if depth != 7 {
                        return self.recurse(Section::FatTower, depth + 1, &last, None, pieces);
                    }
                    self.add(pieces, &last, [-1, 4, -1], "tower_top", rotation, true);
                }
                true
            }
            Section::Bridge => {
                let length = self.random.next_int_bound(4) + 1;
                let mut last = self.add(pieces, parent, [0, 0, -4], "bridge_piece", rotation, true);
                last.template.data.gen_depth = -1;
                pieces.last_mut().unwrap().template.data.gen_depth = -1;
                let mut next_y = 0;
                for _ in 0..length {
                    if self.random.next_boolean() {
                        last = self.add(
                            pieces,
                            &last,
                            [0, next_y, -4],
                            "bridge_piece",
                            rotation,
                            true,
                        );
                        next_y = 0;
                    } else {
                        if self.random.next_boolean() {
                            last = self.add(
                                pieces,
                                &last,
                                [0, next_y, -4],
                                "bridge_steep_stairs",
                                rotation,
                                true,
                            );
                        } else {
                            last = self.add(
                                pieces,
                                &last,
                                [0, next_y, -8],
                                "bridge_gentle_stairs",
                                rotation,
                                true,
                            );
                        }
                        next_y = 4;
                    }
                }
                if !self.ship_created && self.random.next_int_bound(10 - depth) == 0 {
                    let offset = [
                        -8 + self.random.next_int_bound(8),
                        next_y,
                        -70 + self.random.next_int_bound(10),
                    ];
                    self.add(pieces, &last, offset, "ship", rotation, true);
                    self.ship_created = true;
                } else if !self.recurse(
                    Section::House,
                    depth + 1,
                    &last,
                    Some([-3, next_y + 1, -11]),
                    pieces,
                ) {
                    return false;
                }
                last = self.add(
                    pieces,
                    &last,
                    [4, next_y, 0],
                    "bridge_end",
                    rotation.rotated(Rotation::Clockwise180),
                    true,
                );
                last.template.data.gen_depth = -1;
                pieces.last_mut().unwrap().template.data.gen_depth = -1;
                true
            }
            Section::FatTower => {
                let mut last = self.add(
                    pieces,
                    parent,
                    [-3, 4, -3],
                    "fat_tower_base",
                    rotation,
                    true,
                );
                last = self.add(pieces, &last, [0, 4, 0], "fat_tower_middle", rotation, true);
                for _ in 0..2 {
                    if self.random.next_int_bound(3) == 0 {
                        break;
                    }
                    last = self.add(pieces, &last, [0, 8, 0], "fat_tower_middle", rotation, true);
                    for (turn, offset) in FAT_BRIDGES {
                        if self.random.next_boolean() {
                            let start = self.add(
                                pieces,
                                &last,
                                offset,
                                "bridge_end",
                                rotation.rotated(turn),
                                true,
                            );
                            self.recurse(Section::Bridge, depth + 1, &start, None, pieces);
                        }
                    }
                }
                self.add(pieces, &last, [-2, 8, -2], "fat_tower_top", rotation, true);
                true
            }
        }
    }
}
/// Port of EndCityPieces#startHouseTower and recursiveChildren's collision rules.
pub fn start_house_tower(
    templates: &StructureTemplateManager,
    position: [i32; 3],
    rotation: Rotation,
    random: &mut impl RandomSource,
) -> Vec<EndCityPiece> {
    let mut assembler = Assembler {
        templates,
        random,
        ship_created: false,
    };
    let mut last = EndCityPiece::new(templates, "base_floor", position, rotation, true);
    let mut pieces = vec![last.clone()];
    last = assembler.add(
        &mut pieces,
        &last,
        [-1, 0, -1],
        "second_floor_1",
        rotation,
        false,
    );
    last = assembler.add(
        &mut pieces,
        &last,
        [-1, 4, -1],
        "third_floor_1",
        rotation,
        false,
    );
    last = assembler.add(
        &mut pieces,
        &last,
        [-1, 8, -1],
        "third_roof",
        rotation,
        true,
    );
    assembler.recurse(Section::Tower, 1, &last, None, &mut pieces);
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::processor::tests::generator;
    use ferraloom_util::random::LegacyRandomSource;
    use serde_json::Value;
    use std::io::Read;
    #[test]
    fn end_city_recursive_sections_collision_tags_ship_state_and_rng_match_vanilla() {
        let (resources, _) = generator();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/end_city_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let position = std::array::from_fn(|i| case["position"][i].as_i64().unwrap() as i32);
            let rotation = Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize];
            let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
            let pieces = start_house_tower(&templates, position, rotation, &mut random);
            let expected = ferraloom_nbt::parse(case["nbt"].as_str().unwrap()).unwrap();
            let actual =
                crate::structure::nbt::sequence(pieces.iter().map(|p| Tag::Compound(p.to_nbt())));
            assert_eq!(
                actual, expected,
                "seed={} position={position:?} rotation={rotation:?}",
                case["seed"]
            );
            assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
            for piece in pieces {
                let saved = piece.to_nbt();
                assert_eq!(
                    EndCityPiece::from_nbt(&templates, &saved).unwrap().to_nbt(),
                    saved
                );
            }
        }
    }
}
