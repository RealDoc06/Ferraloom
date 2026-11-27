//! The worlds area: time, weather, spawn, game rules, difficulty, chunks
//! and their tickets, the border, sounds, level events, explosions and
//! lightning; the chunk, weather and save events.
use super::{DIMS, GameHost, world_index};
use crate::game::{Dim, Game, explosion::BlockInteraction};
use ferraloom_plugins::{Border, Change, Event, Query, Sound, WorldHost, listens};
use horizon_client::gamerules::{self, Value};
use std::{cell::RefCell, collections::HashMap};

/// The loaded chunks' membership revision and keys, before a streaming update.
pub(in crate::game) type LoadedBefore = (u64, std::collections::HashSet<(i32, i32)>);

thread_local! {
    /// The plugin chunk tickets on each chunk, and whether the ticket made
    /// it a forced chunk (`/forceload` may hold it already): a chunk with a
    /// ticket stays loaded.
    static TICKETS: RefCell<HashMap<(Dim, i32, i32), (u32, bool)>> = RefCell::new(HashMap::new());
}

/// `dim`'s world, in place or parked, to read; none when its level is not loaded.
fn level_world<'g>(game: &'g Game, dim: Dim) -> Option<&'g horizon_client::world::World> {
    if dim == game.dim {
        Some(&game.world)
    } else {
        game.levels.parked_world(dim)
    }
}

/// `WORLD_SURFACE` and the `HeightMap` ordinals that read it; the others read `MOTION_BLOCKING`.
fn reads_world_surface(heightmap: i32) -> bool {
    matches!(heightmap, 4 | 5)
}

impl GameHost<'_, '_> {
    /// Adds or removes a plugin's chunk ticket; true when it changed anything.
    fn ticket(&mut self, dim: Dim, x: i32, z: i32, add: bool) -> bool {
        let forced = if dim == self.game.dim {
            Some(&mut self.game.streaming.forced)
        } else {
            self.game.levels.parked_forced(dim)
        };
        let Some(forced) = forced else {
            return false;
        };
        let ranked = TICKETS.with_borrow_mut(|tickets| {
            let entry = tickets.entry((dim, x, z)).or_insert((0, false));
            if add {
                entry.0 += 1;
                if entry.0 == 1 {
                    entry.1 = forced.insert((x, z));
                }
                return entry.1;
            }
            if entry.0 == 0 {
                return false;
            }
            entry.0 -= 1;
            if entry.0 == 0 {
                if entry.1 {
                    forced.remove(&(x, z));
                }
                tickets.remove(&(dim, x, z));
            }
            false
        });
        if ranked && dim == self.game.dim {
            self.game.streaming.rank_forced(x, z, x, z);
        }
        true
    }

    /// Whether a chunk was ever generated: loaded, or in its region file.
    /// A level that is not loaded answers false.
    fn generated(&mut self, dim: Dim, x: i32, z: i32) -> bool {
        if level_world(self.game, dim).is_some_and(|w| w.chunks.contains_key(&(x, z))) {
            return true;
        }
        let root = if dim == self.game.dim {
            self.game.save.root.clone()
        } else {
            match self.game.levels.parked_save_root(dim) {
                Some(root) => root,
                None => return false,
            }
        };
        let path = root
            .join("region")
            .join(format!("r.{}.{}.mca", x >> 5, z >> 5));
        horizon_world::region::RegionFile::open(path)
            .is_ok_and(|region| region.chunks().any(|(pos, _)| pos.x == x && pos.z == z))
    }
}

impl WorldHost for GameHost<'_, '_> {
    /// CraftWorld.getFullTime and getTime: the day time (every level reads the
    /// server's), whole or within its day.
    fn world_time(&mut self, _world: usize, full: bool) -> i64 {
        let day = self.game.world.day_time;
        if full { day } else { day.rem_euclid(24_000) }
    }

