//! Ports of the eighteen 26.3 PlacementModifier types.
use crate::{
    Error,
    codec::*,
    feature::{FeatureContext, FeatureRegistry, PlacedFeature},
    generator::is_air,
    placement::{predicate::*, providers::*},
    region::Heightmap,
};
use ferraloom_data::Block;
use ferraloom_util::{
    noise::{Noise, SimplexNoise},
    random::{LegacyRandomSource, RandomSource, WorldgenRandom},
};
use serde_json::Value;
use std::sync::LazyLock;
static BIOME_INFO: LazyLock<SimplexNoise> = LazyLock::new(|| {
    SimplexNoise::with_discarded_offset(
        &mut WorldgenRandom::new(LegacyRandomSource::new(2345)),
        true,
    )
});
#[derive(Clone, Debug)]
pub enum PlacementModifier {
    BlockPredicateFilter(BlockPredicate),
    RarityFilter(i32),
    RandomChance(f32),
    Biome,
    SurfaceRelativeThreshold {
        map: Heightmap,
        min: i32,
        max: i32,
    },
    SurfaceWaterDepth(i32),
    Count(IntProvider),
    NoiseBasedCount {
        ratio: i32,
        factor: f64,
        offset: f64,
    },
    NoiseThresholdCount {
        threshold: f64,
        below: i32,
        above: i32,
    },
    CountOnEveryLayer(IntProvider),
    Cuboid {
        xz: IntProvider,
        y: IntProvider,
        edges: bool,
        interior: bool,
    },
    EnvironmentScan {
        direction: ferraloom_util::Direction,
        target: BlockPredicate,
        allowed: BlockPredicate,
        steps: i32,
    },
    Heightmap(Heightmap),
    HeightRange(HeightProvider),
    InSquare,
    Offset([IntProvider; 3]),
    RandomlySelected(Vec<Self>),
    Fixed(Vec<[i32; 3]>),
}
fn heightmap(v: &Value) -> Result<Heightmap, Error> {
    match string(v)? {
        "WORLD_SURFACE_WG" => Ok(Heightmap::WorldSurfaceWg),
        "WORLD_SURFACE" => Ok(Heightmap::WorldSurface),
        "OCEAN_FLOOR_WG" => Ok(Heightmap::OceanFloorWg),
        "OCEAN_FLOOR" => Ok(Heightmap::OceanFloor),
        "MOTION_BLOCKING" => Ok(Heightmap::MotionBlocking),
        "MOTION_BLOCKING_NO_LEAVES" => Ok(Heightmap::MotionBlockingNoLeaves),
        _ => Err(Error("unknown heightmap".into())),
    }
}
impl PlacementModifier {
    pub fn decode(v: &Value, data: &PredicateDecodeData<'_>) -> Result<Self, Error> {
        let ty = kind(v)?;
        Ok(match ty.to_string().as_str() {
            "minecraft:block_predicate_filter" => {
                Self::BlockPredicateFilter(BlockPredicate::decode(field(v, "predicate")?, data)?)
            }
            "minecraft:rarity_filter" => {
                Self::RarityFilter(int_range(field(v, "chance")?, 1, i32::MAX)?)
            }
            "minecraft:random_chance" => {
                Self::RandomChance(float_range(field(v, "chance")?, 0.0, 1.0)?)
            }
            "minecraft:biome" => Self::Biome,
            "minecraft:surface_relative_threshold_filter" => Self::SurfaceRelativeThreshold {
                map: heightmap(field(v, "heightmap")?)?,
                min: optional(v, "min_inclusive", i32::MIN, integer)?,
                max: optional(v, "max_inclusive", i32::MAX, integer)?,
            },
            "minecraft:surface_water_depth_filter" => {
                Self::SurfaceWaterDepth(integer(field(v, "max_water_depth")?)?)
            }
            "minecraft:count" => {
                Self::Count(IntProvider::decode_bounded(field(v, "count")?, 0, 4096)?)
            }
            "minecraft:noise_based_count" => Self::NoiseBasedCount {
                ratio: integer(field(v, "noise_to_count_ratio")?)?,
                factor: double(field(v, "noise_factor")?)?,
                offset: optional(v, "noise_offset", 0.0, double)?,
            },
            "minecraft:noise_threshold_count" => Self::NoiseThresholdCount {
                threshold: double(field(v, "noise_level")?)?,
                below: integer(field(v, "below_noise")?)?,
                above: integer(field(v, "above_noise")?)?,
            },
            "minecraft:count_on_every_layer" => {
                Self::CountOnEveryLayer(IntProvider::decode_bounded(field(v, "count")?, 0, 256)?)
            }
            "minecraft:cuboid" => Self::Cuboid {
                xz: IntProvider::decode_bounded(field(v, "xz_size")?, 1, 16)?,
                y: IntProvider::decode_bounded(field(v, "y_size")?, 1, 16)?,
                edges: optional(v, "include_edges", true, boolean)?,
                interior: optional(v, "include_interior", true, boolean)?,
            },
            "minecraft:environment_scan" => Self::EnvironmentScan {
                direction: vertical_direction(field(v, "direction_of_search")?)?,
                target: BlockPredicate::decode(field(v, "target_condition")?, data)?,
                allowed: optional(v, "allowed_search_condition", BlockPredicate::True, |v| {
                    BlockPredicate::decode(v, data)
                })?,
                steps: int_range(field(v, "max_steps")?, 1, 32)?,
            },
            "minecraft:heightmap" => Self::Heightmap(heightmap(field(v, "heightmap")?)?),
            "minecraft:height_range" => {
                Self::HeightRange(HeightProvider::decode(field(v, "height")?)?)
            }
            "minecraft:in_square" => Self::InSquare,
            "minecraft:offset" => Self::Offset([
                IntProvider::decode_bounded(field(v, "x")?, -16, 16)?,
                IntProvider::decode_bounded(field(v, "y")?, -16, 16)?,
                IntProvider::decode_bounded(field(v, "z")?, -16, 16)?,
            ]),
            "minecraft:randomly_selected" => {
                let entries = list(field(v, "placements")?)?;
                if entries.is_empty() {
                    return Err(Error("randomly_selected requires placements".into()));
                }
                Self::RandomlySelected(
                    entries
                        .iter()
                        .map(|v| Self::decode(v, data))
                        .collect::<Result<_, _>>()?,
                )
            }
            "minecraft:fixed_placement" => Self::Fixed(
                list(field(v, "positions")?)?
                    .iter()
                    .map(|v| position(v, None))
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(Error(format!("unknown placement modifier {ty}"))),
        })
    }
    pub fn modify(
        &self,
        context: &mut FeatureContext<'_, '_>,
        registry: &FeatureRegistry,
        top_feature: Option<&PlacedFeature>,
        p: [i32; 3],
    ) -> Result<Vec<[i32; 3]>, Error> {
        let region = &context.level;
        let random = &mut context.random;
        let generator = context.generator;
        let mut result = Vec::new();
        match self {
            Self::Count(count) => result.resize(count.sample(random).max(0) as usize, p),
            Self::NoiseBasedCount {
                ratio,
                factor,
                offset,
            } => {
                let noise = f64::from(
                    BIOME_INFO.get_2d(f64::from(p[0]) / factor, f64::from(p[2]) / factor),
                );
                result.resize(
                    ((noise + offset) * f64::from(*ratio)).ceil().max(0.0) as usize,
                    p,
                );
            }
            Self::NoiseThresholdCount {
                threshold,
                below,
                above,
            } => {
                let count = if f64::from(
                    BIOME_INFO.get_2d(f64::from(p[0]) / 200.0, f64::from(p[2]) / 200.0),
                ) < *threshold
                {
                    below
                } else {
                    above
                };
                result.resize((*count).max(0) as usize, p);
            }
            Self::BlockPredicateFilter(predicate) => {
                if predicate.test(region, p, generator)? {
                    result.push(p);
                }
            }
            Self::RarityFilter(chance) => {
                if random.next_float() < 1.0 / *chance as f32 {
                    result.push(p);
                }
            }
            Self::RandomChance(chance) => {
                if random.next_float() < *chance {
                    result.push(p);
                }
            }
            Self::SurfaceRelativeThreshold { map, min, max } => {
                let y = i64::from(region.height_at(*map, p[0], p[2])?);
                if y + i64::from(*min) <= i64::from(p[1]) && i64::from(p[1]) <= y + i64::from(*max)
                {
                    result.push(p);
                }
            }
            Self::SurfaceWaterDepth(depth) => {
                if region.height_at(Heightmap::WorldSurface, p[0], p[2])?
                    - region.height_at(Heightmap::OceanFloor, p[0], p[2])?
                    <= *depth
                {
                    result.push(p);
                }
            }
            Self::Heightmap(map) => {
                let y = region.height_at(*map, p[0], p[2])?;
                if y > region.min_y() {
                    result.push([p[0], y, p[2]]);
                }
            }
            Self::HeightRange(height) => {
                let context = WorldGenerationContext::new(
                    generator.min_y(),
                    generator.gen_depth(),
                    region.min_y(),
                    region.height(),
                    generator.sea_level(),
                );
                result.push([p[0], height.sample(random, context), p[2]]);
            }
            Self::InSquare => result.push(offset(
                p,
                [random.next_int_bound(16), 0, random.next_int_bound(16)],
            )),
            Self::Offset(delta) => result.push(offset(
                p,
                [
                    delta[0].sample(random),
                    delta[1].sample(random),
                    delta[2].sample(random),
                ],
            )),
            Self::Fixed(positions) => result.extend(
                positions
                    .iter()
                    .copied()
                    .filter(|pos| pos[0] >> 4 == p[0] >> 4 && pos[2] >> 4 == p[2] >> 4),
            ),
            Self::RandomlySelected(placements) => {
                let index = random.next_int_bound(placements.len() as i32) as usize;
                return placements[index].modify(context, registry, top_feature, p);
            }
            Self::Biome => {
                let top =
                    top_feature.ok_or_else(|| Error("biome check without a top feature".into()))?;
                let id = top
                    .id
                    .as_ref()
                    .ok_or_else(|| Error("biome check on an unregistered feature".into()))?;
                let quart = crate::biome_manager::zoom_quart(
                    crate::biome_manager::obfuscate_seed(region.seed()),
                    p,
                );
                let biome = &generator.data().biome_names[region.noise_biome(quart)?.0 as usize];
                let steps = registry
                    .biome_features
                    .get(biome)
                    .ok_or_else(|| Error(format!("missing generation settings for {biome}")))?;
                if steps.iter().flatten().any(|f| f == id) {
                    result.push(p);
                }
            }
            Self::EnvironmentScan {
                direction,
                target,
                allowed,
                steps,
            } => {
                let mut p = p;
                if !allowed.test(region, p, generator)? {
                    return Ok(result);
                }
                for _ in 0..*steps {
                    if target.test(region, p, generator)? {
                        result.push(p);
                        return Ok(result);
                    }
                    p = move_direction(p, *direction);
                    if p[1] < region.min_y() || p[1] >= region.min_y() + region.height() {
                        return Ok(result);
                    }
                    if !allowed.test(region, p, generator)? {
                        break;
                    }
                }
                if target.test(region, p, generator)? {
                    result.push(p);
                }
            }
            Self::Cuboid {
                xz,
                y,
                edges,
                interior,
            } => {
                let height = y.sample(random);
                let width = xz.sample(random);
                let length = xz.sample(random);
                for x in 0..=width {
                    for y in 0..=height {
                        for z in 0..=length {
                            let inner_x = x != 0 && x != width;
                            let inner_y = y != 0 && y != height;
                            let inner_z = z != 0 && z != length;
                            if (*edges || inner_x || inner_y)
                                && (*edges || inner_z || inner_y)
                                && (*edges || inner_x || inner_z)
                                && (*interior || !inner_x || !inner_y || !inner_z)
                            {
                                result.push(offset(p, [x, y, z]));
                            }
                        }
                    }
                }
            }
            Self::CountOnEveryLayer(count) => {
                let mut layer = 0;
                loop {
                    let mut found = false;
                    let mut i = 0;
                    // Vanilla re-samples the provider for every loop condition.
                    while i < count.sample(random) {
                        let x = p[0].wrapping_add(random.next_int_bound(16));
                        let z = p[2].wrapping_add(random.next_int_bound(16));
                        let start = region.height_at(Heightmap::MotionBlocking, x, z)?;
                        let mut current = region.block([x, start, z])?;
                        let mut current_layer = 0;
                        for y in (region.min_y() + 1..=start).rev() {
                            let below = region.block([x, y - 1, z])?;
                            let empty =
                                |s| is_air(s) || matches!(s.block(), Block::WATER | Block::LAVA);
                            if !empty(below) && empty(current) && below.block() != Block::BEDROCK {
                                if current_layer == layer {
                                    result.push([x, y, z]);
                                    found = true;
                                    break;
                                }
                                current_layer += 1;
                            }
                            current = below;
                        }
                        i += 1;
                    }
                    layer += 1;
                    if !found {
                        break;
                    }
                }
            }
        }
        Ok(result)
    }
}
