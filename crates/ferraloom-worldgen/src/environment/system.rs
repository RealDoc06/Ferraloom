//! Port of EnvironmentAttributeSystem's layer order and native tick caches.
//! Clocks are immutable snapshots; neither this sampler nor a region advances them.
use super::{
    AttributeEntry, AttributeKey, AttributeMap, AttributeValue, ClockSnapshot,
    DimensionEnvironment, EnvironmentDefinitions, ScalarValue, Timeline, UnsupportedAttribute,
};
use ferraloom_data::registries::environment_attribute::{self, Id};
use ferraloom_util::mth;
use std::{
    cell::{Cell, OnceCell},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherSample {
    pub rain: f32,
    /// Native getThunderLevel(1), already multiplied by the rain level.
    pub thunder: f32,
}
struct TimelineLayer {
    timeline: Arc<Timeline>,
    argument: OnceCell<Result<AttributeEntry, UnsupportedAttribute>>,
}
struct Sampler {
    base: Result<ScalarValue, UnsupportedAttribute>,
    positional: bool,
    timelines: Vec<TimelineLayer>,
    dimension_value: Cell<Option<Result<ScalarValue, UnsupportedAttribute>>>,
}
pub struct EnvironmentAttributeSystem {
    samplers: Vec<Sampler>,
    clocks: ClockSnapshot,
    weather: bool,
}
impl EnvironmentAttributeSystem {
    pub fn new_static(
        definitions: &EnvironmentDefinitions,
        dimension: &DimensionEnvironment,
    ) -> Self {
        Self::new(
            definitions,
            dimension,
            ClockSnapshot::default(),
            false,
            false,
        )
    }
    pub fn new_live(
        definitions: &EnvironmentDefinitions,
        dimension: &DimensionEnvironment,
        clocks: ClockSnapshot,
        can_have_weather: bool,
    ) -> Self {
        Self::new(definitions, dimension, clocks, true, can_have_weather)
    }
    fn new(
        definitions: &EnvironmentDefinitions,
        dimension: &DimensionEnvironment,
        clocks: ClockSnapshot,
        dynamic: bool,
        weather: bool,
    ) -> Self {
        let samplers = environment_attribute::ALL
            .iter()
            .copied()
            .map(|id| Sampler {
                base: definitions
                    .default_value(id)
                    .and_then(|base| dimension.attributes.apply(id, base)),
                positional: definitions.biome_attributes[id.protocol_id() as usize],
                timelines: dimension
                    .timelines
                    .iter()
                    .filter(|timeline| dynamic && timeline.track(id).is_some())
                    .map(|timeline| TimelineLayer {
                        timeline: timeline.clone(),
                        argument: OnceCell::new(),
                    })
                    .collect(),
                dimension_value: Cell::new(None),
            })
            .collect();
        Self {
            samplers,
            clocks,
            weather,
        }
    }
    /// Port of invalidateTickCache, also called immediately after clock commands.
    /// Sky darkening is updated separately at Level#updateSkyBrightness's slot.
    pub fn refresh_clocks(&mut self, clocks: ClockSnapshot) {
        self.clocks = clocks;
        self.invalidate_tick_cache();
    }
    pub fn invalidate_tick_cache(&mut self) {
        for sampler in &mut self.samplers {
            sampler.dimension_value.set(None);
            for timeline in &mut sampler.timelines {
                timeline.argument.take();
            }
        }
    }
    pub fn is_affected_by_position(&self, id: Id) -> bool {
        self.samplers[id.protocol_id() as usize].positional
    }
    pub fn clock_ticks(&self, clock: &crate::Identifier) -> Option<i64> {
        self.clocks.ticks(clock)
    }
    pub fn get_dimension_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
        weather: WeatherSample,
    ) -> Result<T, UnsupportedAttribute> {
        let id = key.id();
        let sampler = &self.samplers[id.protocol_id() as usize];
        let result = match sampler.dimension_value.get() {
            Some(result) => result,
            None => {
                let result = sampler
                    .base
                    .and_then(|base| self.finish_layers(id, base, weather));
                sampler.dimension_value.set(Some(result));
                result
            }
        }?;
        T::project(result).ok_or(UnsupportedAttribute(id))
    }
    pub fn get_value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
        biome: Option<&AttributeMap>,
        spatial: Option<&SpatialAttributeInterpolator>,
        weather: WeatherSample,
    ) -> Result<T, UnsupportedAttribute> {
        let id = key.id();
        let sampler = &self.samplers[id.protocol_id() as usize];
        if !sampler.positional {
            return self.get_dimension_value(key, weather);
        }
        let base = sampler.base?;
        let base = if id.data().spatially_interpolated {
            match spatial {
                Some(weights) => weights.apply(id, base)?,
                None => biome.ok_or(UnsupportedAttribute(id))?.apply(id, base)?,
            }
        } else {
            biome.ok_or(UnsupportedAttribute(id))?.apply(id, base)?
        };
        let result = self.finish_layers(id, base, weather)?;
        T::project(result).ok_or(UnsupportedAttribute(id))
    }
    fn finish_layers(
        &self,
        id: Id,
        mut result: ScalarValue,
        weather: WeatherSample,
    ) -> Result<ScalarValue, UnsupportedAttribute> {
        for layer in &self.samplers[id.protocol_id() as usize].timelines {
            let entry = layer
                .argument
                .get_or_init(|| {
                    let ticks = self
                        .clocks
                        .ticks(layer.timeline.clock())
                        .ok_or(UnsupportedAttribute(id))?;
                    layer
                        .timeline
                        .track(id)
                        .expect("layer registered for attribute")
                        .sample(ticks)
                })
                .as_ref()
                .map_err(|error| *error)?;
            result = entry.apply(id, result)?;
        }
        if self.weather {
            result = apply_weather(id, result, weather)?;
        }
        Ok(result.sanitize(id))
    }
}

