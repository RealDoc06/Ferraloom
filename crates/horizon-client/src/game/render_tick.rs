//! Per-tick client visuals: the particle engine, `ClientLevel.animateTick`,
//! campfire smoke and rain splashes (`ClientLevel.tickWeatherEffects`).
use super::{Dim, Game};
use horizon_client::fog::{FogType, fog_type, update_rain_fog};
use horizon_client::screen_effects::lightmap_brightness;
use horizon_client::{
    animate_tick::{AnimateContext, animate_tick, marker_particle_target},
    biome::Biomes,
    blocks::{BlockFacts, Fluid},
    chunk::{HEIGHT, MIN_Y},
    particles::{ParticleLevel, ParticleOptions, ParticleType},
    world::World,
};
use horizon_core::random::{LegacyRandom, RandomSource};

/// The first motion-blocking block at or below `from` (Heightmap MOTION_BLOCKING), plus one.
fn motion_blocking(world: &World, facts: &BlockFacts, x: i32, z: i32, from: i32) -> i32 {
    motion_blocking_leaves(world, facts, x, z, from, true)
}
/// `motion_blocking` where leaves count only for MOTION_BLOCKING, not for
/// MOTION_BLOCKING_NO_LEAVES.
fn motion_blocking_leaves(
    world: &World,
    facts: &BlockFacts,
    x: i32,
    z: i32,
    from: i32,
    leaves: bool,
) -> i32 {
    let mut y = from.min(MIN_Y + HEIGHT - 1);
    while y > MIN_Y {
        let f = facts.get(world.block(x, y, z));
        // #minecraft:leaves block motion, but MOTION_BLOCKING_NO_LEAVES skips them
        // (their water still counts).
        let blocks = if f.name.ends_with("_leaves") {
            leaves
        } else {
            f.full_collision
        };
        if blocks || f.fluid != Fluid::None {
            return y + 1;
        }
        y -= 1;
    }
    MIN_Y
}
/// `Biome.getPrecipitationAt` is rain (not snow) at this position.
fn rains_at(world: &World, biomes: &Biomes, x: i32, y: i32, z: i32) -> bool {
    let Some(chunk) = world.chunks.get(&(x >> 4, z >> 4)) else {
        return false;
    };
    biomes.get(chunk.biome(x, y, z)).snows_at(x, y, z) == Some(false)
}