    fn query(&mut self, world: usize, query: Query) -> Option<i64> {
        let dim = *DIMS.get(world)?;
        let weather = self.game.world.weather;
        let spawn = self.game.level.world_spawn;
        Some(match query {
            Query::GameTime => self.game.world.game_time,
            Query::FullTime => self.game.world.day_time,
            Query::Time => self.game.world.day_time.rem_euclid(24_000),
            Query::Storm => i64::from(weather.raining),
            Query::Thundering => i64::from(weather.thundering),
            Query::RainTime => i64::from(weather.rain_time),
            Query::ThunderTime => i64::from(weather.thunder_time),
            Query::ClearTime => i64::from(weather.clear_time),
            Query::SpawnX => i64::from(spawn?[0]),
            Query::SpawnY => i64::from(spawn?[1]),
            Query::SpawnZ => i64::from(spawn?[2]),
            Query::SeaLevel => {
                if dim == Dim::Nether {
                    32
                } else {
                    63
                }
            }
            Query::Highest { x, z, heightmap } => {
                let facts = self.game.res.facts();
                let w = level_world(self.game, dim)?;
                w.chunks.get(&(x >> 4, z >> 4))?;
                // CraftWorld.getHighestBlockYAt: the heightmap's height less one.
                let height = if reads_world_surface(heightmap) {
                    w.world_surface(x, z, |s| facts.is_air(s))
                } else {
                    w.motion_blocking(x, z, |s| {
                        let f = facts.get(s);
                        f.legacy_solid || f.fluid_level > 0
                    })
                };
                i64::from(height - 1)
            }
            Query::ChunkLoaded { x, z } => {
                i64::from(level_world(self.game, dim)?.chunks.contains_key(&(x, z)))
            }
            Query::ChunkGenerated { x, z } => i64::from(self.generated(dim, x, z)),
            Query::Difficulty => i64::from(self.game.ui.difficulty),
            Query::Seed => self.game.level.seed,
        })
    }

