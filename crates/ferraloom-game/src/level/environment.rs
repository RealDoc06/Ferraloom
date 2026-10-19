//! Concrete Level environment queries. The server owns and advances every clock;
//! this runtime only retains an immutable phase snapshot and native read caches.
use super::{Level, LevelError};
use crate::block::LevelReaderRef;
use ferraloom_util::{BlockPos, Vec3, mth};
pub use ferraloom_worldgen::environment::{
    AttributeKey, AttributeValue, ClockSnapshot, DimensionEnvironment, EnvironmentAttributes,
    EnvironmentDefinitions, SpatialAttributeInterpolator, TriState,
};
use ferraloom_worldgen::environment::{EnvironmentAttributeSystem, WeatherSample};
use ferraloom_worldgen::{Identifier, biome_manager::zoom_quart};
use std::sync::Arc;

pub(super) struct EnvironmentRuntime {
    definitions: Arc<EnvironmentDefinitions>,
    system: EnvironmentAttributeSystem,
    biome_indices: Vec<usize>,
    dimension: Identifier,
    sky_darken: Option<i32>,
}
impl Level {
    /// Install resolved dimension and biome layers once the canonical clock
    /// manager has loaded. Registry IDs are translated once, never by hot strings.
    pub fn set_environment_attributes(
        &mut self,
        definitions: Arc<EnvironmentDefinitions>,
        dimension: &DimensionEnvironment,
        dimension_key: Identifier,
        biome_names: &[Identifier],
        clocks: ClockSnapshot,
    ) -> Result<(), LevelError> {
        let biome_indices = biome_names
            .iter()
            .map(|name| {
                definitions
                    .biome_index(name)
                    .ok_or(LevelError::MissingEnvironmentContext)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let system = EnvironmentAttributeSystem::new_live(
            &definitions,
            dimension,
            clocks,
            self.rules().can_have_weather,
        );
        self.region_mut().environment = Some(EnvironmentRuntime {
            definitions,
            system,
            biome_indices,
            dimension: dimension_key,
            sky_darken: None,
        });
        self.update_sky_brightness()
    }
    /// Port of ServerClockManager#modifyClock's immediate cache invalidation.
    /// This publishes clocks without advancing them or changing skyDarken early.
    pub fn refresh_environment_attributes(
        &mut self,
        clocks: ClockSnapshot,
    ) -> Result<(), LevelError> {
        self.region_mut()
            .environment
            .as_mut()
            .ok_or(LevelError::MissingEnvironmentContext)?
            .system
            .refresh_clocks(clocks);
        Ok(())
    }
    /// Port of ServerLevel#tick's unconditional cache invalidation, before weather.
    pub fn invalidate_environment_tick_cache(&mut self) -> Result<(), LevelError> {
        self.region_mut()
            .environment
            .as_mut()
            .ok_or(LevelError::MissingEnvironmentContext)?
            .system
            .invalidate_tick_cache();
        Ok(())
    }
    pub fn dimension_key(&self) -> Result<&Identifier, LevelError> {
        Ok(&self.environment_runtime()?.dimension)
    }
    pub fn clock_ticks(&self, clock: &Identifier) -> Result<i64, LevelError> {
        self.environment_runtime()?
            .system
            .clock_ticks(clock)
            .ok_or(LevelError::MissingEnvironmentContext)
    }
    fn environment_runtime(&self) -> Result<&EnvironmentRuntime, LevelError> {
        self.region()
            .environment
            .as_ref()
            .ok_or(LevelError::MissingEnvironmentContext)
    }
    fn environment_weather(&self) -> WeatherSample {
        WeatherSample {
            rain: self.rain_level(1.0),
            thunder: self.thunder_level(1.0),
        }
    }
    pub fn get_dimension_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
    ) -> Result<T, LevelError> {
        self.environment_runtime()?
            .system
            .get_dimension_value(key, self.environment_weather())
            .map_err(|error| LevelError::UnsupportedEnvironmentAttribute(error.0))
    }
    /// Port of EnvironmentAttributeReader#getValue(BlockPos): sample its center.
    pub fn get_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
        pos: BlockPos,
    ) -> Result<T, LevelError> {
        self.get_value_at(
            key,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            None,
        )
    }
    pub fn get_value_at<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
        pos: Vec3,
        spatial: Option<&SpatialAttributeInterpolator>,
    ) -> Result<T, LevelError> {
        let runtime = self.environment_runtime()?;
        let id = key.id();
        if !runtime.system.is_affected_by_position(id) {
            return self.get_dimension_value(key);
        }
        let attributes = if spatial.is_some() && id.data().spatially_interpolated {
            None
        } else {
            let block = [mth::floor(pos.x), mth::floor(pos.y), mth::floor(pos.z)];
            let quart = if id.data().full_resolution_biomes {
                zoom_quart(self.weather_environment()?.biome_zoom_seed, block)
            } else {
                block.map(|v| v >> 2)
            };
            let biome = self.noise_biome_at_quart(quart)?;
            let index = runtime
                .biome_indices
                .get(biome.0 as usize)
                .ok_or(LevelError::MissingEnvironmentContext)?;
            Some(
                runtime
                    .definitions
                    .biome(*index)
                    .ok_or(LevelError::MissingEnvironmentContext)?
                    .as_ref(),
            )
        };
        runtime
            .system
            .get_value(key, attributes, spatial, self.environment_weather())
            .map_err(|error| LevelError::UnsupportedEnvironmentAttribute(error.0))
    }
    pub fn water_evaporates(&self, pos: BlockPos) -> Result<bool, LevelError> {
        self.get_value(EnvironmentAttributes::WATER_EVAPORATES, pos)
    }
    pub fn fast_lava(&self) -> Result<bool, LevelError> {
        self.get_dimension_value(EnvironmentAttributes::FAST_LAVA)
    }
    /// Port of Level#updateSkyBrightness, independent of the simulation freeze gate.
    pub fn update_sky_brightness(&mut self) -> Result<(), LevelError> {
        let value = self.get_dimension_value(EnvironmentAttributes::SKY_LIGHT_LEVEL)?;
        self.region_mut()
            .environment
            .as_mut()
            .ok_or(LevelError::MissingEnvironmentContext)?
            .sky_darken = Some((15.0_f32 - value) as i32);
        Ok(())
    }
    pub fn sky_darken(&self) -> Result<i32, LevelError> {
        self.environment_runtime()?
            .sky_darken
            .ok_or(LevelError::MissingEnvironmentContext)
    }
}
impl LevelReaderRef<'_, '_> {
    pub fn get_dimension_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
    ) -> Result<T, LevelError> {
        match self {
            Self::Server(level) => level.get_dimension_value(key),
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                super::path_navigation_region::PathNavigationQuery::Environment,
            )),
            Self::Worldgen(region) => region
                .get_dimension_value(key)
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    pub fn get_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
        pos: BlockPos,
    ) -> Result<T, LevelError> {
        match self {
            Self::Server(level) => level.get_value(key, pos),
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                super::path_navigation_region::PathNavigationQuery::Environment,
            )),
            Self::Worldgen(region) => region
                .get_environment_value(key, [pos.x, pos.y, pos.z])
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    pub fn sky_darken(&self) -> Result<i32, LevelError> {
        match self {
            Self::Server(level) => level.sky_darken(),
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                super::path_navigation_region::PathNavigationQuery::Environment,
            )),
            Self::Worldgen(_) => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{Region, chunk::LevelChunk, weather::ChunkLightSnapshot},
    };
    use ferraloom_data::{
        datapack::{Pack, ResourceManager},
        registries::environment_attribute::Id,
    };
    use ferraloom_util::ChunkPos;
    use ferraloom_worldgen::environment::AttributeMap;
    use serde_json::json;
    static EMPTY: Behaviors = Behaviors::EMPTY;

    #[test]
    fn canonical_clock_refresh_and_sky_slot_keep_biome_and_spatial_reads_exact() {
        let mut level = Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        assert_eq!(
            level.get_dimension_value(EnvironmentAttributes::SKY_LIGHT_LEVEL),
            Err(LevelError::MissingEnvironmentContext)
        );
        let mut pack = Pack::new("live-environment");
        for (name, value) in [
            (
                "worldgen/biome/case.json",
                json!({"attributes":{"gameplay/water_evaporates":true,"visual/fog_start_distance":2}}),
            ),
            (
                "timeline/day.json",
                json!({"clock":"overworld","tracks":{"gameplay/sky_light_level":{"keyframes":[{"ticks":0,"value":15},{"ticks":10,"value":0}]}}}),
            ),
        ] {
            pack.insert(name.parse().unwrap(), serde_json::to_vec(&value).unwrap());
        }
        let definitions =
            Arc::new(EnvironmentDefinitions::load(&ResourceManager::new(vec![pack])).unwrap());
        let mut dimension = definitions
            .dimension(
                &json!({"attributes":{"gameplay/water_evaporates":false},"timelines":["day"]}),
            )
            .unwrap();
        dimension.timelines.push(Arc::new(ferraloom_worldgen::environment::Timeline::decode(&json!({"clock":"overworld","tracks":{"gameplay/fast_lava":{"keyframes":[{"ticks":0,"value":false},{"ticks":10,"value":true}]}}})).unwrap()));
        let clocks = |tick| ClockSnapshot::new([("overworld".parse().unwrap(), tick)]);
        level
            .set_environment_attributes(
                definitions.clone(),
                &dimension,
                "overworld".parse().unwrap(),
                &["case".parse().unwrap()],
                clocks(0),
            )
            .unwrap();
        level.set_has_skylight(true);
        assert_eq!(level.sky_darken(), Ok(0));
        assert_eq!(
            crate::block::fluid::FlowingFluid::Lava.tick_delay_for(&level),
            Ok(30)
        );
        assert_eq!(
            level.get_dimension_value(EnvironmentAttributes::WATER_EVAPORATES),
            Ok(false)
        );
        assert_eq!(
            level.water_evaporates(BlockPos::new(8, 8, 8)),
            Err(LevelError::MissingChunk(ChunkPos::new(0, 0)))
        );
        level
            .add_chunk(LevelChunk::new(
                ChunkPos::new(0, 0),
                0..1,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        let pos = BlockPos::new(8, 8, 8);
        level.set_light_snapshot(ChunkPos::new(0, 0), ChunkLightSnapshot::default());
        assert_eq!(level.water_evaporates(pos), Ok(true));
        level.refresh_environment_attributes(clocks(5)).unwrap();
        assert_eq!(
            level.get_dimension_value(EnvironmentAttributes::SKY_LIGHT_LEVEL),
            Ok(7.5)
        );
        assert_eq!(
            crate::loot::LootFacts::clock_ticks(
                &level,
                &ferraloom_protocol::Identifier::parse("overworld").unwrap()
            )
            .unwrap(),
            5
        );
        assert_eq!(
            crate::loot::LootFacts::environment_float(
                &level,
                &ferraloom_protocol::Identifier::parse("gameplay/sky_light_level").unwrap(),
                &crate::loot::LootParams::default()
            )
            .unwrap(),
            7.5
        );
        assert_eq!(level.sky_darken(), Ok(0));
        level.update_sky_brightness().unwrap();
        assert_eq!(level.sky_darken(), Ok(7));
        assert_eq!(level.max_local_raw_brightness(pos), Ok(8));
        assert_eq!(
            LevelReaderRef::Server(&level).get_effective_sky_brightness(pos),
            Ok(8)
        );
        assert_eq!(
            level.max_local_raw_brightness_with_sky(BlockPos::new(30_000_000, 0, 0), i32::MIN),
            Ok(15)
        );
        assert_eq!(
            level.dimension_key().unwrap().to_string(),
            "minecraft:overworld"
        );
        level.refresh_environment_attributes(clocks(10)).unwrap();
        assert_eq!(
            crate::block::fluid::FlowingFluid::Lava.tick_delay_for(&level),
            Ok(10)
        );

        let source =
            Arc::new(AttributeMap::decode(&json!({"visual/fog_start_distance":4}), true).unwrap());
        let mut weights = SpatialAttributeInterpolator::default();
        weights.accumulate(1.0, source);
        let fog = AttributeKey::<f32>::new(Id::VISUAL_FOG_START_DISTANCE);
        // A supplied interpolator is the biome layer. Native code never makes an
        // extra chunk read here, including outside the owned chunk neighborhood.
        assert_eq!(
            level.get_value_at(fog, Vec3::new(32.5, 8.5, 8.5), Some(&weights)),
            Ok(4.0)
        );
        assert_eq!(
            level.get_value_at(fog, Vec3::new(32.5, 8.5, 8.5), None),
            Err(LevelError::MissingChunk(ChunkPos::new(2, 0)))
        );
    }
}