/// Native reference-identity accumulation and source-order weighted interpolation.
/// Sources are shared immutable maps, including the common missing-field EMPTY map.
#[derive(Default)]
pub struct SpatialAttributeInterpolator {
    sources: Vec<(Arc<AttributeMap>, f64)>,
}
impl SpatialAttributeInterpolator {
    pub fn clear(&mut self) {
        self.sources.clear();
    }
    pub fn accumulate(&mut self, weight: f64, attributes: Arc<AttributeMap>) {
        if let Some((_, existing)) = self
            .sources
            .iter_mut()
            .find(|(source, _)| Arc::ptr_eq(source, &attributes))
        {
            *existing += weight;
        } else {
            self.sources.push((attributes, weight));
        }
    }
    fn apply(&self, id: Id, base: ScalarValue) -> Result<ScalarValue, UnsupportedAttribute> {
        let mut result = None;
        let mut accumulated = 0.0;
        for (attributes, weight) in &self.sources {
            let value = attributes.apply(id, base)?;
            accumulated += weight;
            result = Some(match result {
                None => value,
                Some(previous) => {
                    spatial_lerp(id, (*weight / accumulated) as f32, previous, value)?
                }
            });
        }
        Ok(result.unwrap_or(base))
    }
}
fn spatial_lerp(
    id: Id,
    alpha: f32,
    from: ScalarValue,
    to: ScalarValue,
) -> Result<ScalarValue, UnsupportedAttribute> {
    match (from, to) {
        (ScalarValue::Float(from), ScalarValue::Float(to)) => {
            Ok(ScalarValue::Float(mth::lerp_f32(alpha, from, to)))
        }
        (ScalarValue::Boolean(_), ScalarValue::Boolean(_))
        | (ScalarValue::TriState(_), ScalarValue::TriState(_)) => {
            Ok(if alpha >= 0.5 { to } else { from })
        }
        _ => Err(UnsupportedAttribute(id)),
    }
}
fn apply_weather(
    id: Id,
    value: ScalarValue,
    weather: WeatherSample,
) -> Result<ScalarValue, UnsupportedAttribute> {
    let rain = weather.rain - weather.thunder;
    match id {
        Id::GAMEPLAY_BEES_STAY_IN_HIVE if rain > 0.0 || weather.thunder > 0.0 => {
            Ok(ScalarValue::Boolean(true))
        }
        Id::GAMEPLAY_SKY_LIGHT_LEVEL | Id::VISUAL_SKY_LIGHT_FACTOR | Id::VISUAL_STAR_BRIGHTNESS => {
            let ScalarValue::Float(mut value) = value else {
                return Err(UnsupportedAttribute(id));
            };
            let (target, rain_alpha, thunder_alpha) = if id == Id::VISUAL_STAR_BRIGHTNESS {
                (0.0, 1.0, 1.0)
            } else if id == Id::GAMEPLAY_SKY_LIGHT_LEVEL {
                (4.0, 0.3125, 0.52734375)
            } else {
                (0.24, 0.3125, 0.52734375)
            };
            if rain > 0.0 {
                value = mth::lerp_f32(rain, value, mth::lerp_f32(rain_alpha, value, target));
            }
            if weather.thunder > 0.0 {
                value = mth::lerp_f32(
                    weather.thunder,
                    value,
                    mth::lerp_f32(thunder_alpha, value, target),
                );
            }
            Ok(ScalarValue::Float(value))
        }
        _ => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::EnvironmentAttributes;
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use serde_json::{Value, json};

    fn attribute_map(id: Id, entry: Value) -> Value {
        Value::Object(serde_json::Map::from_iter([(id.name().to_owned(), entry)]))
    }
    fn fixture(
        id: Id,
        dimension: Value,
        biome: Value,
        time: Option<Value>,
    ) -> (EnvironmentDefinitions, DimensionEnvironment) {
        let mut pack = Pack::new("environment-layers");
        pack.insert(
            "worldgen/biome/case.json".parse().unwrap(),
            serde_json::to_vec(&json!({"attributes":biome})).unwrap(),
        );
        let mut input = json!({"attributes":dimension});
        if let Some(time) = time {
            let tracks = attribute_map(
                id,
                json!({"modifier":time["modifier"],"keyframes":[{"ticks":0,"value":time["argument"]}]}),
            );
            pack.insert(
                "timeline/case.json".parse().unwrap(),
                serde_json::to_vec(&json!({"clock":"overworld","tracks":tracks})).unwrap(),
            );
            input["timelines"] = json!(["case"]);
        }
        let definitions = EnvironmentDefinitions::load(&ResourceManager::new(vec![pack])).unwrap();
        let dimension = definitions.dimension(&input).unwrap();
        (definitions, dimension)
    }
    fn clocks() -> ClockSnapshot {
        ClockSnapshot::new([("overworld".parse().unwrap(), 0)])
    }
    #[test]
    fn layers_weather_and_spatial_identity_match_real_environment_system() {
        let mut count = 0;
        for line in include_str!("../../tests/java/environment/layers.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            match fields[0] {
                "layers" => {
                    let id = Id::VISUAL_FOG_END_DISTANCE;
                    let entry = |field: &str| serde_json::from_str::<Value>(field).unwrap();
                    let (definitions, dimension) = fixture(
                        id,
                        attribute_map(id, entry(fields[1])),
                        attribute_map(id, entry(fields[2])),
                        Some(entry(fields[3])),
                    );
                    let system = EnvironmentAttributeSystem::new_live(
                        &definitions,
                        &dimension,
                        clocks(),
                        false,
                    );
                    let key = AttributeKey::<f32>::new(id);
                    assert_eq!(
                        system
                            .get_dimension_value(key, WeatherSample::default())
                            .unwrap()
                            .to_bits(),
                        fields[4].parse::<u32>().unwrap(),
                        "{line}"
                    );
                    assert_eq!(
                        system
                            .get_value(
                                key,
                                Some(definitions.biome(0).unwrap()),
                                None,
                                WeatherSample::default()
                            )
                            .unwrap()
                            .to_bits(),
                        fields[5].parse::<u32>().unwrap(),
                        "{line}"
                    );
                }
                "weather" | "bees" => {
                    let id = if fields[0] == "weather" {
                        Id::GAMEPLAY_SKY_LIGHT_LEVEL
                    } else {
                        Id::GAMEPLAY_BEES_STAY_IN_HIVE
                    };
                    let base: Value = serde_json::from_str(fields[1]).unwrap();
                    let (definitions, dimension) =
                        fixture(id, attribute_map(id, base), json!({}), None);
                    let system = EnvironmentAttributeSystem::new_live(
                        &definitions,
                        &dimension,
                        clocks(),
                        true,
                    );
                    let weather = WeatherSample {
                        rain: fields[2].parse().unwrap(),
                        thunder: fields[3].parse().unwrap(),
                    };
                    if fields[0] == "weather" {
                        assert_eq!(
                            system
                                .get_dimension_value(
                                    EnvironmentAttributes::SKY_LIGHT_LEVEL,
                                    weather
                                )
                                .unwrap()
                                .to_bits(),
                            fields[4].parse::<u32>().unwrap(),
                            "{line}"
                        );
                    } else {
                        assert_eq!(
                            system
                                .get_dimension_value(
                                    EnvironmentAttributes::BEES_STAY_IN_HIVE,
                                    weather
                                )
                                .unwrap(),
                            fields[4].parse::<bool>().unwrap(),
                            "{line}"
                        );
                    }
                }
                "spatial" => {
                    let id = Id::VISUAL_FOG_END_DISTANCE;
                    let base: f32 = fields[1].parse().unwrap();
                    let a: f32 = fields[2].parse().unwrap();
                    let weight: f64 = fields[3].parse().unwrap();
                    let entry = json!({"modifier":"add","argument":a});
                    let (definitions, dimension) = fixture(
                        id,
                        attribute_map(id, json!(base)),
                        attribute_map(id, entry),
                        None,
                    );
                    let system = EnvironmentAttributeSystem::new_live(
                        &definitions,
                        &dimension,
                        clocks(),
                        false,
                    );
                    let first = definitions.biome(0).unwrap().clone();
                    let second = Arc::new(
                        AttributeMap::decode(
                            &attribute_map(id, json!({"modifier":"multiply","argument":0.25})),
                            true,
                        )
                        .unwrap(),
                    );
                    let third = Arc::new(
                        AttributeMap::decode(
                            &attribute_map(id, json!({"modifier":"add","argument":-100})),
                            true,
                        )
                        .unwrap(),
                    );
                    let mut spatial = SpatialAttributeInterpolator::default();
                    spatial.accumulate(weight, first.clone());
                    spatial.accumulate(0.3, second);
                    spatial.accumulate(0.2, first.clone());
                    spatial.accumulate(0.7, third);
                    assert_eq!(
                        system
                            .get_value(
                                AttributeKey::<f32>::new(id),
                                Some(&first),
                                Some(&spatial),
                                WeatherSample::default()
                            )
                            .unwrap()
                            .to_bits(),
                        fields[4].parse::<u32>().unwrap(),
                        "{line}"
                    );
                }
                _ => panic!("unknown case {line}"),
            }
            count += 1;
        }
        assert_eq!(count, 101);
    }
    #[test]
    fn static_layers_ignore_clocks_and_live_cache_requires_explicit_refresh() {
        let id = Id::GAMEPLAY_SKY_LIGHT_LEVEL;
        let (definitions, mut dimension) = fixture(id, json!({}), json!({}), None);
        dimension.timelines.push(Arc::new(Timeline::decode(&json!({"clock":"overworld","tracks":{"gameplay/sky_light_level":{"keyframes":[{"ticks":0,"value":15},{"ticks":10,"value":0}]}}})).unwrap()));
        let key = EnvironmentAttributes::SKY_LIGHT_LEVEL;
        let weather = WeatherSample::default();
        let static_system = EnvironmentAttributeSystem::new_static(&definitions, &dimension);
        assert_eq!(static_system.get_dimension_value(key, weather), Ok(15.0));
        let mut live = EnvironmentAttributeSystem::new_live(
            &definitions,
            &dimension,
            ClockSnapshot::default(),
            false,
        );
        assert_eq!(
            live.get_dimension_value(key, weather),
            Err(UnsupportedAttribute(id))
        );
        live.refresh_clocks(clocks());
        assert_eq!(live.get_dimension_value(key, weather), Ok(15.0));
        live.refresh_clocks(ClockSnapshot::new([("overworld".parse().unwrap(), 5)]));
        assert_eq!(live.get_dimension_value(key, weather), Ok(7.5));
        live.refresh_clocks(ClockSnapshot::new([("overworld".parse().unwrap(), 10)]));
        assert_eq!(live.get_dimension_value(key, weather), Ok(0.0));
    }
}
