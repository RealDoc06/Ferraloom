//! Ports of WeatherData, ServerLevel#advanceWeatherCycle and prepareWeather.
use super::{Level, LevelError, effects::LevelEffect};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_storage::{Error, Result, SavedDataCodec};
use ferraloom_util::BlockPos;
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
    fn decode(tag: &Tag) -> Result<Self> {
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
    fn encode(&self) -> Result<Tag> {
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
    data: WeatherData,
    old_rain: f32,
    rain: f32,
    old_thunder: f32,
    thunder: f32,
    pending_precipitation_calls: u64,
}
impl Weather {
    fn from_saved(data: WeatherData) -> Self {
        Self {
            data,
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
        random: &mut Random,
        advance_weather: bool,
        can_have_weather: bool,
    ) -> Vec<LevelEffect> {
        let was_raining = self.is_raining(can_have_weather);
        if can_have_weather {
            if advance_weather {
                let data = &mut self.data;
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
                (self.thunder + if self.data.thundering { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
            self.old_rain = self.rain;
            self.rain = (self.rain + if self.data.raining { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
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
    pub fn load_weather(&mut self, data: WeatherData) {
        self.region_mut().weather = Weather::from_saved(data);
    }
    pub fn weather(&self) -> &WeatherData {
        &self.region().weather.data
    }
    /// Port of MinecraftServer#setWeatherParameters. A command changes targets;
    /// levels smooth toward those targets in the next weather tick.
    pub fn set_weather(
        &mut self,
        clear_time: i32,
        rain_time: i32,
        raining: bool,
        thundering: bool,
    ) {
        self.region_mut().weather.data = WeatherData {
            clear_weather_time: clear_time,
            rain_time,
            thunder_time: rain_time,
            raining,
            thundering,
        };
    }
    /// Port of ServerLevel#resetWeatherCycle, also used after sleeping. Caller
    /// owns vanilla's sleeping-percentage, ADVANCE_WEATHER and raining guards.
    pub fn reset_weather_cycle(&mut self) {
        let data = &mut self.region_mut().weather.data;
        data.rain_time = 0;
        data.raining = false;
        data.thunder_time = 0;
        data.thundering = false;
    }
    /// Call at ServerLevel's weather slot before tickTime. The rule is read from
    /// canonical GameRules at this call site rather than cached at startup.
    pub fn advance_weather_cycle(&mut self, advance_weather: bool) {
        let region = self.region_mut();
        let events = region.weather.advance(
            &mut region.random,
            advance_weather,
            region.rules.can_have_weather,
        );
        region.effects.extend(events);
    }
    pub fn is_raining(&self) -> bool {
        self.region()
            .weather
            .is_raining(self.rules().can_have_weather)
    }
    /// Port of Level#isRainingAt. The dry short circuit is complete. Rain still
    /// requires the pending live skylight and biome-precipitation queries.
    pub fn is_raining_at(&self, _pos: BlockPos) -> std::result::Result<bool, LevelError> {
        if !self.is_raining() {
            return Ok(false);
        }
        Err(LevelError::UnsupportedWeatherQuery)
    }
    /// ServerLevel#tickPrecipitation integration point. Selection and randValue
    /// advancement run now; snow, ice and cauldron effects await biome helpers.
    pub fn tick_precipitation(&mut self, pos: BlockPos) {
        let weather = &mut self.region_mut().weather;
        weather.pending_precipitation_calls = weather.pending_precipitation_calls.wrapping_add(1);
        self.emit(LevelEffect::PendingPrecipitation(pos));
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
    fn rain_query_short_circuits_only_canonical_dry_weather() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = Level::new(super::super::Region::new(&BEHAVIORS));
        let pos = BlockPos::new(0, 0, 0);
        assert_eq!(level.is_raining_at(pos), Ok(false));
        level.load_weather(WeatherData {
            raining: true,
            ..WeatherData::default()
        });
        assert_eq!(
            level.is_raining_at(pos),
            Err(LevelError::UnsupportedWeatherQuery)
        );
        level.rules_mut().can_have_weather = false;
        assert_eq!(level.is_raining_at(pos), Ok(false));
    }
    #[test]
    fn clear_time_overrides_targets_without_sampling_and_rule_false_still_smooths() {
        let data = WeatherData {
            clear_weather_time: 2,
            rain_time: 5,
            thunder_time: 6,
            raining: true,
            thundering: true,
        };
        let mut weather = Weather::from_saved(data);
        let mut stream = random();
        weather.advance(&mut stream, true, true);
        assert_eq!(
            weather.data,
            WeatherData {
                clear_weather_time: 1,
                ..WeatherData::default()
            }
        );
        assert_eq!(weather.rain, 0.99);
        assert_eq!(stream.next_int(), random().next_int());
        let snapshot = weather.data;
        weather.advance(&mut stream, false, true);
        assert_eq!(weather.data, snapshot);
        assert_eq!(weather.rain, 0.98);
        assert_eq!(WeatherData::decode(&data.encode().unwrap()).unwrap(), data);
    }
    #[test]
    fn timer_expiry_toggles_before_resampling_and_rain_threshold_duplicates_level_packets() {
        let mut weather = Weather::from_saved(WeatherData {
            rain_time: 1,
            thunder_time: 1,
            ..WeatherData::default()
        });
        let mut stream = random();
        weather.advance(&mut stream, true, true);
        assert!(weather.data.raining && weather.data.thundering);
        assert_eq!((weather.data.rain_time, weather.data.thunder_time), (0, 0));
        let mut expected = random();
        let thunder = expected.next_int_between_inclusive(3600, 15600);
        let rain = expected.next_int_between_inclusive(12000, 24000);
        weather.advance(&mut stream, true, true);
        assert_eq!(
            (weather.data.thunder_time, weather.data.rain_time),
            (thunder, rain)
        );
        weather.old_rain = 0.2;
        weather.rain = 0.2;
        weather.old_thunder = 0.0;
        weather.thunder = 0.0;
        let events = weather.advance(&mut stream, false, true);
        assert!(matches!(
            events.as_slice(),
            [
                LevelEffect::WeatherEvent { event: 7, .. },
                LevelEffect::WeatherEvent { event: 8, .. }
            ]
        ));
        weather.rain = 0.19;
        let events = weather.advance(&mut stream, false, true);
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
