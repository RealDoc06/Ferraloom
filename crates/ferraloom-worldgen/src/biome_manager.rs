//! BiomeManager's seed obfuscation, Voronoi zoom and biome temperatures.
use crate::{Error, Identifier, codec::*};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::{
    LegacyRandomSource, WorldgenRandom,
    noise::{Noise, NoiseStack, SimplexNoise},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn obfuscate_seed(seed: i64) -> i64 {
    let hash = Sha256::digest(seed.to_le_bytes());
    i64::from_le_bytes(hash[..8].try_into().unwrap())
}
/// Port of BiomeManager#getBiome. Returns the selected quart coordinate.
pub fn zoom_quart(seed: i64, block: [i32; 3]) -> [i32; 3] {
    let relative = block.map(|n| n.wrapping_sub(2));
    let parent = relative.map(|n| n >> 2);
    let fract = relative.map(|n| f64::from(n & 3) / 4.0);
    let mut min = f64::INFINITY;
    let mut selected = parent;
    for i in 0..8 {
        let offset = [i >> 2 & 1, i >> 1 & 1, i & 1];
        let corner = std::array::from_fn::<_, 3, _>(|j| parent[j].wrapping_add(offset[j]));
        let distance = std::array::from_fn::<_, 3, _>(|j| fract[j] - f64::from(offset[j]));
        let mut value = seed;
        for coordinate in corner.into_iter().chain(corner) {
            value = lcg(value, i64::from(coordinate));
        }
        let x = fiddle(value);
        value = lcg(value, seed);
        let y = fiddle(value);
        value = lcg(value, seed);
        let z = fiddle(value);
        let next = (distance[2] + z) * (distance[2] + z)
            + (distance[1] + y) * (distance[1] + y)
            + (distance[0] + x) * (distance[0] + x);
        if min > next {
            min = next;
            selected = corner;
        }
    }
    selected
}
fn lcg(value: i64, add: i64) -> i64 {
    value
        .wrapping_mul(
            value
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407),
        )
        .wrapping_add(add)
}
fn fiddle(value: i64) -> f64 {
    (f64::from(((value >> 24).rem_euclid(1024)) as i32) / 1024.0 - 0.5) * 0.9
}

#[derive(Clone, Copy, Debug)]
pub struct BiomeClimate {
    pub temperature: f32,
    pub frozen: bool,
    pub has_precipitation: bool,
}
#[derive(Clone, Debug)]
pub struct BiomeTemperatures {
    pub climates: BTreeMap<Identifier, BiomeClimate>,
    temperature: SimplexNoise,
    frozen: NoiseStack,
    info: SimplexNoise,
}
impl BiomeTemperatures {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let climates = resources
            .registry("worldgen/biome")
            .into_iter()
            .map(|(id, resource)| {
                let v = resource.json()?;
                let temperature = float(field(&v, "temperature")?)?;
                let frozen = optional(&v, "temperature_modifier", false, |v| match string(v)? {
                    "none" => Ok(false),
                    "frozen" => Ok(true),
                    _ => Err(Error("unknown temperature modifier".into())),
                })?;
                Ok((
                    id,
                    BiomeClimate {
                        temperature,
                        frozen,
                        has_precipitation: boolean(field(&v, "has_precipitation")?)?,
                    },
                ))
            })
            .collect::<Result<_, Error>>()?;
        let temperature = SimplexNoise::with_discarded_offset(
            &mut WorldgenRandom::new(LegacyRandomSource::new(1234)),
            true,
        );
        let info = SimplexNoise::with_discarded_offset(
            &mut WorldgenRandom::new(LegacyRandomSource::new(2345)),
            true,
        );
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(3456));
        let mut frozen = NoiseStack::new();
        for (frequency, amplitude) in [(1.0, 0.14285715), (0.5, 0.2857143), (0.25, 0.5714286)] {
            frozen.add(
                SimplexNoise::with_discarded_offset(&mut random, true),
                frequency,
                amplitude,
            );
        }
        Ok(Self {
            climates,
            temperature,
            frozen,
            info,
        })
    }
    pub fn temperature(&self, id: &Identifier, p: [i32; 3], sea: i32) -> f32 {
        let climate = self.climates[id];
        let mut temperature = climate.temperature;
        if climate.frozen {
            let large = f64::from(
                self.frozen
                    .get_2d(f64::from(p[0]) * 0.05, f64::from(p[2]) * 0.05)
                    * 7.0,
            );
            let edge = f64::from(
                self.info
                    .get_2d(f64::from(p[0]) * 0.2, f64::from(p[2]) * 0.2),
            );
            if large + edge < 0.3
                && self
                    .info
                    .get_2d(f64::from(p[0]) * 0.09, f64::from(p[2]) * 0.09)
                    < 0.8
            {
                temperature = 0.2;
            }
        }
        let snow_level = sea + 17;
        if p[1] > snow_level {
            let v = self
                .temperature
                .get_2d(f64::from(p[0] as f32 / 8.0), f64::from(p[2] as f32 / 8.0))
                * 8.0;
            temperature - (v + p[1] as f32 - snow_level as f32) * 0.05 / 40.0
        } else {
            temperature
        }
    }
}

impl BiomeTemperatures {
    /// Port of Biome#shouldFreeze, with real fluid and block class checks.
    pub fn should_freeze(
        &self,
        id: &Identifier,
        region: &crate::region::WorldGenRegion<'_>,
        p: [i32; 3],
        check_neighbors: bool,
    ) -> Result<bool, Error> {
        use crate::region::LightLayer;
        use ferraloom_data::{BlockClass, registries::fluid};
        if self.temperature(id, p, region.sea_level) >= 0.15 {
            return Ok(false);
        }
        if p[1] < region.min_y()
            || p[1] >= region.min_y() + region.height()
            || region.brightness(LightLayer::Block, p)? >= 10
        {
            return Ok(false);
        }
        if region.fluid(p)?.fluid_type != fluid::Id::WATER
            || !region
                .block(p)?
                .block()
                .is_instance_of(BlockClass::LiquidBlock)
        {
            return Ok(false);
        }
        if !check_neighbors {
            return Ok(true);
        }
        for d in [
            ferraloom_util::Direction::West,
            ferraloom_util::Direction::East,
            ferraloom_util::Direction::North,
            ferraloom_util::Direction::South,
        ] {
            let step = d.step();
            let q = [p[0] + step.x, p[1] + step.y, p[2] + step.z];
            if !matches!(
                region.fluid(q)?.fluid_type,
                fluid::Id::WATER | fluid::Id::FLOWING_WATER
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Port of Biome#shouldSnow. Survival uses the shared game block behavior.
    pub fn should_snow(
        &self,
        id: &Identifier,
        region: &crate::region::WorldGenRegion<'_>,
        p: [i32; 3],
    ) -> Result<bool, Error> {
        use ferraloom_data::Block;
        if !self.climates[id].has_precipitation || self.temperature(id, p, region.sea_level) >= 0.15
        {
            return Ok(false);
        }
        if p[1] < region.min_y()
            || p[1] >= region.min_y() + region.height()
            || region.brightness(crate::region::LightLayer::Block, p)? >= 10
        {
            return Ok(false);
        }
        let state = region.block(p)?;
        if !state.is_air() && state.block() != Block::SNOW {
            return Ok(false);
        }
        region.can_survive(Block::SNOW.default_state(), p)
    }
}