    fn change(&mut self, world: usize, change: Change) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        let weather = self.game.world.weather;
        match change {
            // CraftWorld.setFullTime: the day time.
            Change::FullTime(t) => self.game.world.day_time = t,
            // CraftWorld.setTime: forward to that time of the day.
            Change::Time(t) => {
                let day = self.game.world.day_time;
                self.game.world.day_time = day + (t - day).rem_euclid(24_000);
            }
            // CraftWorld.setStorm and setThundering: the event first
            // (PrimaryLevelData.setRaining), then the durations start over.
            Change::Storm(on) => {
                if weather.raining != on
                    && listens(Event::WeatherChange)
                    && ferraloom_plugins::weather_event(self, world, false, on)
                {
                    return false;
                }
                let w = &mut self.game.world.weather;
                w.raining = on;
                w.rain_time = 0;
                w.clear_time = 0;
            }
            Change::Thundering(on) => {
                if weather.thundering != on
                    && listens(Event::ThunderChange)
                    && ferraloom_plugins::weather_event(self, world, true, on)
                {
                    return false;
                }
                let w = &mut self.game.world.weather;
                w.thundering = on;
                w.thunder_time = 0;
                w.clear_time = 0;
            }
            Change::RainTime(t) => self.game.world.weather.rain_time = t,
            Change::ThunderTime(t) => self.game.world.weather.thunder_time = t,
            Change::ClearTime(t) => self.game.world.weather.clear_time = t,
            // LevelData.setRespawnData, as /setworldspawn sets it.
            Change::Spawn { pos, angle } => {
                self.game.level.world_spawn = Some(pos);
                self.game.level.world_spawn_angle = [angle, 0.0];
            }
            Change::Difficulty(d) => {
                let Some(d) = u8::try_from(d).ok().filter(|d| *d <= 3) else {
                    return false;
                };
                self.game.ui.difficulty = d;
                self.game.sync_rules();
                if d == 0 {
                    // Peaceful removes hostile mobs.
                    self.game.mobs.retain(|m| !m.hostile());
                }
            }
            // The chunk is asked for: a ticket the bridge holds until `unloadChunk`.
            Change::LoadChunk { x, z, .. } | Change::Ticket { x, z, add: true } => {
                return self.ticket(dim, x, z, true);
            }
            Change::UnloadChunk { x, z } | Change::Ticket { x, z, add: false } => {
                return self.ticket(dim, x, z, false);
            }
            Change::BorderCenter(center) => self.game.border.set_center(center),
            Change::BorderSize { size, ticks } => {
                if ticks > 0 {
                    self.game.border.lerp_to(size, ticks);
                } else {
                    self.game.border.set_size(size);
                }
            }
            Change::BorderDamageBuffer(v) => self.game.border.safe_zone = v,
            Change::BorderDamageAmount(v) => self.game.border.damage_per_block = v,
            Change::BorderWarningDistance(v) => self.game.border.warning_distance = v,
            Change::BorderWarningTime(v) => self.game.border.warning_time = v,
            Change::Save => return self.game.save_game().is_ok(),
        }
        true
    }

    fn game_rule(&mut self, name: &str, default: bool) -> Option<String> {
        let rule = gamerules::rule(name)?;
        let value = if default {
            rule.default
        } else {
            self.game.ui.rules.get(rule.name).unwrap_or(rule.default)
        };
        Some(value.to_string())
    }

    fn set_game_rule(&mut self, name: &str, value: &str) -> bool {
        let Some(rule) = gamerules::rule(name) else {
            return false;
        };
        let value = match rule.default {
            Value::Bool(_) => match value {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return false,
            },
            Value::Int(_) => match value.parse::<i32>() {
                Ok(v) if (rule.min..=rule.max).contains(&v) => Value::Int(v),
                _ => return false,
            },
        };
        self.game.ui.rules.set(rule.name, value);
        self.game.sync_rules();
        true
    }

    fn game_rules(&mut self) -> Vec<String> {
        gamerules::RULES.iter().map(|r| r.name.to_owned()).collect()
    }

    fn loaded_chunks(&mut self, world: usize) -> Vec<[i32; 2]> {
        let Some(w) = DIMS.get(world).and_then(|&d| level_world(self.game, d)) else {
            return Vec::new();
        };
        w.chunks.keys().map(|&(x, z)| [x, z]).collect()
    }

    fn border(&mut self, _world: usize) -> Border {
        let b = &self.game.border;
        Border {
            center: b.center(),
            size: b.size(),
            damage_buffer: b.safe_zone,
            damage_amount: b.damage_per_block,
            warning_distance: b.warning_distance,
            warning_time: b.warning_time,
        }
    }

    /// ServerLevel.playSeededSound: every player of the level within the
    /// sound's range (16 blocks, more when louder) hears it.
    fn play_sound(&mut self, world: usize, sound: &Sound) {
        let Some(&dim) = DIMS.get(world) else { return };
        let range = 16.0 * f64::from(sound.volume.max(1.0));
        let Some(hub) = self.hub() else { return };
        let packet = hub.hub.sound_packet(
            sound.name,
            sound.at,
            sound.volume,
            sound.pitch,
            sound.category,
            sound.seed,
        );
        let hearing: Vec<u32> = hub
            .hub
            .guests
            .iter()
            .filter(|g| g.placed && Dim::from_id(&g.dimension) == dim)
            .filter(|g| {
                let d = [
                    g.position[0] - sound.at[0],
                    g.position[1] - sound.at[1],
                    g.position[2] - sound.at[2],
                ];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < range * range
            })
            .map(|g| g.conn)
            .collect();
        for conn in hearing {
            hub.hub.send_to(conn, &packet);
        }
    }

    fn level_event(&mut self, world: usize, pos: [i32; 3], id: i32, data: i32) {
        let Some(&dim) = DIMS.get(world) else { return };
        self.game.in_level(dim, |g| g.level_event_at(id, pos, data));
    }

    /// CraftWorld.createExplosion with no source: a block explosion
    /// (BlockExplodeEvent) with the `block_explosion_drop_decay` rule.
    fn explode(
        &mut self,
        world: usize,
        at: [f64; 3],
        power: f32,
        fire: bool,
        breaks: bool,
    ) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        let interaction = if !breaks {
            BlockInteraction::Keep
        } else if self.game.ui.rules.bool("block_explosion_drop_decay") {
            BlockInteraction::DestroyWithDecay
        } else {
            BlockInteraction::Destroy
        };
        let exploded = self.game.in_level(dim, |g| {
            super::blocks::as_block_explosion(|| g.server_explode(at, power, interaction, fire))
        });
        matches!(exploded, Some(Ok(())))
    }

    fn strike_lightning(&mut self, world: usize, at: [f64; 3], effect: bool) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        self.game
            .in_level(dim, |g| g.spawn_bolt(at, effect))
            .is_some()
    }
}

