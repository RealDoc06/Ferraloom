use super::igloo::{read_rotation, rotation_name};
use crate::structure::{
    BoundingBox,
    nbt::string,
    processor::StructureProcessor,
    rule::BlockTags,
    template::{StructurePlaceSettings, StructureTemplateManager},
    template_piece::TemplateStructurePiece,
};
use crate::{Error, structure::StructurePlacementContext};
use ferraloom_data::Block;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{
    Rotation,
    random::{LegacyRandomSource, RandomSource},
};

/// Port of NetherFossilPieces.NetherFossilPiece.
#[derive(Clone, Debug)]
pub struct NetherFossilPiece {
    pub template: TemplateStructurePiece,
}
impl NetherFossilPiece {
    pub fn add_random(
        templates: &StructureTemplateManager,
        position: [i32; 3],
        random: &mut impl RandomSource,
    ) -> Self {
        let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
        let location = format!(
            "minecraft:nether_fossils/fossil_{}",
            random.next_int_bound(14) + 1
        );
        Self {
            template: TemplateStructurePiece::new(
                templates,
                location.clone(),
                location.parse().expect("builtin template"),
                position,
                StructurePlaceSettings {
                    rotation,
                    ..Default::default()
                },
                0,
            ),
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .template
            .to_nbt(&"minecraft:nefos".parse().expect("builtin piece id"));
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
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        templates: &StructureTemplateManager,
        tags: &BlockTags,
        bounds: &mut BoundingBox,
        reference: [i32; 3],
    ) -> Result<(), Error> {
        let fossil = templates
            .get_or_create(&self.template.location)
            .bounding_box(&self.template.settings, self.template.position);
        *bounds = bounds.encapsulating(fossil);
        self.template.post_process(
            context,
            templates,
            tags,
            *bounds,
            reference,
            &[StructureProcessor::BlockIgnore(vec![
                Block::STRUCTURE_BLOCK,
                Block::AIR,
            ])],
            |_, _, _| Ok(()),
        )?;
        // RandomSource#createThreadLocalInstance(seed) uses the legacy stream.
        let [x, y, z] = fossil.center();
        let mut random = LegacyRandomSource::new(context.region.seed)
            .fork_positional()
            .at(x, y, z);
        if random.next_float() < 0.5 {
            let x = fossil.min[0] + random.next_int_bound(fossil.max[0] - fossil.min[0] + 1);
            let z = fossil.min[2] + random.next_int_bound(fossil.max[2] - fossil.min[2] + 1);
            let p = [x, fossil.min[1], z];
            if context.region.block(p)?.is_air() && bounds.contains(p) {
                let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
                context.region.set_block(
                    p,
                    Block::DRIED_GHAST.default_state().rotate(rotation),
                    2,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::processor::tests::generator;
    use serde_json::Value;
    use std::io::Read;
    #[test]
    fn nether_fossil_templates_rotation_bounds_and_rng_match_vanilla() {
        let (resources, _) = generator();
        let templates = StructureTemplateManager::load_all(&resources).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/nether_fossil_pieces.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let position = std::array::from_fn(|i| case["position"][i].as_i64().unwrap() as i32);
            let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
            let piece = NetherFossilPiece::add_random(&templates, position, &mut random);
            let expected = ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap();
            assert_eq!(piece.to_nbt(), expected, "{case}");
            assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
            assert_eq!(
                NetherFossilPiece::from_nbt(&templates, &expected)
                    .unwrap()
                    .to_nbt(),
                expected
            );
        }
    }
}
