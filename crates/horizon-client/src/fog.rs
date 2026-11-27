//! `FogRenderer` and its fog environments (atmospheric with rain fog, water
//! with water vision, lava, powder snow, blindness), the camera fog type
//! (`Camera.getFluidInCamera`) and the fog colour (`computeFogColor`: sunrise
//! tint, sky-colour mix, void darkness, water-vision brightening).
use crate::{
    biome::Biomes,
    blocks::{BlockFacts, Fluid},
    world::World,
};
use std::path::Path;

/// `FogType` of the camera position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FogType {
    Atmospheric,
    Water,
    Lava,
    PowderSnow,
}

/// Per-biome water fog end multipliers (`visual/water_fog_end_distance`).
#[derive(Default)]
pub struct FogTables {}
impl FogTables {
    pub fn load(root: &Path, biomes: &Biomes) -> Self {
        let mut water_fog_end = vec![1.0; biomes.list.len()];
        for (name, &id) in &biomes.index {
            let path = root.join(format!(
                "data/minecraft/worldgen/biome/{}.json",
                name.trim_start_matches("minecraft:")
            ));
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let v = &json["attributes"]["minecraft:visual/water_fog_end_distance"];
            if v["modifier"].as_str() == Some("multiply")
                && let (Some(f), Some(slot)) =
                    (v["argument"].as_f64(), water_fog_end.get_mut(id as usize))
            {
                *slot = f as f32;
            }
        }
        Self {}
    }
}

/// Fog state that evolves over time.
#[derive(Clone, Copy, Debug, Default)]
pub struct FogState {
    /// `LocalPlayer.waterVisionTime` (0..600).
    pub water_vision_time: i32,
    /// `AtmosphericFogEnvironment.rainFogMultiplier`.
    pub rain_fog_multiplier: f32,
    /// `LightmapRenderStateExtractor.blockLightFlicker`.
    pub block_light_flicker: f32,
    flicker_seed: u64,
}
impl FogState {
    fn next_float(&mut self) -> f32 {
        // splitmix64: the vanilla source is unseeded `RandomSource.create()`.
        self.flicker_seed = self.flicker_seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.flicker_seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u32 << 24) as f32
    }
    /// `LightmapRenderStateExtractor.tick`: the torch light flickers.
    pub fn tick_flicker(&mut self) {
        let (a, b, c, d) = (
            self.next_float(),
            self.next_float(),
            self.next_float(),
            self.next_float(),
        );
        self.block_light_flicker += (a - b) * c * d * 0.1;
        self.block_light_flicker *= 0.9;
    }
    /// `LocalPlayer.aiStep`: water vision ramps up while the eye is in water.
    pub fn tick(&mut self, eye_in_water: bool) {
        self.water_vision_time = if eye_in_water {
            (self.water_vision_time + 1).min(600)
        } else {
            (self.water_vision_time - 10).max(0)
        };
    }
}

/// `FluidState.getHeightForCamera`.
fn camera_fluid_height(world: &World, facts: &BlockFacts, b: [i32; 3]) -> Option<(Fluid, f64)> {
    let s = facts.get(world.block(b[0], b[1], b[2]));
    if s.fluid == Fluid::None {
        return None;
    }
    let above = facts.get(world.block(b[0], b[1] + 1, b[2]));
    let source = s.fluid_level >= 8 && !s.fluid_falling;
    let height = if (source && above.full_collision) || above.fluid == s.fluid {
        1.0
    } else {
        f64::from(s.fluid_level) / 9.0
    };
    Some((s.fluid, height))
}
/// `Camera.getFluidInCamera` (near-plane points reduced to the camera and
/// the point just in front of it).
pub fn fog_type(world: &World, facts: &BlockFacts, camera: [f64; 3], forward: [f64; 3]) -> FogType {
    let block = |p: [f64; 3]| {
        [
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        ]
    };
    let b = block(camera);
    if let Some((Fluid::Water, h)) = camera_fluid_height(world, facts, b)
        && camera[1] < f64::from(b[1]) + h
    {
        return FogType::Water;
    }
    for p in [
        [
            camera[0] + forward[0] * 0.05,
            camera[1] + forward[1] * 0.05,
            camera[2] + forward[2] * 0.05,
        ],
        camera,
    ] {
        let b = block(p);
        match camera_fluid_height(world, facts, b) {
            Some((Fluid::Lava, h)) if p[1] <= f64::from(b[1]) + h => return FogType::Lava,
            Some((Fluid::Lava, _)) => {}
            _ => {
                if &*facts.get(world.block(b[0], b[1], b[2])).name == "minecraft:powder_snow" {
                    return FogType::PowderSnow;
                }
            }
        }
    }
    FogType::Atmospheric
}

/// `AtmosphericFogEnvironment.updateRainFogState`.
pub fn update_rain_fog(
    state: &mut FogState,
    rain: f32,
    sky_light: u8,
    biome_rains: bool,
    delta_ticks: f32,
) {
    let sky = ((f32::from(sky_light) - 8.0) / 7.0).clamp(0.0, 1.0);
    let target = rain * sky * if biome_rains { 1.0 } else { 0.5 };
    state.rain_fog_multiplier += (target - state.rain_fog_multiplier) * delta_ticks * 0.2;
}
