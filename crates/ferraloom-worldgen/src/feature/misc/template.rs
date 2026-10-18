//! Configured TemplateFeature. Templates and processor configs load once, while
//! all placement draws use the caller's single WorldgenRandom stream.
use super::super::{Feature, FeatureContext, FeatureLoader};
use crate::{
    Error,
    codec::*,
    placement::predicate::offset,
    structure::{
        processor::StructureProcessor,
        rule::BlockTags,
        template::{PlacementRandom, StructurePlaceSettings, StructureTemplate, transform_block},
    },
};
use ferraloom_util::{Mirror, Rotation, random::RandomSource};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug)]
struct TemplateEntry {
    // Vanilla resolves only the selected template, after the rotation draw.
    // Retain a conversion error until then, even for unused zero-weight entries.
    template: Result<Arc<StructureTemplate>, String>,
    rotations: Vec<Rotation>,
    weight: i32,
}
/// Port of TemplateFeature and TemplateFeature.TemplateEntry.
#[derive(Debug)]
struct TemplateFeature {
    templates: Vec<TemplateEntry>,
    total: i32,
    processors: Vec<StructureProcessor>,
    tags: BlockTags,
}
fn rotation(v: &Value) -> Result<Rotation, Error> {
    match string(v)? {
        "none" => Ok(Rotation::None),
        "clockwise_90" => Ok(Rotation::Clockwise90),
        "180" => Ok(Rotation::Clockwise180),
        "counterclockwise_90" => Ok(Rotation::CounterClockwise90),
        _ => Err(Error("unknown template rotation".into())),
    }
}
pub(super) fn decode(v: &Value, loader: &mut FeatureLoader<'_>) -> Result<Arc<dyn Feature>, Error> {
    let mut total = 0i32;
    let templates = list(field(v, "templates")?)?
        .iter()
        .map(|entry| {
            let weight = int_range(field(entry, "weight")?, 0, i32::MAX)?;
            total = total
                .checked_add(weight)
                .ok_or_else(|| Error("template weight total exceeds i32::MAX".into()))?;
            let entry = field(entry, "data")?;
            let id = id(field(entry, "id")?)?;
            let rotations = match entry.get("rotations") {
                Some(v) => list(v)?.iter().map(rotation).collect::<Result<_, _>>()?,
                None => vec![
                    Rotation::None,
                    Rotation::Clockwise90,
                    Rotation::Clockwise180,
                    Rotation::CounterClockwise90,
                ],
            };
            let path = id.with_path(format!("structure/{}.nbt", id.path()))?;
            let template = if loader.data.resources.get(&path).is_none() {
                // StructureTemplateManager#getOrCreate caches an empty template
                // when every TemplateSource reports that the resource is absent.
                Ok(Arc::new(StructureTemplate::default()))
            } else {
                StructureTemplate::load(loader.data.resources, &id)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            };
            Ok(TemplateEntry {
                template,
                rotations,
                weight,
            })
        })
        .collect::<Result<_, Error>>()?;
    let processors = match v.get("processors") {
        None => Vec::new(),
        Some(v) if v.is_string() => {
            let id = id(v)?;
            let resource = loader
                .data
                .resources
                .require(&id.with_path(format!("worldgen/processor_list/{}.json", id.path()))?)?;
            StructureProcessor::decode_list(&resource.json()?)?
        }
        Some(v) => StructureProcessor::decode_list(v)?,
    };
    Ok(Arc::new(TemplateFeature {
        templates,
        total,
        processors,
        tags: BlockTags(loader.data.block_tags.clone()),
    }))
}
impl Feature for TemplateFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if self.total == 0 {
            return Err(Error("Weighted list has no elements".into()));
        }
        let mut selection = ctx.random()?.next_int_bound(self.total);
        let entry = self
            .templates
            .iter()
            .find(|entry| {
                selection -= entry.weight;
                selection < 0
            })
            .expect("validated weight total");
        if entry.rotations.is_empty() {
            return Err(Error("template rotation list has no elements".into()));
        }
        let rotation =
            entry.rotations[ctx.random()?.next_int_bound(entry.rotations.len() as i32) as usize];
        let template = entry.template.as_ref().map_err(|e| Error(e.clone()))?;
        let half = [-(template.size[0] / 2), 0, -(template.size[2] / 2)];
        let position = offset(
            ctx.origin,
            transform_block(half, Mirror::None, rotation, [0; 3]),
        );
        let mut settings = StructurePlaceSettings {
            rotation,
            random: PlacementRandom::Shared,
            ..StructurePlaceSettings::default()
        };
        template.place_with_access(
            &mut ctx.placement(),
            position,
            position,
            &mut settings,
            &self.processors,
            &self.tags,
            3,
        )
    }
}
