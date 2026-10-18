use crate::structure::{
    BoundingBox,
    nbt::string,
    processor::StructureProcessor,
    rule::BlockTags,
    template::{LiquidSettings, StructurePlaceSettings, StructureTemplateManager, transform_block},
    template_piece::TemplateStructurePiece,
};
use crate::{Error, region::Heightmap, structure::StructurePlacementContext};
use ferraloom_data::Block;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{Mirror, Rotation, random::RandomSource};

/// Port of IglooPieces.IglooPiece.
#[derive(Clone, Debug)]
pub struct IglooPiece {
    pub template: TemplateStructurePiece,
}
fn settings(rotation: Rotation, location: &str) -> Result<StructurePlaceSettings, Error> {
    let pivot = match location {
        "minecraft:igloo/top" => [3, 5, 5],
        "minecraft:igloo/middle" => [1, 3, 1],
        "minecraft:igloo/bottom" => [3, 6, 7],
        _ => return Err(Error(format!("unknown igloo template {location}"))),
    };
    Ok(StructurePlaceSettings {
        rotation,
        pivot,
        liquid_settings: LiquidSettings::IgnoreWaterlogging,
        ..Default::default()
    })
}
fn offset(location: &str) -> Result<[i32; 3], Error> {
    match location {
        "minecraft:igloo/top" => Ok([0, 0, 0]),
        "minecraft:igloo/middle" => Ok([2, -3, 4]),
        "minecraft:igloo/bottom" => Ok([0, -3, -2]),
        _ => Err(Error(format!("unknown igloo template {location}"))),
    }
}
impl IglooPiece {
    pub fn new(
        templates: &StructureTemplateManager,
        name: &str,
        position: [i32; 3],
        rotation: Rotation,
        depth: i32,
    ) -> Result<Self, Error> {
        let delta = offset(name)?;
        let mut p = std::array::from_fn(|i| position[i].wrapping_add(delta[i]));
        p[1] = p[1].wrapping_sub(depth);
        Ok(Self {
            template: TemplateStructurePiece::new(
                templates,
                name.into(),
                name.parse()?,
                p,
                settings(rotation, name)?,
                0,
            ),
        })
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:iglu".parse().expect("builtin piece id"));
        tag.insert(
            "Rot",
            string(rotation_name(self.template.settings.rotation)),
        );
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
        let rotation = read_rotation(tag)?;
        Ok(Self {
            template: TemplateStructurePiece::from_nbt(
                templates,
                tag,
                name.parse()?,
                settings(rotation, &name)?,
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
        let settings = settings(
            self.template.settings.rotation,
            &self.template.location.to_string(),
        )?;
        let offset = offset(&self.template.location.to_string())?;
        let entrance = transform_block(
            [3 - offset[0], 0, -offset[2]],
            Mirror::None,
            settings.rotation,
            settings.pivot,
        );
        let entrance: [i32; 3] =
            std::array::from_fn(|i| entrance[i].wrapping_add(self.template.position[i]));
        let height =
            context
                .region
                .height_at(Heightmap::WorldSurfaceWg, entrance[0], entrance[2])?;
        let old = self.template.position;
        self.template.position[1] = self.template.position[1].wrapping_add(height - 91);
        let result = self.template.post_process(
            context,
            templates,
            tags,
            bounds,
            reference,
            &[StructureProcessor::BlockIgnore(vec![
                Block::STRUCTURE_BLOCK,
            ])],
            |marker, p, context| {
                if marker == "chest" {
                    context.region.set_block(p, Block::AIR.default_state(), 3)?;
                    let below = [p[0], p[1] - 1, p[2]];
                    if context.region.block(below)?.block() == Block::CHEST {
                        context.region.set_loot_table(
                            below,
                            &"minecraft:chests/igloo_chest".parse()?,
                            context.random,
                        )?;
                    }
                }
                Ok(())
            },
        );
        if result.is_ok() && self.template.location.to_string() == "minecraft:igloo/top" {
            let trap = transform_block([3, 0, 5], Mirror::None, settings.rotation, settings.pivot);
            let trap = std::array::from_fn(|i| trap[i] + self.template.position[i]);
            let below = context.region.block([trap[0], trap[1] - 1, trap[2]])?;
            if !below.is_air() && below.block() != Block::LADDER {
                context
                    .region
                    .set_block(trap, Block::SNOW_BLOCK.default_state(), 3)?;
            }
        }
        self.template.position = old;
        result
    }
}
/// Port of IglooPieces#addPieces; the caller has already drawn the rotation.
pub fn add_pieces(
    templates: &StructureTemplateManager,
    position: [i32; 3],
    rotation: Rotation,
    random: &mut impl RandomSource,
) -> Result<Vec<IglooPiece>, Error> {
    let mut pieces = Vec::new();
    if random.next_double() < 0.5 {
        let depth = random.next_int_bound(8) + 4;
        pieces.push(IglooPiece::new(
            templates,
            "minecraft:igloo/bottom",
            position,
            rotation,
            depth * 3,
        )?);
        for i in 0..depth - 1 {
            pieces.push(IglooPiece::new(
                templates,
                "minecraft:igloo/middle",
                position,
                rotation,
                i * 3,
            )?);
        }
    }
    pieces.push(IglooPiece::new(
        templates,
        "minecraft:igloo/top",
        position,
        rotation,
        0,
    )?);
    Ok(pieces)
}
pub(super) fn rotation_name(rotation: Rotation) -> &'static str {
    match rotation {
        Rotation::None => "NONE",
        Rotation::Clockwise90 => "CLOCKWISE_90",
        Rotation::Clockwise180 => "CLOCKWISE_180",
        Rotation::CounterClockwise90 => "COUNTERCLOCKWISE_90",
    }
}
pub(super) fn read_rotation(tag: &CompoundTag) -> Result<Rotation, Error> {
    match tag
        .get_string("Rot")
        .map(|s| s.to_string_lossy())
        .as_deref()
    {
        Some("NONE") => Ok(Rotation::None),
        Some("CLOCKWISE_90") => Ok(Rotation::Clockwise90),
        Some("CLOCKWISE_180") => Ok(Rotation::Clockwise180),
        Some("COUNTERCLOCKWISE_90") => Ok(Rotation::CounterClockwise90),
        _ => Err(Error("invalid template piece rotation".into())),
    }
}