impl Game<'_> {
    pub(super) fn forward(&self) -> [f64; 3] {
        let (yaw, pitch) = (
            f64::from(self.yaw).to_radians(),
            f64::from(self.pitch).to_radians(),
        );
        [
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        ]
    }
    /// `Lightmap.getBrightness(getMaxLocalRawBrightness(eye))`.
    fn eye_brightness(&self, eye: [f64; 3]) -> f32 {
        let b = [
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        ];
        let packed = self.world.light(b[0], b[1], b[2]);
        let sky = i32::from(packed >> 4) - self.sky_darken();
        let raw = sky.max(i32::from(packed & 15)).clamp(0, 15) as u8;
        lightmap_brightness(if self.dim == Dim::Nether { 0.1 } else { 0.0 }, raw)
    }
    pub(super) fn tick_particles(&mut self) {
        self.fog.tick_flicker();
        let eye = self.eye_position();
        if let Some(e) = eye {
            let brightness = self.eye_brightness(e);
            let inside_portal = self.client_portal_effect();
            self.screen_fx.tick(brightness, inside_portal);
            let nausea = self
                .potions
                .effects
                .get("nausea")
                .map(|e| !e.ends_within(60));
            self.screen_fx.tick_spinning(nausea);
            // LocalPlayer water vision and AtmosphericFogEnvironment rain fog.
            let in_water =
                fog_type(&self.world, self.res.facts(), e, self.forward()) == FogType::Water;
            self.fog.tick(in_water);
            let b = [
                e[0].floor() as i32,
                e[1].floor() as i32,
                e[2].floor() as i32,
            ];
            let rain = if self.dim == Dim::Overworld {
                self.world.weather.levels(1.0).0
            } else {
                0.0
            };
            let biome = self
                .world
                .chunks
                .get(&(b[0] >> 4, b[2] >> 4))
                .map_or(0, |c| c.biome(b[0], b[1], b[2]));
            let rains = self.res.biomes().get(biome).precipitation;
            update_rain_fog(
                &mut self.fog,
                rain,
                self.world.light(b[0], b[1], b[2]) >> 4,
                rains,
                1.0,
            );
        }
        self.particles.particle_level = self.settings.particles;
        if let Some(e) = eye {
            self.particles.camera = e;
        }
        let (rain, _) = if self.dim == Dim::Overworld {
            self.world.weather.levels(1.0)
        } else {
            (0.0, 0.0)
        };
        let marker = marker_particle_target(self.mode, self.stack_in(self.selected));
        let marker_options = |state| {
            // BlockMarker: the particle sprite, untinted.
            let sprite = Self::block_particle_sprite(&self.pack, state)?;
            Some(ParticleOptions::block(
                ParticleType::BlockMarker,
                sprite,
                0xFFFFFF,
            ))
        };
        let world = &self.world;
        let facts = self.res.facts();
        let biomes = self.res.biomes();
        let raining = rain > 0.2;
        let raining_at = |p: [i32; 3]| {
            raining
                && motion_blocking(world, facts, p[0], p[2], p[1] + 64) <= p[1]
                && rains_at(world, biomes, p[0], p[1], p[2])
        };
        let height_no_leaves =
            |p: [i32; 3]| motion_blocking_leaves(world, facts, p[0], p[2], i32::MAX, false);
        // The day timeline: firefly bush sounds from 12600 to 23400.
        let firefly_bush_sounds = self.dim == Dim::Overworld
            && (12600..23401).contains(&self.world.day_time.rem_euclid(24000));
        let tables = &self.pack.context.tables;
        let model_offset = |state, x, z| tables.offset(state, x, z);
        let gateway_particles = |state, p| {
            self.res
                .block_entity_render()
                .gateway_faces(world, state, p)
                .len()
        };
        // Level.isThundering: a thunder level above 0.9 where the sky is open.
        let thundering = self.dim == Dim::Overworld && world.weather.levels(1.0).1 > 0.9;
        let ctx = AnimateContext {
            world,
            facts,
            collision: self.collision,
            biomes,
            registry: self.pack.assets.registry,
            raining_at: &raining_at,
            height_no_leaves: &height_no_leaves,
            firefly_bush_sounds,
            sky_darken: self.sky_darken(),
            marker: marker.map(|name| (name, &marker_options as &dyn Fn(_) -> _)),
            model_offset: &model_offset,
            gateway_particles: &gateway_particles,
            thundering,
            ultrawarm: self.dim == Dim::Nether,
        };
        if let Some(e) = eye {
            let block = [
                e[0].floor() as i32,
                e[1].floor() as i32,
                e[2].floor() as i32,
            ];
            animate_tick(self.res.animate(), &mut self.particles, &ctx, block);
            // The Horizon Weather option off means no rain visuals or sounds at all.
            if rain > 0.0 && self.settings.horizon.weather {
                // ClientLevel.tickWeatherEffects: rain splashes within the weather
                // radius, and the rain sound.
                let mut random = LegacyRandom::new(self.world.game_time.wrapping_mul(312_987_231));
                let radius = self.settings.weather_radius;
                let diameter = 2 * radius + 1;
                let count = (0.225 * (diameter * diameter) as f32 * rain * rain) as i32
                    / if self.settings.particles == 1 { 2 } else { 1 };
                let mut rain_position = None;
                for _ in 0..count {
                    let x = block[0] + random.next_int_bound(diameter).unwrap_or(0) - radius;
                    let z = block[2] + random.next_int_bound(diameter).unwrap_or(0) - radius;
                    let top = motion_blocking(world, facts, x, z, block[1] + 64);
                    if top <= MIN_Y
                        || top > block[1] + 10
                        || top < block[1] - 10
                        || !rains_at(world, biomes, x, top, z)
                    {
                        continue;
                    }
                    let below = top - 1;
                    rain_position = Some([x, below, z]);
                    // The Minimal option stops at the first spot that rains (the
                    // rain sound still plays there).
                    if self.settings.particles >= 2 {
                        break;
                    }
                    let (bx, bz) = (random.next_double(), random.next_double());
                    let state = world.block(x, below, z);
                    let f = facts.get(state);
                    let block_top = match self.collision.cached_shapes().cached_shape(state) {
                        Ok(Some(shape)) if !shape.is_empty() => {
                            shape.max_at(horizon_core::geometry::Axis::Y, bx, bz)
                        }
                        _ => 0.0,
                    };
                    let fluid_top = if f.fluid == Fluid::None {
                        0.0
                    } else {
                        f64::from(f.fluid_level) / 9.0
                    };
                    let smoke = f.fluid == Fluid::Lava
                        || &*f.name == "minecraft:magma_block"
                        || (f.name.ends_with("campfire")
                            && self.res.animate().is_lit_campfire(state.0));
                    let ty = if smoke {
                        ParticleType::Smoke
                    } else {
                        ParticleType::Rain
                    };
                    let p = [
                        f64::from(x) + bx,
                        f64::from(below) + block_top.max(fluid_top),
                        f64::from(z) + bz,
                    ];
                    self.particles
                        .add(&ParticleOptions::simple(ty), p, [0.0; 3]);
                }
                // The last rain column of the tick makes the sound, more often the
                // longer it has been quiet; indoors it is the quieter one.
                if let Some(at) = rain_position {
                    let waited = self.rain_sound_time;
                    self.rain_sound_time += 1;
                    if random.next_int_bound(3).unwrap_or(0) < waited {
                        self.rain_sound_time = 0;
                        let roof =
                            motion_blocking(world, facts, block[0], block[2], MIN_Y + HEIGHT);
                        let (name, volume, pitch) = if at[1] > block[1] + 1 && roof > block[1] {
                            ("minecraft:weather.rain.above", 0.1, 0.5)
                        } else {
                            ("minecraft:weather.rain", 0.2, 1.0)
                        };
                        let p = at.map(|c| f64::from(c) + 0.5);
                        // playLocalSound: guests make their own rain, so a host
                        // must not capture it for them.
                        self.audio.play_unshared(name, Some(p), volume, pitch);
                    }
                }
            }
        }
        let level = ParticleLevel {
            world,
            facts,
            collision: self.collision,
            player: self
                .player
                .as_ref()
                .map(|p| ([p.position.x, p.position.y, p.position.z], p.velocity.y)),
        };
        self.particles.tick(&level);
        for (name, at, volume, pitch) in std::mem::take(&mut self.particles.sounds) {
            self.audio.play(name, Some(at), volume, pitch);
        }
    }
}
