//! 26.3 WorldCarver geometry. Carvers union a mask; terrain applies it once,
//! through aquifers and material rules after the surface stage.
use crate::{Error, Identifier, codec::*, placement::providers::*};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::{
    mth,
    random::{LegacyRandomSource, RandomSource, SingleThreadedRandomSource, WorldgenRandom},
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// Port of CarvingMask. Layout is Y contiguous, then Z, then X. Visit emits
/// each maximal vertical run, in X/Z/ascending-bottom order.
#[derive(Clone, Debug)]
pub struct CarvingMask {
    pub min_y: i32,
    pub max_y: i32,
    words: Vec<u64>,
}
impl CarvingMask {
    pub fn new(min_y: i32, max_y: i32) -> Self {
        assert!(max_y >= min_y);
        Self {
            min_y,
            max_y,
            words: vec![0; (256 * (max_y - min_y + 1) as usize).div_ceil(64)],
        }
    }
    fn index(&self, x: u8, y: i32, z: u8) -> usize {
        assert!(x < 16 && z < 16 && (self.min_y..=self.max_y).contains(&y));
        (y - self.min_y) as usize
            + (usize::from(z) + (usize::from(x) << 4)) * (self.max_y - self.min_y + 1) as usize
    }
    pub fn carve(&mut self, x: u8, y: i32, z: u8) {
        let n = self.index(x, y, z);
        self.words[n / 64] |= 1 << (n % 64);
    }
    pub fn contains(&self, x: u8, y: i32, z: u8) -> bool {
        let n = self.index(x, y, z);
        self.words[n / 64] & (1 << (n % 64)) != 0
    }
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }
    pub fn visit(&self, mut visitor: impl FnMut(u8, u8, i32, i32)) {
        for x in 0..16 {
            for z in 0..16 {
                let mut y = self.min_y;
                while y <= self.max_y {
                    if !self.contains(x, y, z) {
                        y += 1;
                        continue;
                    }
                    let bottom = y;
                    while y < self.max_y && self.contains(x, y + 1, z) {
                        y += 1;
                    }
                    visitor(x, z, bottom, y);
                    y += 1;
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct CaveWorldCarver {
    pub probability: f32,
    pub y: HeightProvider,
    pub count: IntProvider,
    pub thickness: FloatProvider,
    pub weird_thickness_bias: bool,
    pub room_vertical_radius_multiplier: FloatProvider,
    pub horizontal_radius_multiplier: FloatProvider,
    pub vertical_radius_multiplier: FloatProvider,
    pub start_vertical_radius_multiplier: FloatProvider,
    pub floor_level: FloatProvider,
}
#[derive(Clone, Debug)]
pub struct CanyonWorldCarver {
    pub probability: f32,
    pub y: HeightProvider,
    pub vertical_rotation: FloatProvider,
    pub shape: CanyonShape,
}
#[derive(Clone, Debug)]
pub struct CanyonShape {
    pub distance_factor: FloatProvider,
    pub thickness: FloatProvider,
    pub width_smoothness: i32,
    pub horizontal_radius_factor: FloatProvider,
    pub vertical_radius_default_factor: f32,
    pub vertical_radius_center_factor: f32,
    pub y_scale: FloatProvider,
}
#[derive(Clone, Debug)]
pub enum WorldCarver {
    Cave(CaveWorldCarver),
    Canyon(CanyonWorldCarver),
}
impl WorldCarver {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let ty = kind(v)?;
        let probability = float(field(v, "probability")?)?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(Error("carver probability outside [0,1]".into()));
        }
        let y = HeightProvider::decode(field(v, "y")?)?;
        Ok(match ty.to_string().as_str() {
            "minecraft:cave" => Self::Cave(CaveWorldCarver {
                probability,
                y,
                count: IntProvider::decode_bounded(field(v, "count")?, 0, i32::MAX)?,
                thickness: FloatProvider::decode_bounded(
                    field(v, "thickness")?,
                    0.0,
                    f32::INFINITY,
                )?,
                weird_thickness_bias: optional(v, "weird_thickness_bias", false, boolean)?,
                room_vertical_radius_multiplier: FloatProvider::decode(field(
                    v,
                    "room_vertical_radius_multiplier",
                )?)?,
                horizontal_radius_multiplier: FloatProvider::decode(field(
                    v,
                    "horizontal_radius_multiplier",
                )?)?,
                vertical_radius_multiplier: FloatProvider::decode(field(
                    v,
                    "vertical_radius_multiplier",
                )?)?,
                start_vertical_radius_multiplier: optional(
                    v,
                    "start_vertical_radius_multiplier",
                    FloatProvider::Constant(1.0),
                    FloatProvider::decode,
                )?,
                floor_level: FloatProvider::decode_bounded(field(v, "floor_level")?, -1.0, 1.0)?,
            }),
            "minecraft:canyon" => {
                let s = field(v, "shape")?;
                Self::Canyon(CanyonWorldCarver {
                    probability,
                    y,
                    vertical_rotation: FloatProvider::decode(field(v, "vertical_rotation")?)?,
                    shape: CanyonShape {
                        distance_factor: FloatProvider::decode(field(s, "distance_factor")?)?,
                        thickness: FloatProvider::decode(field(s, "thickness")?)?,
                        width_smoothness: int_range(field(s, "width_smoothness")?, 1, i32::MAX)?,
                        horizontal_radius_factor: FloatProvider::decode(field(
                            s,
                            "horizontal_radius_factor",
                        )?)?,
                        vertical_radius_default_factor: float(field(
                            s,
                            "vertical_radius_default_factor",
                        )?)?,
                        vertical_radius_center_factor: float(field(
                            s,
                            "vertical_radius_center_factor",
                        )?)?,
                        y_scale: FloatProvider::decode(field(s, "y_scale")?)?,
                    },
                })
            }
            _ => return Err(Error(format!("unknown carver type {ty}"))),
        })
    }
    pub fn is_start_chunk(&self, random: &mut impl RandomSource) -> bool {
        random.next_float()
            <= match self {
                Self::Cave(c) => c.probability,
                Self::Canyon(c) => c.probability,
            }
    }
    pub fn carve(
        &self,
        context: WorldGenerationContext,
        random: &mut impl RandomSource,
        chunk: [i32; 2],
        source: [i32; 2],
        output: &mut CarvingMask,
    ) {
        match self {
            Self::Cave(c) => c.carve(context, random, chunk, source, output),
            Self::Canyon(c) => c.carve(context, random, chunk, source, output),
        }
    }
}

/// Port of WorldCarver#carveEllipsoid, including the exclusive lower Y bound
/// and half-block vertical offset (worldY - 0.5).
pub fn carve_ellipsoid(
    chunk: [i32; 2],
    center: [f64; 3],
    horizontal: f64,
    vertical: f64,
    output: &mut CarvingMask,
    skip: impl Fn(f64, f64, f64, i32) -> bool,
) {
    let [x, y, z] = center;
    let max_delta = 16.0 + horizontal * 2.0;
    if (x - f64::from(chunk[0] * 16 + 8)).abs() > max_delta
        || (z - f64::from(chunk[1] * 16 + 8)).abs() > max_delta
    {
        return;
    }
    let min_x = (mth::floor(x - horizontal) - chunk[0] * 16 - 1).max(0);
    let max_x = (mth::floor(x + horizontal) - chunk[0] * 16).min(15);
    let min_z = (mth::floor(z - horizontal) - chunk[1] * 16 - 1).max(0);
    let max_z = (mth::floor(z + horizontal) - chunk[1] * 16).min(15);
    let min_y = (mth::floor(y - vertical) - 1).max(output.min_y);
    let max_y = (mth::floor(y + vertical) + 1).min(output.max_y);
    for lx in min_x..=max_x {
        let xd = (f64::from(chunk[0] * 16 + lx) + 0.5 - x) / horizontal;
        for lz in min_z..=max_z {
            let zd = (f64::from(chunk[1] * 16 + lz) + 0.5 - z) / horizontal;
            if xd * xd + zd * zd >= 1.0 {
                continue;
            }
            for wy in (min_y + 1..=max_y).rev() {
                let yd = (f64::from(wy) - 0.5 - y) / vertical;
                if !skip(xd, yd, zd, wy) {
                    output.carve(lx as u8, wy, lz as u8);
                }
            }
        }
    }
}
fn can_reach(chunk: [i32; 2], p: [f64; 3], step: i32, total: i32, thickness: f32) -> bool {
    let dx = p[0] - f64::from(chunk[0] * 16 + 8);
    let dz = p[2] - f64::from(chunk[1] * 16 + 8);
    let remaining = f64::from(total - step);
    let radius = f64::from(thickness + 2.0 + 16.0);
    dx * dx + dz * dz - remaining * remaining <= radius * radius
}
#[derive(Clone, Copy)]
struct Tunnel {
    p: [f64; 3],
    horizontal: f32,
    vertical: f32,
    thickness: f32,
    step: i32,
    distance: i32,
    y_scale: f64,
}
impl CaveWorldCarver {
    fn carve(
        &self,
        context: WorldGenerationContext,
        random: &mut impl RandomSource,
        chunk: [i32; 2],
        source: [i32; 2],
        output: &mut CarvingMask,
    ) {
        let count = self.count.sample(random);
        for _ in 0..count {
            let x = f64::from(source[0] * 16 + random.next_int_bound(16));
            let y = f64::from(self.y.sample(random, context));
            let z = f64::from(source[1] * 16 + random.next_int_bound(16));
            let horizontal = f64::from(self.horizontal_radius_multiplier.sample(random));
            let vertical = f64::from(self.vertical_radius_multiplier.sample(random));
            let y_scale = f64::from(self.start_vertical_radius_multiplier.sample(random));
            let floor = f64::from(self.floor_level.sample(random));
            let skip = |xd: f64, yd: f64, zd: f64, _: i32| {
                yd <= floor || xd * xd + yd * yd + zd * zd >= 1.0
            };
            let mut tunnels = 1;
            if random.next_int_bound(4) == 0 {
                let room_scale = f64::from(self.room_vertical_radius_multiplier.sample(random));
                let thickness = 1.0 + random.next_float() * 6.0;
                let radius = 1.5 + f64::from(mth::sin(1.5707963705062866) * thickness);
                carve_ellipsoid(
                    chunk,
                    [x + 1.0, y, z],
                    radius,
                    radius * room_scale,
                    output,
                    skip,
                );
                tunnels += random.next_int_bound(4);
            }
            for _ in 0..tunnels {
                let horizontal_rotation = random.next_float() * std::f32::consts::TAU;
                let vertical_rotation = (random.next_float() - 0.5) / 4.0;
                let mut thickness = self.thickness.sample(random);
                if self.weird_thickness_bias && random.next_int_bound(10) == 0 {
                    thickness *= random.next_float() * random.next_float() * 3.0 + 1.0;
                }
                let distance = 112 - random.next_int_bound(28);
                Self::tunnel(
                    chunk,
                    random.next_long(),
                    Tunnel {
                        p: [x, y, z],
                        horizontal: horizontal_rotation,
                        vertical: vertical_rotation,
                        thickness,
                        step: 0,
                        distance,
                        y_scale,
                    },
                    [horizontal, vertical],
                    output,
                    &skip,
                );
            }
        }
    }
    fn tunnel(
        chunk: [i32; 2],
        seed: i64,
        mut t: Tunnel,
        multipliers: [f64; 2],
        output: &mut CarvingMask,
        skip: &impl Fn(f64, f64, f64, i32) -> bool,
    ) {
        let mut random = SingleThreadedRandomSource::new(seed);
        let split = random.next_int_bound(t.distance / 2) + t.distance / 4;
        let steep = random.next_int_bound(6) == 0;
        let mut yaw = 0.0f32;
        let mut pitch = 0.0f32;
        for step in t.step..t.distance {
            let radius = 1.5
                + f64::from(
                    mth::sin(f64::from(
                        std::f32::consts::PI * step as f32 / t.distance as f32,
                    )) * t.thickness,
                );
            let vertical_radius = radius * t.y_scale;
            let cos = mth::cos(f64::from(t.vertical));
            t.p[0] += f64::from(mth::cos(f64::from(t.horizontal)) * cos);
            t.p[1] += f64::from(mth::sin(f64::from(t.vertical)));
            t.p[2] += f64::from(mth::sin(f64::from(t.horizontal)) * cos);
            t.vertical *= if steep { 0.92 } else { 0.7 };
            t.vertical += pitch * 0.1;
            t.horizontal += yaw * 0.1;
            pitch *= 0.9;
            yaw *= 0.75;
            pitch += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if step == split && t.thickness > 1.0 {
                let seed = random.next_long();
                let thickness = random.next_float() * 0.5 + 0.5;
                Self::tunnel(
                    chunk,
                    seed,
                    Tunnel {
                        thickness,
                        horizontal: t.horizontal - std::f32::consts::FRAC_PI_2,
                        vertical: t.vertical / 3.0,
                        step,
                        y_scale: 1.0,
                        ..t
                    },
                    multipliers,
                    output,
                    skip,
                );
                let seed = random.next_long();
                let thickness = random.next_float() * 0.5 + 0.5;
                Self::tunnel(
                    chunk,
                    seed,
                    Tunnel {
                        thickness,
                        horizontal: t.horizontal + std::f32::consts::FRAC_PI_2,
                        vertical: t.vertical / 3.0,
                        step,
                        y_scale: 1.0,
                        ..t
                    },
                    multipliers,
                    output,
                    skip,
                );
                return;
            }
            if random.next_int_bound(4) != 0 {
                if !can_reach(chunk, t.p, step, t.distance, t.thickness) {
                    return;
                }
                carve_ellipsoid(
                    chunk,
                    t.p,
                    radius * multipliers[0],
                    vertical_radius * multipliers[1],
                    output,
                    skip,
                );
            }
        }
    }
}
impl CanyonWorldCarver {
    fn carve(
        &self,
        context: WorldGenerationContext,
        random: &mut impl RandomSource,
        chunk: [i32; 2],
        source: [i32; 2],
        output: &mut CarvingMask,
    ) {
        let x = f64::from(source[0] * 16 + random.next_int_bound(16));
        let y = f64::from(self.y.sample(random, context));
        let z = f64::from(source[1] * 16 + random.next_int_bound(16));
        let horizontal = random.next_float() * std::f32::consts::TAU;
        let vertical = self.vertical_rotation.sample(random);
        let y_scale = f64::from(self.shape.y_scale.sample(random));
        let thickness = self.shape.thickness.sample(random);
        let distance = (112.0 * self.shape.distance_factor.sample(random)) as i32;
        self.tunnel(
            context,
            chunk,
            random.next_long(),
            Tunnel {
                p: [x, y, z],
                horizontal,
                vertical,
                thickness,
                step: 0,
                distance,
                y_scale,
            },
            output,
        );
    }
    fn tunnel(
        &self,
        context: WorldGenerationContext,
        chunk: [i32; 2],
        seed: i64,
        mut t: Tunnel,
        output: &mut CarvingMask,
    ) {
        let mut random = SingleThreadedRandomSource::new(seed);
        let mut width = 1.0f32;
        let widths = (0..context.height)
            .map(|y| {
                if y == 0 || random.next_int_bound(self.shape.width_smoothness) == 0 {
                    width = 1.0 + random.next_float() * random.next_float();
                }
                width * width
            })
            .collect::<Vec<_>>();
        let mut yaw = 0.0f32;
        let mut pitch = 0.0f32;
        for step in 0..t.distance {
            let mut radius = 1.5
                + f64::from(
                    mth::sin(f64::from(
                        step as f32 * std::f32::consts::PI / t.distance as f32,
                    )) * t.thickness,
                );
            let mut vertical_radius = radius * t.y_scale;
            radius *= f64::from(self.shape.horizontal_radius_factor.sample(&mut random));
            let vertical_multiplier = 1.0 - (0.5 - step as f32 / t.distance as f32).abs() * 2.0;
            let factor = self.shape.vertical_radius_default_factor
                + self.shape.vertical_radius_center_factor * vertical_multiplier;
            vertical_radius = f64::from(factor)
                * vertical_radius
                * f64::from(mth::random_between(&mut random, 0.75, 1.0));
            let cos = mth::cos(f64::from(t.vertical));
            let sin = mth::sin(f64::from(t.vertical));
            t.p[0] += f64::from(mth::cos(f64::from(t.horizontal)) * cos);
            t.p[1] += f64::from(sin);
            t.p[2] += f64::from(mth::sin(f64::from(t.horizontal)) * cos);
            t.vertical *= 0.7;
            t.vertical += pitch * 0.05;
            t.horizontal += yaw * 0.05;
            pitch *= 0.8;
            yaw *= 0.5;
            pitch += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if random.next_int_bound(4) != 0 {
                if !can_reach(chunk, t.p, step, t.distance, t.thickness) {
                    return;
                }
                carve_ellipsoid(
                    chunk,
                    t.p,
                    radius,
                    vertical_radius,
                    output,
                    |xd, yd, zd, y| {
                        (xd * xd + zd * zd) * f64::from(widths[(y - context.min_y - 1) as usize])
                            + yd * yd / 6.0
                            >= 1.0
                    },
                );
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CarverRegistry {
    pub carvers: BTreeMap<Identifier, Arc<WorldCarver>>,
    pub biomes: BTreeMap<Identifier, Vec<Arc<WorldCarver>>>,
}
impl CarverRegistry {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let carvers: BTreeMap<_, _> = resources
            .registry("worldgen/carver")
            .into_iter()
            .map(|(id, r)| Ok((id, Arc::new(WorldCarver::decode(&r.json()?)?))))
            .collect::<Result<_, Error>>()?;
        let tags = ferraloom_data::datapack::load_tags(resources, "worldgen/carver", |id, _| {
            carvers.contains_key(id).then(|| id.clone())
        });
        if !tags.diagnostics.is_empty() {
            return Err(Error("invalid carver tags".into()));
        }
        let holder = |v: &Value| -> Result<Arc<WorldCarver>, Error> {
            if v.is_string() {
                let id = crate::codec::id(v)?;
                carvers
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| Error(format!("missing carver {id}")))
            } else {
                Ok(Arc::new(WorldCarver::decode(v)?))
            }
        };
        let biomes = resources
            .registry("worldgen/biome")
            .into_iter()
            .map(|(id, r)| {
                let v = r.json()?;
                let entries = field(&v, "carvers")?;
                let carvers = if let Some(tag) = entries.as_str().and_then(|s| s.strip_prefix('#'))
                {
                    tags.values
                        .get(&tag.parse()?)
                        .ok_or_else(|| Error(format!("missing carver tag {tag}")))?
                        .iter()
                        .map(|id| {
                            carvers
                                .get(id)
                                .cloned()
                                .ok_or_else(|| Error(format!("missing carver {id}")))
                        })
                        .collect::<Result<_, _>>()?
                } else if let Some(entries) = entries.as_array() {
                    entries.iter().map(&holder).collect::<Result<_, _>>()?
                } else {
                    vec![holder(entries)?]
                };
                Ok((id, carvers))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { carvers, biomes })
    }
    /// Port of NoiseBasedChunkGenerator#generateCarvers neighbor iteration and
    /// seed handling. Biome sampling is at the source chunk's minimum X/Z, Y=0.
    pub fn generate_mask(
        &self,
        context: WorldGenerationContext,
        seed: i64,
        chunk: [i32; 2],
        upgrading: bool,
        mut biome: impl FnMut([i32; 3]) -> Identifier,
    ) -> Result<CarvingMask, Error> {
        let mut output = CarvingMask::new(
            context.min_y + 1,
            context.min_y + context.height - 1 - if upgrading { 0 } else { 7 },
        );
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        for dx in -8..=8 {
            for dz in -8..=8 {
                let source = [chunk[0] + dx, chunk[1] + dz];
                let id = biome([source[0] * 4, 0, source[1] * 4]);
                let carvers = self
                    .biomes
                    .get(&id)
                    .ok_or_else(|| Error(format!("missing biome carvers {id}")))?;
                for (index, carver) in carvers.iter().enumerate() {
                    random.set_large_feature_seed(
                        seed.wrapping_add(index as i64),
                        source[0],
                        source[1],
                    );
                    if carver.is_start_chunk(&mut random) {
                        carver.carve(context, &mut random, chunk, source, &mut output);
                    }
                }
            }
        }
        Ok(output)
    }
}