impl Game<'_> {
    /// Chunks the streaming update just unloaded: ChunkUnloadEvent for each.
    pub(in crate::game) fn plugin_chunks_unloaded(
        &mut self,
        unloaded: &[std::sync::Arc<horizon_client::chunk::Chunk>],
    ) {
        let world = world_index(self.dim);
        for chunk in unloaded {
            let mut host = GameHost::new(self, None);
            ferraloom_plugins::chunk_event(&mut host, world, [chunk.x, chunk.z], false, false);
        }
    }

    /// The chunks loaded before a streaming update, to find those it loads
    /// (only while a plugin listens to ChunkLoadEvent: a copy of the keys).
    pub(in crate::game) fn plugin_loaded_before(&self) -> Option<LoadedBefore> {
        let chunks = &self.world.chunks;
        Some((
            chunks.membership_revision(),
            chunks.keys().copied().collect(),
        ))
    }

    /// ChunkLoadEvent for each chunk the streaming update loaded (a new
    /// chunk is one generated just now).
    pub(in crate::game) fn plugin_chunks_loaded(&mut self, (revision, before): LoadedBefore) {
        let chunks = &self.world.chunks;
        if chunks.membership_revision() == revision {
            return;
        }
        let world = world_index(self.dim);
        let new: Vec<((i32, i32), bool)> = chunks
            .iter()
            .filter(|(k, _)| !before.contains(*k))
            .map(|(k, c)| (*k, c.fresh))
            .collect();
        for ((x, z), fresh) in new {
            let mut host = GameHost::new(self, None);
            ferraloom_plugins::chunk_event(&mut host, world, [x, z], true, fresh);
        }
    }

    /// ServerLevel.save's WorldSaveEvent, before the level in place saves.
    pub(in crate::game) fn plugin_world_save(&mut self) {
        let world = world_index(self.dim);
        ferraloom_plugins::save_event(&mut GameHost::new(self, None), world);
    }

    /// The weather cycle's step (`advanceWeatherCycle`), with
    /// WeatherChangeEvent and ThunderChangeEvent: a cancelled turn of the
    /// rain or the thunder is undone (PrimaryLevelData.setRaining returns
    /// before it changes).
    pub(in crate::game) fn plugin_tick_weather(&mut self) {
        let before = self.world.weather;
        self.world.weather.tick_timers(&mut self.world.random);
        let after = self.world.weather;
        let world = world_index(self.dim);
        if after.raining != before.raining && listens(Event::WeatherChange) {
            let mut host = GameHost::new(self, None);
            if ferraloom_plugins::weather_event(&mut host, world, false, after.raining) {
                self.world.weather.raining = before.raining;
            }
        }
        if after.thundering != before.thundering && listens(Event::ThunderChange) {
            let mut host = GameHost::new(self, None);
            if ferraloom_plugins::weather_event(&mut host, world, true, after.thundering) {
                self.world.weather.thundering = before.thundering;
            }
        }
    }
}
