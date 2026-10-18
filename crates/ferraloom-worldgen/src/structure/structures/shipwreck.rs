use super::igloo::{read_rotation, rotation_name};
use crate::structure::{
    BoundingBox,
    nbt::string,
    processor::StructureProcessor,
    rule::BlockTags,
    template::{StructurePlaceSettings, StructureTemplateManager},
    template_piece::TemplateStructurePiece,
};
use crate::{
    Error, generator::ChunkGeneratorAccess, region::Heightmap, structure::StructurePlacementContext,
};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Rotation, random::RandomSource};

const BEACHED: &[&str] = &[
    "with_mast",
    "sideways_full",
    "sideways_fronthalf",
    "sideways_backhalf",
    "rightsideup_full",
    "rightsideup_fronthalf",
    "rightsideup_backhalf",
    "with_mast_degraded",
    "rightsideup_full_degraded",
    "rightsideup_fronthalf_degraded",
    "rightsideup_backhalf_degraded",
];
const OCEAN: &[&str] = &[
    "with_mast",
    "upsidedown_full",
    "upsidedown_fronthalf",
    "upsidedown_backhalf",
    "sideways_full",
    "sideways_fronthalf",
    "sideways_backhalf",
    "rightsideup_full",
    "rightsideup_fronthalf",
    "rightsideup_backhalf",
    "with_mast_degraded",
    "upsidedown_full_degraded",
    "upsidedown_fronthalf_degraded",
    "upsidedown_backhalf_degraded",
    "sideways_full_degraded",
    "sideways_fronthalf_degraded",
    "sideways_backhalf_degraded",
    "rightsideup_full_degraded",
    "rightsideup_fronthalf_degraded",
    "rightsideup_backhalf_degraded",
];
fn settings(rotation: Rotation) -> StructurePlaceSettings {
    StructurePlaceSettings {
        rotation,
        pivot: [4, 0, 15],
        ..Default::default()
    }
}
/// Port of ShipwreckPieces.ShipwreckPiece, including persisted height adjustment.
#[derive(Clone, Debug)]
pub struct ShipwreckPiece {
    pub template: TemplateStructurePiece,
    pub beached: bool,
    pub height_adjusted: bool,
}
impl ShipwreckPiece {
    pub fn add_random(
        templates: &StructureTemplateManager,
        position: [i32; 3],
        rotation: Rotation,
        random: &mut impl RandomSource,
        beached: bool,
    ) -> Self {
        let names = if beached { BEACHED } else { OCEAN };
        let location = format!(
            "minecraft:shipwreck/{}",
            names[random.next_int_bound(names.len() as i32) as usize]
        );
        Self {
            template: TemplateStructurePiece::new(
                templates,
                location.clone(),
                location.parse().expect("builtin template"),
                position,
                settings(rotation),
                0,
            ),
            beached,
            height_adjusted: false,
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:shipwreck".parse().expect("builtin piece id"));
        tag.insert(
            "Rot",
            string(rotation_name(self.template.settings.rotation)),
        );
        tag.insert("isBeached", Tag::Byte(i8::from(self.beached)));
        tag.insert("height_adjusted", Tag::Byte(i8::from(self.height_adjusted)));
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
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(
                templates,
                tag,
                name.parse()?,
                settings(read_rotation(tag)?),
            )?,
            beached: tag.get_byte("isBeached").unwrap_or(0) != 0,
            height_adjusted: tag.get_byte("height_adjusted").unwrap_or(0) != 0,
        })
    }
    pub fn is_too_big(&self, templates: &StructureTemplateManager) -> bool {
        let size = templates.get_or_create(&self.template.location).size;
        // Vanilla checks X and Y here, despite the worldgen region being X/Z.
        size[0] > 32 || size[1] > 32
    }
    pub fn beached_position(
        &self,
        templates: &StructureTemplateManager,
        min_y: i32,
        random: &mut impl RandomSource,
    ) -> i32 {
        min_y
            - templates.get_or_create(&self.template.location).size[1] / 2
            - random.next_int_bound(3)
    }
    pub fn adjust_height(&mut self, height: i32) {
        self.height_adjusted = true;
        self.template.position[1] = height;
    }
    /// ShipwreckStructure adjusts oversized pieces before biome decoration.
    pub fn adjust_generation_height(
        &mut self,
        templates: &StructureTemplateManager,
        generator: &dyn ChunkGeneratorAccess,
        min_y: i32,
        height: i32,
        random: &mut impl RandomSource,
    ) -> Result<(), Error> {
        if !self.is_too_big(templates) {
            return Ok(());
        }
        let b = self.template.data.bounds;
        let sx = b.max[0] - b.min[0] + 1;
        let sz = b.max[2] - b.min[2] + 1;
        // ShipwreckStructure passes (minX, spanX, minZ, spanZ) to both
        // helpers. getLowestY's parameter order differs from getMean...:
        // vanilla's beached branch therefore uses spanX as the minimum Z.
        let (min_z, size_x) = if self.beached {
            (sx, b.min[2])
        } else {
            (b.min[2], sx)
        };
        let mut heights = [0; 4];
        for (i, [x, z]) in [
            [b.min[0], min_z],
            [b.min[0], min_z + sz],
            [b.min[0] + size_x, min_z],
            [b.min[0] + size_x, min_z + sz],
        ]
        .into_iter()
        .enumerate()
        {
            heights[i] =
                generator.get_base_height(x, z, Heightmap::WorldSurfaceWg, min_y, height)? - 1;
        }
        let y = if self.beached {
            self.beached_position(templates, *heights.iter().min().unwrap(), random)
        } else {
            heights.iter().copied().fold(0i32, i32::wrapping_add) / 4
        };
        self.adjust_height(y);
        Ok(())
    }
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        if !self.height_adjusted && !self.is_too_big(templates) {
            let mut min_y = context.region.min_y() + context.region.height();
            let mut mean = 0i32;
            let size = templates.get_or_create(&self.template.location).size;
            let map = if self.beached {
                Heightmap::WorldSurfaceWg
            } else {
                Heightmap::OceanFloorWg
            };
            let base = size[0] * size[2];
            if base == 0 {
                mean = context.region.height_at(
                    map,
                    self.template.position[0],
                    self.template.position[2],
                )?;
            } else {
                for z in self.template.position[2]..self.template.position[2] + size[2] {
                    for x in self.template.position[0]..self.template.position[0] + size[0] {
                        let y = context.region.height_at(map, x, z)?;
                        mean = mean.wrapping_add(y);
                        min_y = min_y.min(y);
                    }
                }
                mean /= base;
            }
            let y = if self.beached {
                self.beached_position(templates, min_y, context.random)
            } else {
                mean
            };
            self.adjust_height(y);
        }
        self.template.post_process(
            context,
            templates,
            tags,
            bounds,
            reference,
            &[StructureProcessor::BlockIgnore(vec![
                Block::STRUCTURE_BLOCK,
                Block::AIR,
            ])],
            |marker, p, context| {
                let loot = match marker {
                    "map_chest" => "minecraft:chests/shipwreck_map",
                    "treasure_chest" => "minecraft:chests/shipwreck_treasure",
                    "supply_chest" => "minecraft:chests/shipwreck_supply",
                    _ => return Ok(()),
                };
                context.region.set_loot_table(
                    [p[0], p[1] - 1, p[2]],
                    &loot.parse()?,
                    context.random,
                )
            },
        )
    }
}
