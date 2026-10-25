//! Ports of WeatherData, ServerLevel#advanceWeatherCycle and prepareWeather.
use super::{Level, LevelError, effects::LevelEffect};
use ferraloom_light::{DataLayer, LightLayer};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_storage::{Error, Result as StorageResult, SavedDataCodec};
use ferraloom_util::{BlockPos, ChunkPos};
use ferraloom_worldgen::{
    Identifier,
    biome_manager::{BiomeTemperatures, Precipitation, zoom_quart},
};
use std::{collections::BTreeMap, sync::Arc};

/// Dimension-owned biome inputs; climate math is shared with worldgen.
pub struct WeatherEnvironment {
    pub temperatures: Arc<BiomeTemperatures>,
    pub biome_names: Arc<Vec<Identifier>>,
    pub biome_zoom_seed: i64,
    pub sea_level: i32,
}
/// Visible light data copied at the native light-result integration point.
/// Missing sky sections inherit the bottom plane of the next stored section,
/// exactly as SkyLightSectionStorage#getLightValue(false) does.
#[derive(Clone, Default)]
pub struct ChunkLightSnapshot {
    pub sky: BTreeMap<i32, DataLayer>,
    pub block: BTreeMap<i32, DataLayer>,
}
impl ChunkLightSnapshot {
    fn brightness(&self, layer: LightLayer, pos: BlockPos) -> u8 {
        let section = pos.y >> 4;
        let layers = match layer {
            LightLayer::Sky => &self.sky,
            LightLayer::Block => &self.block,
        };
        if let Some(data) = layers.get(&section) {
            return data.get((pos.x & 15) as u8, (pos.y & 15) as u8, (pos.z & 15) as u8);
        }
        if layer == LightLayer::Block {
            return 0;
        }
        layers
            .range((section + 1)..)
            .next()
            .map_or(15, |(_, data)| {
                data.get((pos.x & 15) as u8, 0, (pos.z & 15) as u8)
            })
    }
}
use ferraloom_util::random::{Random, RandomSource};

