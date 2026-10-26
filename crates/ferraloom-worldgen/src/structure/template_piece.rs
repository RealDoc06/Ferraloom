//! TemplateStructurePiece base behavior, including marker and jigsaw passes.
use super::{
    BoundingBox,
    nbt::string,
    piece::StructurePieceData,
    processor::{StructureProcessor, parse_jigsaw_replacement},
    rule::BlockTags,
    template::{
        StructureBlockInfo, StructurePlaceSettings, StructureTemplate, StructureTemplateManager,
        transform_block,
    },
};
use crate::{Error, Identifier, structure::StructurePlacementContext};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::Direction;

/// Port of TemplateStructurePiece. Subclasses own their settings/processor
/// choices and marker behavior, with no second game adapter.
#[derive(Clone, Debug)]
pub struct TemplateStructurePiece {
    pub data: StructurePieceData,
    pub template_name: String,
    pub location: Identifier,
    pub position: [i32; 3],
    pub settings: StructurePlaceSettings,
}
impl TemplateStructurePiece {
    pub fn new(
        templates: &StructureTemplateManager,
        name: String,
        location: Identifier,
        position: [i32; 3],
        settings: StructurePlaceSettings,
        gen_depth: i32,
    ) -> Self {
        let bounds = templates
            .get_or_create(&location)
            .bounding_box(&settings, position);
        Self {
            data: StructurePieceData::new(bounds, gen_depth, Some(Direction::North)),
            template_name: name,
            location,
            position,
            settings,
        }
    }
    pub fn to_nbt(&self, id: &Identifier) -> CompoundTag {
        let mut tag = self.data.to_nbt(id);
        tag.insert("Template", string(&self.template_name));
        for (key, n) in [("TPX", 0), ("TPY", 1), ("TPZ", 2)] {
            tag.insert(key, Tag::Int(self.position[n]));
        }
        tag
    }
    pub fn from_nbt(
        templates: &StructureTemplateManager,
        tag: &CompoundTag,
        location: Identifier,
        settings: StructurePlaceSettings,
    ) -> Result<Self, Error> {
        let data = StructurePieceData::from_nbt(tag)?;
        Ok(Self::new(
            templates,
            tag.get_string("Template")
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            location,
            [
                tag.get_int("TPX").unwrap_or(0),
                tag.get_int("TPY").unwrap_or(0),
                tag.get_int("TPZ").unwrap_or(0),
            ],
            settings,
            data.gen_depth,
        ))
    }
    pub fn moved(&mut self, delta: [i32; 3]) {
        self.data.bounds = self.data.bounds.moved(delta);
        self.position = std::array::from_fn(|i| self.position[i].wrapping_add(delta[i]));
    }
    #[allow(clippy::too_many_arguments)]
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: BoundingBox,
        reference: [i32; 3],
        processors: &[StructureProcessor],
        mut handle_marker: impl FnMut(
            &str,
            [i32; 3],
            &mut StructurePlacementContext<'_, '_>,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let template = templates.get_or_create(&self.location);
        self.settings.bounding_box = Some(bounds);
        self.data.bounds = template.bounding_box(&self.settings, self.position);
        if !template.place_in_world(
            context.region,
            self.position,
            reference,
            &mut self.settings,
            processors,
            tags,
            context.random,
            2,
        )? {
            return Ok(());
        }
        for marker in filter_blocks(
            &template,
            self.position,
            &mut self.settings,
            Block::STRUCTURE_BLOCK,
            context.random,
        )? {
            if let Some(tag) = marker.nbt {
                let mode = tag
                    .get_string("mode")
                    .map(|s| s.to_string_lossy())
                    .ok_or_else(|| Error("missing template structure mode".into()))?;
                match mode.as_str() {
                    "DATA" => handle_marker(
                        &tag.get_string("metadata")
                            .map(|s| s.to_string_lossy())
                            .unwrap_or_default(),
                        marker.position,
                        context,
                    )?,
                    "SAVE" | "LOAD" | "CORNER" => {}
                    _ => return Err(Error(format!("unknown template structure mode {mode}"))),
                }
            }
        }
        for block in filter_blocks(
            &template,
            self.position,
            &mut self.settings,
            Block::JIGSAW,
            context.random,
        )? {
            if let Some(tag) = block.nbt {
                let state = tag
                    .get_string("final_state")
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_else(|| "minecraft:air".into());
                let state =
                    parse_jigsaw_replacement(&state).unwrap_or_else(|| Block::AIR.default_state());
                context.region.set_block(block.position, state, 3)?;
            }
        }
        Ok(())
    }
}
/// Port of StructureTemplate#filterBlocks(position, settings, block, true).
/// The state rotates without mirroring, as vanilla's marker pass does.
pub fn filter_blocks(
    template: &StructureTemplate,
    position: [i32; 3],
    settings: &mut StructurePlaceSettings,
    block: Block,
    random: &mut impl ferraloom_util::random::RandomSource,
) -> Result<Vec<StructureBlockInfo>, Error> {
    if template.palettes.is_empty() {
        return Ok(Vec::new());
    }
    let palette = template.random_palette(settings, position, random)?;
    Ok(palette
        .blocks
        .iter()
        .filter(|b| b.state.block() == block)
        .filter_map(|b| {
            let local = transform_block(
                b.position,
                settings.mirror,
                settings.rotation,
                settings.pivot,
            );
            let p = std::array::from_fn(|i| local[i].wrapping_add(position[i]));
            if settings
                .bounding_box
                .is_some_and(|bounds| !bounds.contains(p))
            {
                return None;
            }
            Some(StructureBlockInfo {
                position: p,
                state: b.state.rotate(settings.rotation),
                nbt: b.nbt.clone(),
            })
        })
        .collect())
}
