//! Template pools and jigsaw connectors. Weighted copies retain vanilla order
//! through selection and Fisher-Yates shuffling.
use super::{
    BoundingBox, Projection,
    processor::{StructureProcessor, shuffle},
    template::{LiquidSettings, StructurePlaceSettings, StructureTemplateManager, transform_block},
};
use crate::{
    Error, Identifier,
    codec::*,
    feature::{FeatureContext, PlacedFeature},
    structure::rule::BlockTags,
};
use ferraloom_data::{Block, datapack::ResourceManager, properties};
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{Direction, Mirror, Rotation, random::RandomSource};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

impl Projection {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match string(v)? {
            "rigid" => Ok(Self::Rigid),
            "terrain_matching" => Ok(Self::TerrainMatching),
            s => Err(Error(format!("unknown pool projection {s}"))),
        }
    }
}
impl LiquidSettings {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match string(v)? {
            "apply_waterlogging" => Ok(Self::ApplyWaterlogging),
            "ignore_waterlogging" => Ok(Self::IgnoreWaterlogging),
            s => Err(Error(format!("unknown liquid settings {s}"))),
        }
    }
}
#[derive(Clone, Debug)]
pub enum PoolFeature {
    Reference(Identifier),
    Direct(Arc<PlacedFeature>),
}
#[derive(Clone, Debug)]
pub struct ProcessorList {
    pub id: Option<Identifier>,
    pub processors: Arc<Vec<StructureProcessor>>,
}
/// Port of the five StructurePoolElementType codecs (single and legacy share
/// storage but retain different processor ordering on placement).
#[derive(Clone, Debug)]
pub enum StructurePoolElement {
    Empty,
    Single {
        location: Identifier,
        processors: ProcessorList,
        projection: Projection,
        liquid: Option<LiquidSettings>,
        legacy: bool,
    },
    List {
        elements: Vec<Arc<Self>>,
        projection: Projection,
    },
    Feature {
        feature: PoolFeature,
        projection: Projection,
    },
}
impl StructurePoolElement {
    pub fn decode(
        v: &Value,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let ty = id(field(v, "element_type")?)?;
        if ty.to_string() == "minecraft:empty_pool_element" {
            return Ok(Self::Empty);
        }
        let projection = Projection::decode(field(v, "projection")?)?;
        match ty.to_string().as_str() {
            "minecraft:single_pool_element" | "minecraft:legacy_single_pool_element" => {
                let value = field(v, "processors")?;
                let (processor_id, value) = if value.is_string() {
                    let id = id(value)?;
                    let value = resources
                        .require(
                            &id.with_path(format!("worldgen/processor_list/{}.json", id.path()))?,
                        )?
                        .json()?;
                    (Some(id), value)
                } else {
                    (None, value.clone())
                };
                Ok(Self::Single {
                    location: id(field(v, "location")?)?,
                    processors: ProcessorList {
                        id: processor_id,
                        processors: Arc::new(StructureProcessor::decode_list(&value)?),
                    },
                    projection,
                    liquid: v
                        .get("override_liquid_settings")
                        .map(LiquidSettings::decode)
                        .transpose()?,
                    legacy: ty.path() == "legacy_single_pool_element",
                })
            }
            "minecraft:list_pool_element" => {
                let elements = list(field(v, "elements")?)?;
                if elements.is_empty() {
                    return Err(Error("Elements are empty".into()));
                }
                let elements = elements
                    .iter()
                    .map(|v| {
                        let mut element = Self::decode(v, resources, decode_feature)?;
                        element.set_projection(projection);
                        Ok(Arc::new(element))
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(Self::List {
                    elements,
                    projection,
                })
            }
            "minecraft:feature_pool_element" => {
                let feature = field(v, "feature")?;
                Ok(Self::Feature {
                    feature: if feature.is_string() {
                        PoolFeature::Reference(id(feature)?)
                    } else {
                        PoolFeature::Direct(decode_feature(feature)?)
                    },
                    projection,
                })
            }
            _ => Err(Error(format!("unknown pool element {ty}"))),
        }
    }
    fn set_projection(&mut self, p: Projection) {
        match self {
            Self::Empty => {}
            Self::Single { projection, .. } | Self::Feature { projection, .. } => *projection = p,
            Self::List {
                elements,
                projection,
            } => {
                *projection = p;
                for element in elements {
                    Arc::make_mut(element).set_projection(p);
                }
            }
        }
    }
    pub fn projection(&self) -> Projection {
        match self {
            Self::Empty => Projection::TerrainMatching,
            Self::Single { projection, .. }
            | Self::List { projection, .. }
            | Self::Feature { projection, .. } => *projection,
        }
    }
    pub fn ground_level_delta(&self) -> i32 {
        1
    }
    pub fn size(
        &self,
        templates: &StructureTemplateManager,
        rotation: Rotation,
    ) -> Result<[i32; 3], Error> {
        match self {
            Self::Empty | Self::Feature { .. } => Ok([0; 3]),
            Self::Single { location, .. } => {
                Ok(templates.get_or_create(location).rotated_size(rotation))
            }
            Self::List { elements, .. } => {
                let mut size = [0; 3];
                for element in elements {
                    let s = element.size(templates, rotation)?;
                    size = std::array::from_fn(|i| size[i].max(s[i]));
                }
                Ok(size)
            }
        }
    }
    pub fn bounding_box(
        &self,
        templates: &StructureTemplateManager,
        position: [i32; 3],
        rotation: Rotation,
    ) -> Result<BoundingBox, Error> {
        match self {
            Self::Empty => Err(Error(
                "Invalid call to EmptyPoolElement.getBoundingBox".into(),
            )),
            Self::Feature { .. } => Ok(BoundingBox::point(position)),
            Self::Single { location, .. } => Ok(templates.get_or_create(location).bounding_box(
                &StructurePlaceSettings {
                    rotation,
                    ..Default::default()
                },
                position,
            )),
            Self::List { elements, .. } => {
                let mut bounds: Option<BoundingBox> = None;
                for element in elements {
                    if matches!(element.as_ref(), Self::Empty) {
                        continue;
                    }
                    let b = element.bounding_box(templates, position, rotation)?;
                    bounds = Some(bounds.map_or(b, |v| v.encapsulating(b)));
                }
                bounds.ok_or_else(|| {
                    Error("Unable to calculate boundingbox for ListPoolElement".into())
                })
            }
        }
    }
    pub fn jigsaws(
        &self,
        templates: &StructureTemplateManager,
        position: [i32; 3],
        rotation: Rotation,
        random: &mut impl RandomSource,
    ) -> Result<Vec<JigsawBlockInfo>, Error> {
        match self {
            Self::Empty => Ok(Vec::new()),
            Self::List { elements, .. } => {
                elements[0].jigsaws(templates, position, rotation, random)
            }
            Self::Feature { .. } => Ok(vec![JigsawBlockInfo {
                position,
                front: Direction::Down,
                top: Direction::South,
                rollable: true,
                name: None,
                pool: "empty".parse()?,
                target: "empty".parse()?,
                placement_priority: 0,
                selection_priority: 0,
            }]),
            Self::Single { location, .. } => {
                let template = templates.get_or_create(location);
                if template.palettes.is_empty() {
                    return Ok(Vec::new());
                }
                let palette = template.random_palette(
                    &mut StructurePlaceSettings::default(),
                    position,
                    random,
                )?;
                let mut values = Vec::new();
                for block in &palette.blocks {
                    if block.state.block() == Block::JIGSAW {
                        let mut info = JigsawBlockInfo::parse(
                            block
                                .nbt
                                .as_ref()
                                .ok_or_else(|| Error("jigsaw is missing nbt".into()))?,
                            block.state.rotate(rotation),
                        )?;
                        let p = transform_block(block.position, Mirror::None, rotation, [0; 3]);
                        info.position = std::array::from_fn(|i| p[i].wrapping_add(position[i]));
                        values.push(info);
                    }
                }
                shuffle(&mut values, random);
                values.sort_by_key(|info| std::cmp::Reverse(info.selection_priority));
                Ok(values)
            }
        }
    }
    /// Port of SinglePoolElement#getSettings, including LegacySingle's final
    /// air ignore processor. Processors may inspect states before that ignore.
    pub fn placement_settings(
        &self,
        rotation: Rotation,
        bounds: BoundingBox,
        liquid: LiquidSettings,
        keep_jigsaws: bool,
    ) -> Result<(StructurePlaceSettings, Vec<StructureProcessor>), Error> {
        let Self::Single {
            processors,
            projection,
            liquid: override_liquid,
            legacy,
            ..
        } = self
        else {
            return Err(Error("settings only exist for single pool elements".into()));
        };
        let settings = StructurePlaceSettings {
            rotation,
            bounding_box: Some(bounds),
            known_shape: true,
            ignore_entities: false,
            finalize_entities: true,
            liquid_settings: override_liquid.unwrap_or(liquid),
            ..Default::default()
        };
        let mut list = Vec::new();
        if !legacy {
            list.push(StructureProcessor::BlockIgnore(vec![
                Block::STRUCTURE_BLOCK,
            ]));
        }
        if !keep_jigsaws {
            list.push(StructureProcessor::JigsawReplacement);
        }
        list.extend(processors.processors.iter().cloned());
        if *projection == Projection::TerrainMatching {
            list.push(StructureProcessor::Gravity {
                heightmap: crate::region::Heightmap::WorldSurfaceWg,
                offset: -1,
            });
        }
        if *legacy {
            list.push(StructureProcessor::BlockIgnore(vec![
                Block::AIR,
                Block::STRUCTURE_BLOCK,
            ]));
        }
        Ok((settings, list))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &self,
        templates: &StructureTemplateManager,
        context: &mut FeatureContext<'_, '_>,
        position: [i32; 3],
        reference: [i32; 3],
        rotation: Rotation,
        bounds: BoundingBox,
        liquid: LiquidSettings,
        keep_jigsaws: bool,
        tags: &BlockTags,
    ) -> Result<bool, Error> {
        match self {
            Self::Empty => Ok(true),
            Self::List { elements, .. } => {
                for element in elements {
                    if !element.place(
                        templates,
                        context,
                        position,
                        reference,
                        rotation,
                        bounds,
                        liquid,
                        keep_jigsaws,
                        tags,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Feature { feature, .. } => {
                let feature = match feature {
                    PoolFeature::Direct(f) => f.clone(),
                    PoolFeature::Reference(id) => context
                        .registry
                        .placed
                        .get(id)
                        .cloned()
                        .ok_or_else(|| Error(format!("missing pool feature {id}")))?,
                };
                let origin = context.origin;
                context.origin = position;
                let result = feature.place(context, false);
                context.origin = origin;
                result
            }
            Self::Single { location, .. } => {
                let (mut settings, processors) =
                    self.placement_settings(rotation, bounds, liquid, keep_jigsaws)?;
                let template = templates.get_or_create(location);
                template.place_in_world(
                    context.region,
                    position,
                    reference,
                    &mut settings,
                    &processors,
                    tags,
                    context.random,
                    18,
                )
            }
        }
    }
    pub fn template_locations(&self, output: &mut Vec<Identifier>) {
        match self {
            Self::Single { location, .. } => output.push(location.clone()),
            Self::List { elements, .. } => {
                for e in elements {
                    e.template_locations(output)
                }
            }
            _ => {}
        }
    }
}
/// Port of StructureTemplate.JigsawBlockInfo with decoded facing directions.
#[derive(Clone, Debug)]
pub struct JigsawBlockInfo {
    pub position: [i32; 3],
    pub front: Direction,
    pub top: Direction,
    pub rollable: bool,
    pub name: Option<Identifier>,
    pub pool: Identifier,
    pub target: Identifier,
    pub placement_priority: i32,
    pub selection_priority: i32,
}
impl JigsawBlockInfo {
    fn parse(nbt: &CompoundTag, state: ferraloom_data::BlockState) -> Result<Self, Error> {
        let orientation = state
            .get(properties::ORIENTATION)
            .ok_or_else(|| Error("invalid jigsaw orientation".into()))?;
        let (front, top) = orientation
            .as_str()
            .split_once('_')
            .ok_or_else(|| Error("invalid jigsaw orientation".into()))?;
        let front = Direction::by_name(front).unwrap();
        let top = Direction::by_name(top).unwrap();
        let identifier = |name| {
            nbt.get_string(name)
                .and_then(|s| s.to_string_lossy().parse::<Identifier>().ok())
                .unwrap_or_else(|| "empty".parse().unwrap())
        };
        let rollable = nbt
            .get_string("joint")
            .and_then(|v| match v.to_string_lossy().as_str() {
                "aligned" => Some(false),
                "rollable" => Some(true),
                _ => None,
            })
            .unwrap_or(matches!(front, Direction::Up | Direction::Down));
        Ok(Self {
            position: [0; 3],
            front,
            top,
            rollable,
            name: Some(identifier("name")),
            pool: identifier("pool"),
            target: identifier("target"),
            placement_priority: nbt.get_int("placement_priority").unwrap_or(0),
            selection_priority: nbt.get_int("selection_priority").unwrap_or(0),
        })
    }
    pub fn can_attach(&self, target: &Self) -> bool {
        self.front == target.front.opposite()
            && (self.rollable || self.top == target.top)
            && target.name.as_ref().is_none_or(|name| self.target == *name)
    }
}
#[derive(Clone, Debug)]
pub enum PoolHolder {
    Reference(Identifier),
    Direct(Box<StructureTemplatePool>),
}
/// Port of StructureTemplatePool. Weighted entries expand only for shuffling.
#[derive(Clone, Debug)]
pub struct StructureTemplatePool {
    pub fallback: PoolHolder,
    pub elements: Vec<(Arc<StructurePoolElement>, i32)>,
    size: i32,
}
impl StructureTemplatePool {
    pub fn decode(
        v: &Value,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let fallback = field(v, "fallback")?;
        let fallback = if fallback.is_string() {
            PoolHolder::Reference(id(fallback)?)
        } else {
            PoolHolder::Direct(Box::new(Self::decode(fallback, resources, decode_feature)?))
        };
        let mut size = 0_i32;
        let elements = list(field(v, "elements")?)?
            .iter()
            .map(|v| {
                let weight = int_range(field(v, "weight")?, 1, 150)?;
                size = size
                    .checked_add(weight)
                    .ok_or_else(|| Error("pool weight overflow".into()))?;
                Ok((
                    Arc::new(StructurePoolElement::decode(
                        field(v, "element")?,
                        resources,
                        decode_feature,
                    )?),
                    weight,
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            fallback,
            elements,
            size,
        })
    }
    pub fn size(&self) -> i32 {
        self.size
    }
    pub fn random_template(&self, random: &mut impl RandomSource) -> Arc<StructurePoolElement> {
        if self.size == 0 {
            return Arc::new(StructurePoolElement::Empty);
        }
        let mut choice = random.next_int_bound(self.size);
        for (element, weight) in &self.elements {
            choice -= weight;
            if choice < 0 {
                return element.clone();
            }
        }
        unreachable!("validated pool weight sum")
    }
    pub fn shuffled_templates(
        &self,
        random: &mut impl RandomSource,
    ) -> Vec<Arc<StructurePoolElement>> {
        let mut result = Vec::with_capacity(self.size as usize);
        for (element, weight) in &self.elements {
            result.extend(std::iter::repeat_n(element.clone(), *weight as usize));
        }
        shuffle(&mut result, random);
        result
    }
    pub fn max_size(&self, templates: &StructureTemplateManager) -> Result<i32, Error> {
        let mut max = 0;
        for (e, _) in &self.elements {
            if matches!(e.as_ref(), StructurePoolElement::Empty) {
                continue;
            }
            max = max.max(e.bounding_box(templates, [0; 3], Rotation::None)?.span()[1]);
        }
        Ok(max)
    }
}
#[derive(Clone, Debug, Default)]
pub struct StructurePools {
    pub pools: BTreeMap<Identifier, StructureTemplatePool>,
}
impl StructurePools {
    pub fn load(
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let pools = resources
            .registry("worldgen/template_pool")
            .into_iter()
            .map(|(id, r)| {
                Ok((
                    id,
                    StructureTemplatePool::decode(&r.json()?, resources, decode_feature)?,
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { pools })
    }
    pub fn get(&self, id: &Identifier) -> Result<&StructureTemplatePool, Error> {
        self.pools
            .get(id)
            .ok_or_else(|| Error(format!("missing template pool {id}")))
    }
    pub fn resolve<'a>(
        &'a self,
        holder: &'a PoolHolder,
    ) -> Result<&'a StructureTemplatePool, Error> {
        match holder {
            PoolHolder::Reference(id) => self.get(id),
            PoolHolder::Direct(pool) => Ok(pool),
        }
    }
    pub fn templates(&self) -> Vec<Identifier> {
        let mut names = Vec::new();
        for pool in self.pools.values() {
            for (element, _) in &pool.elements {
                element.template_locations(&mut names);
            }
        }
        names.sort();
        names.dedup();
        names
    }
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