/// Shared server weather save, stored in data/minecraft/weather.dat in 26.3.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeatherData {
    pub clear_weather_time: i32,
    pub rain_time: i32,
    pub thunder_time: i32,
    pub raining: bool,
    pub thundering: bool,
}
impl SavedDataCodec for WeatherData {
    fn decode(tag: &Tag) -> StorageResult<Self> {
        let compound = tag
            .as_compound()
            .ok_or_else(|| Error::Invalid("weather data must be a compound".into()))?;
        let int = |key| {
            compound
                .get_int(key)
                .ok_or_else(|| Error::Invalid(format!("weather data requires integer {key}")))
        };
        let boolean = |key| {
            compound
                .get_boolean(key)
                .ok_or_else(|| Error::Invalid(format!("weather data requires boolean {key}")))
        };
        Ok(Self {
            clear_weather_time: int("clear_weather_time")?,
            rain_time: int("rain_time")?,
            thunder_time: int("thunder_time")?,
            raining: boolean("raining")?,
            thundering: boolean("thundering")?,
        })
    }
    fn encode(&self) -> StorageResult<Tag> {
        let mut tag = CompoundTag::new();
        tag.insert("clear_weather_time", Tag::Int(self.clear_weather_time));
        tag.insert("rain_time", Tag::Int(self.rain_time));
        tag.insert("thunder_time", Tag::Int(self.thunder_time));
        tag.insert("raining", Tag::Byte(i8::from(self.raining)));
        tag.insert("thundering", Tag::Byte(i8::from(self.thundering)));
        Ok(Tag::Compound(tag))
    }
}
#[derive(Default)]
pub(super) struct Weather {
    old_rain: f32,
    rain: f32,
    old_thunder: f32,
    thunder: f32,
    pending_precipitation_calls: u64,
    environment: Option<WeatherEnvironment>,
    lights: super::store::SnapshotMap<ChunkPos, ChunkLightSnapshot>,
}
impl Weather {
    fn from_saved(data: WeatherData) -> Self {
        Self {
            rain: if data.raining { 1.0 } else { 0.0 },
            thunder: if data.raining && data.thundering {
                1.0
            } else {
                0.0
            },
            ..Self::default()
        }
    }
    fn advance(
        &mut self,
        data: &mut WeatherData,
        random: &mut Random,
        advance_weather: bool,
        can_have_weather: bool,
    ) -> Vec<LevelEffect> {
        let was_raining = self.is_raining(can_have_weather);
        if can_have_weather {
            if advance_weather {
                if data.clear_weather_time > 0 {
                    data.clear_weather_time -= 1;
                    data.thunder_time = i32::from(!data.thundering);
                    data.rain_time = i32::from(!data.raining);
                    data.thundering = false;
                    data.raining = false;
                } else {
                    if data.thunder_time > 0 {
                        data.thunder_time -= 1;
                        if data.thunder_time == 0 {
                            data.thundering = !data.thundering;
                        }
                    } else {
                        data.thunder_time = if data.thundering {
                            random.next_int_between_inclusive(3600, 15600)
                        } else {
                            random.next_int_between_inclusive(12000, 180000)
                        };
                    }
                    if data.rain_time > 0 {
                        data.rain_time -= 1;
                        if data.rain_time == 0 {
                            data.raining = !data.raining;
                        }
                    } else {
                        data.rain_time = if data.raining {
                            random.next_int_between_inclusive(12000, 24000)
                        } else {
                            random.next_int_between_inclusive(12000, 180000)
                        };
                    }
                }
            }
            self.old_thunder = self.thunder;
            self.thunder =
                (self.thunder + if data.thundering { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
            self.old_rain = self.rain;
            self.rain = (self.rain + if data.raining { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
        }
        let mut events = Vec::new();
        if self.old_rain != self.rain {
            events.push(LevelEffect::WeatherEvent {
                event: 7,
                param: self.rain,
            });
        }
        if self.old_thunder != self.thunder {
            events.push(LevelEffect::WeatherEvent {
                event: 8,
                param: self.thunder,
            });
        }
        if was_raining != self.is_raining(can_have_weather) {
            events.push(LevelEffect::WeatherEvent {
                event: if was_raining { 2 } else { 1 },
                param: 0.0,
            });
            events.push(LevelEffect::WeatherEvent {
                event: 7,
                param: self.rain,
            });
            events.push(LevelEffect::WeatherEvent {
                event: 8,
                param: self.thunder,
            });
        }
        events
    }
    fn is_raining(&self, can_have_weather: bool) -> bool {
        can_have_weather && f64::from(self.rain) > 0.2
    }
    fn is_thundering(&self, can_have_weather: bool) -> bool {
        can_have_weather && f64::from(self.thunder * self.rain) > 0.9
    }
}
impl Level {
    /// Port of ServerLevel#prepareWeather: only per-level interpolation is
    /// initialized from the one server-owned WeatherData.
    pub fn prepare_weather(&mut self, data: &WeatherData) {
        let weather = &mut self.region_mut().weather;
        let environment = weather.environment.take();
        let lights = std::mem::take(&mut weather.lights);
        *weather = Weather::from_saved(*data);
        weather.environment = environment;
        weather.lights = lights;
    }
    /// Install loaded canonical weather and prepare this level's interpolation.
    pub fn load_weather(&mut self, data: WeatherData) -> Result<(), LevelError> {
        *self.weather_mut()? = data;
        self.prepare_weather(&data);
        Ok(())
    }
    pub fn weather(&self) -> Result<&WeatherData, LevelError> {
        Ok(&self.shared()?.weather)
    }
    pub fn weather_mut(&mut self) -> Result<&mut WeatherData, LevelError> {
        Ok(&mut self.shared_mut()?.weather)
    }
    /// Port of MinecraftServer#setWeatherParameters. A command changes targets;
    /// levels smooth toward those targets in their next weather tick.
    pub fn set_weather(
        &mut self,
        clear_time: i32,
        rain_time: i32,
        raining: bool,
        thundering: bool,
    ) -> Result<(), LevelError> {
        *self.weather_mut()? = WeatherData {
            clear_weather_time: clear_time,
            rain_time,
            thunder_time: rain_time,
            raining,
            thundering,
        };
        Ok(())
    }
    /// Port of ServerLevel#resetWeatherCycle, after the caller's sleep/rule guards.
    pub fn reset_weather_cycle(&mut self) -> Result<(), LevelError> {
        let data = self.weather_mut()?;
        data.rain_time = 0;
        data.raining = false;
        data.thunder_time = 0;
        data.thundering = false;
        Ok(())
    }
    /// Port of ServerLevel#advanceWeatherCycle at each level's native slot.
    /// The shared data and local interpolation/RNG have disjoint owners; no
    /// shared-state copy survives a phase or gets written back after callbacks.
    pub fn advance_weather_cycle(&mut self, advance_weather: bool) -> Result<(), LevelError> {
        let shared = self
            .shared
            .as_mut()
            .ok_or(LevelError::MissingServerShared)?;
        let region = self.region.as_mut().expect("level region is attached");
        let events = region.weather.advance(
            &mut shared.weather,
            &mut region.random,
            advance_weather,
            region.rules.can_have_weather,
        );
        region.effects.extend(events);
        Ok(())
    }
    pub fn is_raining(&self) -> bool {
        self.region()
            .weather
            .is_raining(self.rules().can_have_weather)
    }
    pub fn set_weather_environment(&mut self, environment: WeatherEnvironment) {
        self.region_mut().weather.environment = Some(environment);
    }
    /// Exclusive imports retire obsolete snapshots immediately. Managed imports
    /// use the same store and retain versions until an exclusive boundary.
    pub fn set_light_snapshot(&mut self, pos: ChunkPos, snapshot: ChunkLightSnapshot) {
        self.publish_light_snapshot(pos, snapshot);
        self.prune_light_snapshots();
    }
    pub fn publish_light_snapshot(&self, pos: ChunkPos, snapshot: ChunkLightSnapshot) {
        self.region().weather.lights.insert(pos, snapshot);
    }
    pub(super) fn prune_light_snapshots(&mut self) {
        self.region_mut().weather.lights.prune();
    }
    pub(super) fn remove_light_snapshot(&mut self, pos: ChunkPos) {
        self.region_mut().weather.lights.remove(&pos);
    }
    pub(super) fn weather_environment(
        &self,
    ) -> std::result::Result<&WeatherEnvironment, LevelError> {
        self.region()
            .weather
            .environment
            .as_ref()
            .ok_or(LevelError::UnsupportedWeatherQuery)
    }
    pub fn brightness(
        &self,
        layer: LightLayer,
        pos: BlockPos,
    ) -> std::result::Result<u8, LevelError> {
        let chunk = ChunkPos::containing(pos);
        if layer == LightLayer::Sky && !self.has_skylight()? {
            return Ok(0);
        }
        self.region()
            .weather
            .lights
            .get(&chunk)
            .map(|data| data.brightness(layer, pos))
            .ok_or(LevelError::UnsupportedWeatherQuery)
    }
    pub fn biome_at(&self, pos: BlockPos) -> std::result::Result<&Identifier, LevelError> {
        let environment = self.weather_environment()?;
        let quart = zoom_quart(environment.biome_zoom_seed, [pos.x, pos.y, pos.z]);
        let biome = self.noise_biome_at_quart(quart)?;
        environment
            .biome_names
            .get(biome.0 as usize)
            .ok_or(LevelError::UnsupportedWeatherQuery)
    }
    /// Port of ChunkAccess#getNoiseBiome with height clamping to owned sections.
    pub(super) fn noise_biome_at_quart(
        &self,
        quart: [i32; 3],
    ) -> std::result::Result<ferraloom_chunk::BiomeId, LevelError> {
        let p = ChunkPos::new(quart[0] >> 2, quart[2] >> 2);
        let chunk = if let Some(driver) = &self.acquire_driver {
            match self.get_chunk(p, super::chunk_holder::ChunkStatus::Biomes, false)? {
                Some(reference) if reference.kind == super::acquisition::ChunkKind::Full => {
                    self.resolve_full_chunk(reference)?
                }
                Some(reference) => return driver.proto_noise_biome(reference, quart),
                None => return driver.uncached_noise_biome(quart),
            }
        } else {
            self.chunk(p).ok_or(LevelError::MissingChunk(p))?
        };
        let y = quart[1].clamp(chunk.min_y() >> 2, (chunk.max_y() - 1) >> 2);
        let section = chunk
            .section(y >> 2)
            .expect("biome Y clamped to owned chunk");
        Ok(section
            .biomes()
            .get((quart[0] & 3) as u8, (y & 3) as u8, (quart[2] & 3) as u8))
    }
    pub fn heightmap_pos(
        &self,
        map: super::heightmap::Heightmap,
        pos: BlockPos,
    ) -> std::result::Result<BlockPos, LevelError> {
        let height = self.get_height(map, pos.x, pos.z)?;
        Ok(BlockPos::new(pos.x, height, pos.z))
    }
    /// Port of Level#getHeight, including its non-loading presence guard.
    pub fn get_height(
        &self,
        map: super::heightmap::Heightmap,
        x: i32,
        z: i32,
    ) -> std::result::Result<i32, LevelError> {
        if !(-30_000_000..30_000_000).contains(&x) || !(-30_000_000..30_000_000).contains(&z) {
            return Ok(self.sea_level().wrapping_add(1));
        }
        let p = ChunkPos::new(x >> 4, z >> 4);
        if !self.has_chunk(p) {
            return Ok(self.min_build_height());
        }
        let chunk = self.get_full_chunk(p)?;
        let height = chunk
            .height_at(map, (x & 15) as u8, (z & 15) as u8)
            .ok_or(LevelError::UnsupportedWeatherQuery)?;
        Ok(height)
    }
    /// Port of Level#precipitationAt and isRainingAt, including sky and the
    /// motion-blocking heightmap before the zoomed biome climate query.
    pub fn precipitation_at(
        &self,
        pos: BlockPos,
    ) -> std::result::Result<Precipitation, LevelError> {
        if !self.is_raining() {
            return Ok(Precipitation::None);
        }
        if self.brightness(LightLayer::Sky, pos)? < 15 {
            return Ok(Precipitation::None);
        }
        if self
            .heightmap_pos(super::heightmap::Heightmap::MotionBlocking, pos)?
            .y
            > pos.y
        {
            return Ok(Precipitation::None);
        }
        let environment = self.weather_environment()?;
        Ok(environment.temperatures.precipitation_at(
            self.biome_at(pos)?,
            [pos.x, pos.y, pos.z],
            environment.sea_level,
        ))
    }
    pub fn is_raining_at(&self, pos: BlockPos) -> std::result::Result<bool, LevelError> {
        Ok(self.precipitation_at(pos)? == Precipitation::Rain)
    }
    /// Concrete live port of Biome#shouldFreeze; climate math is shared.
    pub fn should_freeze(
        &self,
        biome: &Identifier,
        pos: BlockPos,
        check_neighbors: bool,
    ) -> std::result::Result<bool, LevelError> {
        use ferraloom_data::{BlockClass, registries::fluid::Id as Fluid, tags::FluidTags};
        let environment = self.weather_environment()?;
        if environment.temperatures.warm_enough_to_rain(
            biome,
            [pos.x, pos.y, pos.z],
            environment.sea_level,
        ) {
            return Ok(false);
        }
        if pos.y < self.region().min_y
            || pos.y >= self.region().max_y
            || self.brightness(LightLayer::Block, pos)? >= 10
        {
            return Ok(false);
        }
        let state = self.get_block_state(pos)?;
        if state.data().fluid.fluid_type != Fluid::WATER
            || !state.block().is_instance_of(BlockClass::LiquidBlock)
        {
            return Ok(false);
        }
        if !check_neighbors {
            return Ok(true);
        }
        for direction in [
            ferraloom_util::Direction::West,
            ferraloom_util::Direction::East,
            ferraloom_util::Direction::North,
            ferraloom_util::Direction::South,
        ] {
            if !self.fluid_tags()?.contains_key(
                FluidTags::WATER,
                self.get_block_state(pos.relative(direction, 1))?
                    .data()
                    .fluid
                    .fluid_type,
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Required survival queries remain Unsupported in both runtime modes.
    pub fn should_snow(
        &self,
        biome: &Identifier,
        pos: BlockPos,
    ) -> std::result::Result<bool, LevelError> {
        use ferraloom_data::Block;
        let environment = self.weather_environment()?;
        if environment.temperatures.precipitation_at(
            biome,
            [pos.x, pos.y, pos.z],
            environment.sea_level,
        ) != Precipitation::Snow
        {
            return Ok(false);
        }
        if pos.y < self.region().min_y
            || pos.y >= self.region().max_y
            || self.brightness(LightLayer::Block, pos)? >= 10
        {
            return Ok(false);
        }
        let state = self.get_block_state(pos)?;
        if !state.is_air() && state.block() != Block::SNOW {
            return Ok(false);
        }
        self.can_survive(Block::SNOW.default_state(), pos)
    }
    /// Port of ServerLevel#tickPrecipitation. Selection remains observable in
    /// the pending-family hook while survival, entity pushing and cauldrons land.
    pub fn tick_precipitation(&mut self, pos: BlockPos) -> std::result::Result<(), LevelError> {
        use crate::block::{BlockMethod, UnportedOperation};
        use ferraloom_data::Block;
        let weather = &mut self.region_mut().weather;
        weather.pending_precipitation_calls = weather.pending_precipitation_calls.wrapping_add(1);
        self.emit(LevelEffect::PendingPrecipitation(pos));
        let top = self.heightmap_pos(super::heightmap::Heightmap::MotionBlocking, pos)?;
        let below = top.below(1);
        let biome = self.biome_at(top)?.clone();
        if self.should_freeze(&biome, below, true)? {
            self.set_block_and_update(below, Block::ICE.default_state())?;
        }
        if self.is_raining() {
            if self.rules().max_snow_accumulation_height > 0 {
                let snow = self.accumulate_snow(&biome, top);
                match snow {
                    Err(
                        error @ (LevelError::UnsupportedBlockMethod(_, _)
                        | LevelError::UnsupportedBlock(_)
                        | LevelError::UnsupportedShape(_)),
                    ) => {
                        if self.unported_behavior() == crate::block::UnportedBehavior::Strict {
                            return Err(error);
                        }
                        self.unported_operation(UnportedOperation::SnowAccumulation)?;
                    }
                    other => other?,
                }
            }
            let environment = self.weather_environment()?;
            let precipitation = environment.temperatures.precipitation_at(
                &biome,
                [below.x, below.y, below.z],
                environment.sea_level,
            );
            if precipitation != Precipitation::None {
                let state = self.get_block_state(below)?;
                self.optional_block_call(
                    state.block(),
                    BlockMethod::HandlePrecipitation,
                    |behavior, level| {
                        behavior.handle_precipitation(state, below, precipitation, level)
                    },
                )?;
            }
        }
        Ok(())
    }
    fn accumulate_snow(
        &mut self,
        biome: &Identifier,
        top: BlockPos,
    ) -> std::result::Result<(), LevelError> {
        use ferraloom_data::{Block, properties};
        if !self.should_snow(biome, top)? {
            return Ok(());
        }
        let state = self.get_block_state(top)?;
        if state.block() == Block::SNOW {
            let layers = state.get(properties::LAYERS).unwrap();
            if i32::from(layers) < self.rules().max_snow_accumulation_height.min(8) {
                let next = state.with(properties::LAYERS, layers + 1).unwrap();
                self.unported_operation(crate::block::UnportedOperation::PushEntitiesUp)?;
                self.set_block_and_update(top, next)?;
            }
        } else {
            self.set_block_and_update(top, Block::SNOW.default_state())?;
        }
        Ok(())
    }
    pub fn pending_precipitation_calls(&self) -> u64 {
        self.region().weather.pending_precipitation_calls
    }
    pub fn is_thundering(&self) -> bool {
        self.region()
            .weather
            .is_thundering(self.rules().can_have_weather)
    }
    pub fn rain_level(&self, partial_tick: f32) -> f32 {
        let weather = &self.region().weather;
        weather.old_rain + partial_tick * (weather.rain - weather.old_rain)
    }
    pub fn thunder_level(&self, partial_tick: f32) -> f32 {
        let weather = &self.region().weather;
        (weather.old_thunder + partial_tick * (weather.thunder - weather.old_thunder))
            * self.rain_level(partial_tick)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::LegacyRandomSource;
    fn random() -> Random {
        Random::Legacy(LegacyRandomSource::new(0))
    }
    #[test]
    fn weather_requires_the_lent_server_owner_under_both_policies() {
        static EMPTY: crate::Behaviors = crate::Behaviors::EMPTY;
        for policy in [
            crate::block::UnportedBehavior::Strict,
            crate::block::UnportedBehavior::Warn,
        ] {
            let mut level = Level::new(
                super::super::Region::new(&EMPTY),
                crate::level::LevelContext::fixture(0, 63),
            );
            level.set_unported_behavior(policy);
            assert_eq!(level.weather(), Err(LevelError::MissingServerShared));
            assert_eq!(
                level.set_weather(1, 2, true, false),
                Err(LevelError::MissingServerShared)
            );
            assert_eq!(
                level.reset_weather_cycle(),
                Err(LevelError::MissingServerShared)
            );
            assert_eq!(
                level.advance_weather_cycle(true),
                Err(LevelError::MissingServerShared)
            );
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
    }

    #[test]
    fn serial_levels_advance_the_same_weather_object_with_local_interpolation() {
        static EMPTY: crate::Behaviors = crate::Behaviors::EMPTY;
        let context = crate::level::LevelContext::fixture(0, 63);
        let mut first = Level::new(super::super::Region::new(&EMPTY), context.clone());
        let mut second = Level::new(super::super::Region::new(&EMPTY), context);
        let data = WeatherData {
            rain_time: 2,
            thunder_time: 2,
            ..Default::default()
        };
        first.prepare_weather(&data);
        second.prepare_weather(&data);
        first.install_shared(Box::new(crate::ServerShared {
            weather: data,
            ..Default::default()
        }));
        let identity = std::ptr::from_ref(first.weather().unwrap());
        first.advance_weather_cycle(true).unwrap();
        assert_eq!(first.weather().unwrap().rain_time, 1);
        second.install_shared(first.take_shared().unwrap());
        second.advance_weather_cycle(true).unwrap();
        assert_eq!(std::ptr::from_ref(second.weather().unwrap()), identity);
        assert_eq!(
            *second.weather().unwrap(),
            WeatherData {
                raining: true,
                thundering: true,
                ..Default::default()
            }
        );
        assert_eq!(first.region().weather.rain, 0.0);
        assert_eq!(second.region().weather.rain, 0.01);
        assert_eq!(first.weather(), Err(LevelError::MissingServerShared));
        first.install_shared(second.take_shared().unwrap());
        assert!(first.weather().unwrap().raining);
        assert_eq!(first.region().weather.rain, 0.0);
    }

    #[test]
    fn rain_query_short_circuits_only_canonical_dry_weather() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        for policy in [
            crate::block::UnportedBehavior::Strict,
            crate::block::UnportedBehavior::Warn,
        ] {
            let mut level = Level::new(
                super::super::Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            level.set_unported_behavior(policy);
            let pos = BlockPos::new(0, 0, 0);
            assert_eq!(level.is_raining_at(pos), Ok(false));
            level.install_shared(Box::default());
            level
                .load_weather(WeatherData {
                    raining: true,
                    ..WeatherData::default()
                })
                .unwrap();
            assert_eq!(
                level.is_raining_at(pos),
                Err(LevelError::UnsupportedLightQuery)
            );
            level.set_has_skylight(true);
            assert_eq!(
                level.is_raining_at(pos),
                Err(LevelError::UnsupportedWeatherQuery)
            );
            level.rules_mut().can_have_weather = false;
            assert_eq!(level.is_raining_at(pos), Ok(false));
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
    }
    #[test]
    fn clear_time_overrides_targets_without_sampling_and_rule_false_still_smooths() {
        let mut data = WeatherData {
            clear_weather_time: 2,
            rain_time: 5,
            thunder_time: 6,
            raining: true,
            thundering: true,
        };
        let mut weather = Weather::from_saved(data);
        let mut stream = random();
        weather.advance(&mut data, &mut stream, true, true);
        assert_eq!(
            data,
            WeatherData {
                clear_weather_time: 1,
                ..WeatherData::default()
            }
        );
        assert_eq!(weather.rain, 0.99);
        assert_eq!(stream.next_int(), random().next_int());
        let snapshot = data;
        weather.advance(&mut data, &mut stream, false, true);
        assert_eq!(data, snapshot);
        assert_eq!(weather.rain, 0.98);
        assert_eq!(WeatherData::decode(&data.encode().unwrap()).unwrap(), data);
    }
    #[test]
    fn timer_expiry_toggles_before_resampling_and_rain_threshold_duplicates_level_packets() {
        let mut data = WeatherData {
            rain_time: 1,
            thunder_time: 1,
            ..WeatherData::default()
        };
        let mut weather = Weather::from_saved(data);
        let mut stream = random();
        weather.advance(&mut data, &mut stream, true, true);
        assert!(data.raining && data.thundering);
        assert_eq!((data.rain_time, data.thunder_time), (0, 0));
        let mut expected = random();
        let thunder = expected.next_int_between_inclusive(3600, 15600);
        let rain = expected.next_int_between_inclusive(12000, 24000);
        weather.advance(&mut data, &mut stream, true, true);
        assert_eq!((data.thunder_time, data.rain_time), (thunder, rain));
        weather.old_rain = 0.2;
        weather.rain = 0.2;
        weather.old_thunder = 0.0;
        weather.thunder = 0.0;
        let events = weather.advance(&mut data, &mut stream, false, true);
        assert!(matches!(
            events.as_slice(),
            [
                LevelEffect::WeatherEvent { event: 7, .. },
                LevelEffect::WeatherEvent { event: 8, .. }
            ]
        ));
        weather.rain = 0.19;
        let events = weather.advance(&mut data, &mut stream, false, true);
        assert_eq!(
            events
                .iter()
                .map(|e| match e {
                    LevelEffect::WeatherEvent { event, .. } => *event,
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>(),
            vec![7, 8, 1, 7, 8]
        );
    }
}
