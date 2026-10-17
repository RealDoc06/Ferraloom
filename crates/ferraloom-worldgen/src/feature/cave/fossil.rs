use super::*;
use crate::{
    Identifier,
    region::Heightmap,
    structure::{
        BoundingBox,
        processor::StructureProcessor,
        template::{PlacementRandom, StructurePlaceSettings, StructureTemplateManager},
    },
};
use ferraloom_util::{Mirror, Rotation};

/// Port of FossilFeature. Templates/processors are decoded once; placement
/// shares the feature RNG with both template passes, as StructurePlaceSettings
/// does when setRandom is supplied by vanilla.
#[derive(Debug)]
struct FossilFeature {
    base: Vec<Identifier>,
    overlay: Vec<Identifier>,
    base_processors: Vec<StructureProcessor>,
    overlay_processors: Vec<StructureProcessor>,
    templates: StructureTemplateManager,
    tags: BlockTags,
    max_empty: i32,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("fossil".parse()?, |v, l| {
        let ids = |key| {
            list(field(v, key)?)?
                .iter()
                .map(id)
                .collect::<Result<Vec<_>, Error>>()
        };
        let base = ids("fossil_structures")?;
        let overlay = ids("overlay_structures")?;
        if base.is_empty() || base.len() != overlay.len() {
            return Err(Error(
                "Fossil structure lists must be nonempty and equal lengths".into(),
            ));
        }
        let processors = |key| {
            let value = field(v, key)?;
            if value.is_string() {
                let name = id(value)?;
                let value = l
                    .data
                    .resources
                    .require(
                        &name.with_path(format!("worldgen/processor_list/{}.json", name.path()))?,
                    )?
                    .json()?;
                StructureProcessor::decode_list(&value)
            } else {
                StructureProcessor::decode_list(value)
            }
        };
        let base_processors = processors("fossil_processors")?;
        let overlay_processors = processors("overlay_processors")?;
        let templates =
            StructureTemplateManager::load(l.data.resources, base.iter().chain(&overlay).cloned())?;
        Ok(Arc::new(FossilFeature {
            base,
            overlay,
            base_processors,
            overlay_processors,
            templates,
            tags: BlockTags(l.data.block_tags.clone()),
            max_empty: int_range(field(v, "max_empty_corners_allowed")?, 0, 7)?,
        }))
    })
}
impl Feature for FossilFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let rotation = [
            Rotation::None,
            Rotation::Clockwise90,
            Rotation::Clockwise180,
            Rotation::CounterClockwise90,
        ][ctx.random.next_int_bound(4) as usize];
        let index = ctx.random.next_int_bound(self.base.len() as i32) as usize;
        let base = self.templates.get(&self.base[index])?;
        let overlay = self.templates.get(&self.overlay[index])?;
        let min = [
            (ctx.origin[0] >> 4) * 16 - 16,
            ctx.region.min_y(),
            (ctx.origin[2] >> 4) * 16 - 16,
        ];
        let max = [
            (ctx.origin[0] >> 4) * 16 + 31,
            ctx.region.min_y() + ctx.region.height() - 1,
            (ctx.origin[2] >> 4) * 16 + 31,
        ];
        let mut settings = StructurePlaceSettings {
            rotation,
            bounding_box: Some(BoundingBox::new(min, max)),
            random: PlacementRandom::Shared,
            ..StructurePlaceSettings::default()
        };
        let size = base.rotated_size(rotation);
        let corner = offset(ctx.origin, [-size[0] / 2, 0, -size[2] / 2]);
        let mut lowest = ctx.origin[1];
        for x in 0..size[0] {
            for z in 0..size[2] {
                lowest = lowest.min(ctx.region.height_at(
                    Heightmap::OceanFloorWg,
                    corner[0] + x,
                    corner[2] + z,
                )?);
            }
        }
        let y = (lowest - 15 - ctx.random.next_int_bound(10)).max(ctx.region.min_y() + 10);
        let position =
            base.zero_position_with_transform([corner[0], y, corner[2]], Mirror::None, rotation);
        let bounds = base.bounding_box(&settings, position);
        let mut empty = 0;
        for x in [bounds.min[0], bounds.max[0]] {
            for y in [bounds.min[1], bounds.max[1]] {
                for z in [bounds.min[2], bounds.max[2]] {
                    if empty_or_liquid(ctx.region.block([x, y, z])?) {
                        empty += 1;
                    }
                }
            }
        }
        if empty > self.max_empty {
            return Ok(false);
        }
        base.place_in_world(
            ctx.region,
            position,
            position,
            &mut settings,
            &self.base_processors,
            &self.tags,
            ctx.random,
            260,
        )?;
        overlay.place_in_world(
            ctx.region,
            position,
            position,
            &mut settings,
            &self.overlay_processors,
            &self.tags,
            ctx.random,
            260,
        )?;
        Ok(true)
    }
}
