//! Ports of all 26.3 BlockPredicate codecs. Context-dependent block behavior
//! remains on WorldgenBlockBehavior rather than using extracted static shapes.
use crate::{Error, Identifier, codec::*, material::VerticalAnchor, region::WorldGenRegion};
use ferraloom_data::{
    Block, BlockState,
    datapack::{ResourceManager, decode_block_state, load_tags},
    registries::fluid,
};
use ferraloom_util::{Direction, core::Axis};
use serde_json::Value;
use std::collections::BTreeMap;

pub struct PredicateDecodeData<'a> {
    pub resources: &'a ResourceManager,
    pub block_tags: BTreeMap<Identifier, Vec<Block>>,
    pub fluid_tags: BTreeMap<Identifier, Vec<fluid::Id>>,
    pub biome_tags: BTreeMap<Identifier, Vec<Identifier>>,
}
impl<'a> PredicateDecodeData<'a> {
    pub fn load(resources: &'a ResourceManager) -> Result<Self, Error> {
        let blocks = load_tags(resources, "block", |id, _| {
            Block::from_name(&id.to_string())
        });
        let fluids = load_tags(resources, "fluid", |id, _| {
            fluid::Id::from_name(&id.to_string())
        });
        let biome_ids = resources.registry("worldgen/biome");
        let biomes = load_tags(resources, "worldgen/biome", |id, _| {
            biome_ids.contains_key(id).then(|| id.clone())
        });
        if !blocks.diagnostics.is_empty()
            || !fluids.diagnostics.is_empty()
            || !biomes.diagnostics.is_empty()
        {
            return Err(Error("invalid predicate registry tags".into()));
        }
        Ok(Self {
            resources,
            block_tags: blocks.values,
            fluid_tags: fluids.values,
            biome_tags: biomes.values,
        })
    }
    pub fn blocks(&self, v: &Value) -> Result<Vec<Block>, Error> {
        holder_set(v, &self.block_tags, |id| Block::from_name(&id.to_string()))
    }
    pub fn fluids(&self, v: &Value) -> Result<Vec<fluid::Id>, Error> {
        holder_set(v, &self.fluid_tags, |id| {
            fluid::Id::from_name(&id.to_string())
        })
    }
    pub fn biomes(&self, v: &Value) -> Result<Vec<Identifier>, Error> {
        holder_set(v, &self.biome_tags, |id| {
            self.resources
                .registry("worldgen/biome")
                .contains_key(id)
                .then(|| id.clone())
        })
    }
}
fn holder_set<T: Clone>(
    v: &Value,
    tags: &BTreeMap<Identifier, Vec<T>>,
    resolve: impl Fn(&Identifier) -> Option<T>,
) -> Result<Vec<T>, Error> {
    if let Some(text) = v.as_str() {
        if let Some(tag) = text.strip_prefix('#') {
            return tags
                .get(&tag.parse()?)
                .cloned()
                .ok_or_else(|| Error(format!("missing tag {tag}")));
        }
        return Ok(vec![
            resolve(&text.parse()?)
                .ok_or_else(|| Error(format!("unknown registry entry {text}")))?,
        ]);
    }
    list(v)?
        .iter()
        .map(|v| {
            let id = id(v)?;
            resolve(&id).ok_or_else(|| Error(format!("unknown registry entry {id}")))
        })
        .collect()
}
pub fn position(v: &Value, bound: Option<i32>) -> Result<[i32; 3], Error> {
    let values = list(v)?;
    if values.len() != 3 {
        return Err(Error("expected three coordinates".into()));
    }
    let mut p = [0; 3];
    for i in 0..3 {
        p[i] = integer(&values[i])?;
        // Vec3i.offsetCodec uses Math.abs(int); MIN_VALUE remains negative.
        if bound.is_some_and(|bound| p[i].wrapping_abs() > bound) {
            return Err(Error("position offset exceeds bound".into()));
        }
    }
    Ok(p)
}
pub fn direction(v: &Value) -> Result<Direction, Error> {
    match string(v)? {
        "down" => Ok(Direction::Down),
        "up" => Ok(Direction::Up),
        "north" => Ok(Direction::North),
        "south" => Ok(Direction::South),
        "west" => Ok(Direction::West),
        "east" => Ok(Direction::East),
        _ => Err(Error("unknown direction".into())),
    }
}
pub fn offset(p: [i32; 3], delta: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| p[i].wrapping_add(delta[i]))
}
pub fn move_direction(p: [i32; 3], d: Direction) -> [i32; 3] {
    let delta = match d {
        Direction::Down => [0, -1, 0],
        Direction::Up => [0, 1, 0],
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
    };
    offset(p, delta)
}
#[derive(Clone, Debug)]
pub enum BlockPredicate {
    True,
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
    Not(Box<Self>),
    MatchingBlocks([i32; 3], Vec<Block>),
    MatchingFluids([i32; 3], Vec<fluid::Id>),
    MatchingBiomes(Vec<Identifier>),
    Replaceable([i32; 3]),
    Solid([i32; 3]),
    InsideWorldBounds([i32; 3]),
    HasSturdyFace([i32; 3], Direction),
    WouldSurvive([i32; 3], BlockState),
    Unobstructed,
    HeightRange(VerticalAnchor, VerticalAnchor),
    VolumeMatch {
        min: [i32; 3],
        max: [i32; 3],
        predicate: Box<Self>,
    },
}
impl BlockPredicate {
    pub fn decode(v: &Value, data: &PredicateDecodeData<'_>) -> Result<Self, Error> {
        let ty = kind(v)?;
        let delta = || optional(v, "offset", [0; 3], |v| position(v, Some(16)));
        Ok(match ty.to_string().as_str() {
            "minecraft:true" => Self::True,
            "minecraft:all_of" => Self::AllOf(
                list(field(v, "predicates")?)?
                    .iter()
                    .map(|v| Self::decode(v, data))
                    .collect::<Result<_, _>>()?,
            ),
            "minecraft:any_of" => Self::AnyOf(
                list(field(v, "predicates")?)?
                    .iter()
                    .map(|v| Self::decode(v, data))
                    .collect::<Result<_, _>>()?,
            ),
            "minecraft:not" => Self::Not(Box::new(Self::decode(field(v, "predicate")?, data)?)),
            "minecraft:matching_blocks" => {
                Self::MatchingBlocks(delta()?, data.blocks(field(v, "blocks")?)?)
            }
            "minecraft:matching_block_tag" => {
                let tag = id(field(v, "tag")?)?;
                Self::MatchingBlocks(
                    delta()?,
                    data.block_tags
                        .get(&tag)
                        .cloned()
                        .ok_or_else(|| Error(format!("missing block tag {tag}")))?,
                )
            }
            "minecraft:matching_fluids" => {
                Self::MatchingFluids(delta()?, data.fluids(field(v, "fluids")?)?)
            }
            "minecraft:matching_biomes" => Self::MatchingBiomes(data.biomes(field(v, "biomes")?)?),
            "minecraft:replaceable" => Self::Replaceable(delta()?),
            "minecraft:solid" => Self::Solid(delta()?),
            "minecraft:inside_world_bounds" => Self::InsideWorldBounds(delta()?),
            "minecraft:has_sturdy_face" => {
                Self::HasSturdyFace(delta()?, direction(field(v, "direction")?)?)
            }
            "minecraft:would_survive" => {
                Self::WouldSurvive(delta()?, decode_block_state(field(v, "state")?)?)
            }
            "minecraft:unobstructed" => {
                optional(v, "offset", [0; 3], |v| position(v, None))?;
                Self::Unobstructed
            }
            "minecraft:height_range" => Self::HeightRange(
                VerticalAnchor::decode(field(v, "min_inclusive")?)?,
                VerticalAnchor::decode(field(v, "max_inclusive")?)?,
            ),
            "minecraft:volume_match" => {
                let min = position(field(v, "min")?, Some(16))?;
                let max = position(field(v, "max")?, Some(16))?;
                if (0..3).any(|i| min[i] > max[i]) {
                    return Err(Error("volume minimum exceeds maximum".into()));
                }
                Self::VolumeMatch {
                    min,
                    max,
                    predicate: Box::new(Self::decode(field(v, "match")?, data)?),
                }
            }
            _ => return Err(Error(format!("unknown block predicate {ty}"))),
        })
    }
    pub fn test(
        &self,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
        generator: &dyn crate::generator::ChunkGeneratorAccess,
    ) -> Result<bool, Error> {
        Ok(match self {
            Self::True => true,
            Self::AllOf(values) => {
                for value in values {
                    if !value.test(region, p, generator)? {
                        return Ok(false);
                    }
                }
                true
            }
            Self::AnyOf(values) => {
                for value in values {
                    if value.test(region, p, generator)? {
                        return Ok(true);
                    }
                }
                false
            }
            Self::Not(value) => !value.test(region, p, generator)?,
            Self::MatchingBlocks(d, values) => {
                values.contains(&region.block(offset(p, *d))?.block())
            }
            Self::MatchingFluids(d, values) => {
                values.contains(&region.fluid(offset(p, *d))?.fluid_type)
            }
            Self::MatchingBiomes(values) => values.contains(
                &generator.data().biome_names[region
                    .noise_biome(crate::biome_manager::zoom_quart(
                        crate::biome_manager::obfuscate_seed(region.seed),
                        p,
                    ))?
                    .0 as usize],
            ),
            Self::Replaceable(d) => region.block(offset(p, *d))?.data().replaceable,
            Self::Solid(d) => region.is_solid(offset(p, *d))?,
            Self::InsideWorldBounds(d) => {
                let y = p[1].wrapping_add(d[1]);
                y >= region.min_y() && y < region.min_y() + region.height()
            }
            Self::HasSturdyFace(d, face) => region.has_sturdy_face(offset(p, *d), *face)?,
            Self::WouldSurvive(d, state) => region.can_survive(*state, offset(p, *d))?,
            // Vanilla tests entity obstruction against a full unit cube.
            Self::Unobstructed => region.is_unobstructed(Block::STONE.default_state(), p)?,
            Self::HeightRange(min, max) => {
                let min_y = region.min_y().max(generator.min_y());
                let height = region.height().min(generator.gen_depth());
                p[1] >= min.resolve(min_y, height, generator.sea_level())
                    && p[1] <= max.resolve(min_y, height, generator.sea_level())
            }
            Self::VolumeMatch {
                min,
                max,
                predicate,
            } => {
                for x in min[0]..=max[0] {
                    for z in min[2]..=max[2] {
                        for y in min[1]..=max[1] {
                            if !predicate.test(region, offset(p, [x, y, z]), generator)? {
                                return Ok(false);
                            }
                        }
                    }
                }
                true
            }
        })
    }
}
pub fn vertical_direction(v: &Value) -> Result<Direction, Error> {
    let d = direction(v)?;
    if d.axis() != Axis::Y {
        return Err(Error("expected vertical direction".into()));
    }
    Ok(d)
}
