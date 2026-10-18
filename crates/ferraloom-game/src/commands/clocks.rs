//! Port of ServerClockManager and TimeCommand's clock operations (26.3).
use super::{
    PreparedCommand,
    arguments::ArgumentValue,
    builtins::{self, CommandResult},
    exceptions::error,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::datapack::ResourceManager;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    chat::{Argument, Component, Numeric},
    codec::VarLong,
    play::ClockNetworkState,
};
use ferraloom_storage::{Error, SavedDataCodec};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockState {
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
    pub paused: bool,
}
impl Default for ClockState {
    fn default() -> Self {
        Self {
            total_ticks: 0,
            partial_tick: 0.,
            rate: 1.,
            paused: false,
        }
    }
}
impl ClockState {
    pub fn tick(&mut self) {
        if !self.paused {
            self.partial_tick += self.rate;
            let full = self.partial_tick.floor() as i32;
            self.partial_tick -= full as f32;
            self.total_ticks = self.total_ticks.wrapping_add(i64::from(full));
        }
    }
    pub fn network(self, advance_time: bool) -> ClockNetworkState {
        ClockNetworkState {
            total_ticks: VarLong(self.total_ticks),
            partial_tick: self.partial_tick,
            rate: if self.paused || !advance_time {
                0.
            } else {
                self.rate
            },
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackedClockStates(pub BTreeMap<String, ClockState>);
impl PackedClockStates {
    pub const STORAGE_ID: &'static str = "minecraft:world_clocks";
}
impl SavedDataCodec for PackedClockStates {
    fn decode(tag: &Tag) -> ferraloom_storage::Result<Self> {
        let map = tag
            .as_compound()
            .ok_or_else(|| Error::Invalid("clock states are not a map".into()))?;
        let mut states = BTreeMap::new();
        for (id, value) in map.iter() {
            let state = value
                .as_compound()
                .ok_or_else(|| Error::Invalid("clock state is not a compound".into()))?;
            let rate = state.get_float_or("rate", 1.);
            if rate <= 0. || !rate.is_finite() {
                return Err(Error::Invalid("clock rate must be positive".into()));
            }
            states.insert(
                id.to_string_lossy(),
                ClockState {
                    total_ticks: state
                        .get_long("total_ticks")
                        .ok_or_else(|| Error::Invalid("clock missing total_ticks".into()))?,
                    partial_tick: state.get_float_or("partial_tick", 0.),
                    rate,
                    paused: state.get_boolean_or("paused", false),
                },
            );
        }
        Ok(Self(states))
    }
    fn encode(&self) -> ferraloom_storage::Result<Tag> {
        let mut map = CompoundTag::new();
        for (id, state) in &self.0 {
            let mut value = CompoundTag::new();
            value.insert("total_ticks", Tag::Long(state.total_ticks));
            if state.partial_tick != 0. {
                value.insert("partial_tick", Tag::Float(state.partial_tick));
            }
            if state.rate != 1. {
                value.insert("rate", Tag::Float(state.rate));
            }
            if state.paused {
                value.insert("paused", Tag::Byte(1));
            }
            map.insert(id.as_str(), Tag::Compound(value));
        }
        Ok(Tag::Compound(map))
    }
}
#[derive(Clone, Debug)]
struct Timeline {
    clock: String,
    period: Option<i32>,
}
#[derive(Clone, Debug)]
struct TimeMarker {
    ticks: i32,
    period: Option<i32>,
    show_in_commands: bool,
}
/// Owns canonical clock state; the runtime handles packet audiences and storage IO.
pub struct ServerClockManager {
    pub states: PackedClockStates,
    timelines: BTreeMap<String, Timeline>,
    markers: BTreeMap<(String, String), TimeMarker>,
}
impl ServerClockManager {
    pub fn load(resources: &ResourceManager, saved: PackedClockStates) -> Result<Self, String> {
        let mut states = PackedClockStates::default();
        for id in resources.list("world_clock/", ".json") {
            let name = format!(
                "{}:{}",
                id.namespace,
                id.path
                    .strip_prefix("world_clock/")
                    .unwrap()
                    .strip_suffix(".json")
                    .unwrap()
            );
            states.0.insert(
                name.clone(),
                saved.0.get(&name).copied().unwrap_or_default(),
            );
        }
        let mut timelines = BTreeMap::new();
        let mut markers = BTreeMap::new();
        for id in resources.list("timeline/", ".json") {
            let json = resources
                .get(&id)
                .ok_or("missing timeline")?
                .json()
                .map_err(|e| e.to_string())?;
            let clock = json["clock"]
                .as_str()
                .ok_or("timeline missing clock")?
                .to_owned();
            if !states.0.contains_key(&clock) {
                return Err(format!("unknown clock {clock}"));
            }
            let period = json["period_ticks"].as_i64().map(|value| value as i32);
            if period.is_some_and(|period| period <= 0) {
                return Err("timeline period must be positive".into());
            }
            if let Some(time_markers) = json["time_markers"].as_object() {
                for (name, value) in time_markers {
                    let (ticks, show_in_commands) = if let Some(ticks) = value.as_i64() {
                        (ticks as i32, false)
                    } else {
                        (
                            value["ticks"].as_i64().ok_or("marker missing ticks")? as i32,
                            value["show_in_commands"].as_bool().unwrap_or(false),
                        )
                    };
                    if period.is_some_and(|period| ticks < 0 || ticks >= period) {
                        return Err("time marker outside timeline".into());
                    }
                    if markers
                        .insert(
                            (clock.clone(), name.clone()),
                            TimeMarker {
                                ticks,
                                period,
                                show_in_commands,
                            },
                        )
                        .is_some()
                    {
                        return Err(format!("duplicate clock marker {name}"));
                    }
                }
            }
            let name = format!(
                "{}:{}",
                id.namespace,
                id.path
                    .strip_prefix("timeline/")
                    .unwrap()
                    .strip_suffix(".json")
                    .unwrap()
            );
            timelines.insert(name, Timeline { clock, period });
        }
        Ok(Self {
            states,
            timelines,
            markers,
        })
    }
    pub fn tick(&mut self, advance_time: bool) {
        if advance_time {
            for clock in self.states.0.values_mut() {
                clock.tick();
            }
        }
    }
    pub fn time_markers<'a>(&'a self, clock: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.markers
            .iter()
            .filter(move |((owner, _), marker)| owner == clock && marker.show_in_commands)
            .map(|((_, name), _)| name.as_str())
    }
    /// Returns the modified clock even when move-to-marker reports an error: vanilla still synchronizes it.
    pub fn command(
        &mut self,
        command: &PreparedCommand,
        game_time: i64,
    ) -> (
        Result<CommandResult, CommandSyntaxException>,
        Option<String>,
    ) {
        let clock = command
            .string("clock")
            .unwrap_or("minecraft:overworld")
            .to_owned();
        let mut changed = false;
        let result = (|| {
            if command.literals.iter().any(|literal| literal == "gametime") {
                return Ok(CommandResult::success(
                    wrap_time(game_time),
                    Component::translatable("commands.time.query.gametime", vec![long(game_time)]),
                    false,
                ));
            }
            let state = self.states.0.get_mut(&clock).ok_or_else(|| {
                error(
                    "argument.resource.not_found",
                    &[clock.clone(), "minecraft:world_clock".into()],
                )
            })?;
            if command.literals.iter().any(|literal| literal == "set") {
                if let Some(time) = command.integer("time") {
                    if state.total_ticks == i64::from(time) {
                        return Err(error(
                            "commands.time.set.already_at_time",
                            &[clock.clone(), time.to_string()],
                        ));
                    }
                    state.total_ticks = i64::from(time);
                    state.partial_tick = 0.;
                    changed = true;
                    return Ok(CommandResult::success(
                        time,
                        Component::translatable(
                            "commands.time.set.absolute",
                            vec![builtins::string(&clock), builtins::integer(time)],
                        ),
                        true,
                    ));
                }
                let marker_id = command.string("timemarker").expect("parsed time marker");
                changed = true;
                let marker = self
                    .markers
                    .get(&(clock.clone(), marker_id.to_owned()))
                    .ok_or_else(|| {
                        error(
                            "commands.time.no_time_marker_found",
                            &[marker_id.into(), clock.clone()],
                        )
                    })?;
                let current = marker.period.map_or(state.total_ticks, |period| {
                    state.total_ticks % i64::from(period)
                });
                if current == i64::from(marker.ticks) {
                    return Err(error(
                        "commands.time.set.already_at_time_marker",
                        &[clock.clone(), marker_id.into()],
                    ));
                }
                state.total_ticks = if let Some(period) = marker.period {
                    let duration = i64::from(marker.ticks) - current;
                    state.total_ticks.wrapping_add(if duration > 0 {
                        duration
                    } else {
                        i64::from(period) + duration
                    })
                } else {
                    i64::from(marker.ticks)
                };
                state.partial_tick = 0.;
                return Ok(CommandResult::success(
                    wrap_time(state.total_ticks),
                    Component::translatable(
                        "commands.time.set.time_marker",
                        vec![builtins::string(&clock), builtins::string(marker_id)],
                    ),
                    true,
                ));
            }
            if command.literals.iter().any(|literal| literal == "add") {
                state.total_ticks = state
                    .total_ticks
                    .wrapping_add(i64::from(command.integer("time").unwrap()))
                    .max(0);
                changed = true;
                return Ok(CommandResult::success(
                    wrap_time(state.total_ticks),
                    Component::translatable(
                        "commands.time.set.absolute",
                        vec![builtins::string(&clock), long(state.total_ticks)],
                    ),
                    true,
                ));
            }
            if command
                .literals
                .iter()
                .any(|literal| literal == "pause" || literal == "resume")
            {
                let paused = command.literals.iter().any(|literal| literal == "pause");
                if state.paused == paused {
                    return Err(error(
                        if paused {
                            "commands.time.pause.already_paused"
                        } else {
                            "commands.time.pause.already_running"
                        },
                        std::slice::from_ref(&clock),
                    ));
                }
                state.paused = paused;
                changed = true;
                return Ok(CommandResult::success(
                    1,
                    Component::translatable(
                        if paused {
                            "commands.time.pause"
                        } else {
                            "commands.time.resume"
                        },
                        vec![builtins::string(&clock)],
                    ),
                    true,
                ));
            }
            if let Some(ArgumentValue::Float(rate)) = command.argument("rate") {
                if state.rate == *rate {
                    return Err(error(
                        "commands.time.rate.already_same",
                        &[clock.clone(), rate.to_string()],
                    ));
                }
                state.rate = *rate;
                changed = true;
                return Ok(CommandResult::success(
                    1,
                    Component::translatable(
                        "commands.time.rate",
                        vec![
                            builtins::string(&clock),
                            Argument::Number(Numeric::Float(*rate)),
                        ],
                    ),
                    true,
                ));
            }
            if let Some(timeline) = command.string("timeline") {
                let timeline_data = self.timelines.get(timeline).ok_or_else(|| {
                    error(
                        "argument.resource.not_found",
                        &[timeline.into(), "minecraft:timeline".into()],
                    )
                })?;
                if timeline_data.clock != clock {
                    return Err(error(
                        "commands.time.wrong_timeline_for_clock",
                        &[timeline.into(), clock.clone()],
                    ));
                }
                let repetitions = command
                    .literals
                    .iter()
                    .any(|literal| literal == "repetition");
                let value = if repetitions {
                    timeline_data
                        .period
                        .map_or(0, |period| (state.total_ticks / i64::from(period)) as i32)
                        as i64
                } else {
                    timeline_data.period.map_or(state.total_ticks, |period| {
                        state.total_ticks % i64::from(period)
                    })
                };
                return Ok(CommandResult::success(
                    wrap_time(value),
                    Component::translatable(
                        if repetitions {
                            "commands.time.query.timeline.repetitions"
                        } else {
                            "commands.time.query.timeline"
                        },
                        vec![builtins::string(timeline), long(value)],
                    ),
                    false,
                ));
            }
            Ok(CommandResult::success(
                wrap_time(state.total_ticks),
                Component::translatable(
                    "commands.time.query.absolute",
                    vec![builtins::string(&clock), long(state.total_ticks)],
                ),
                false,
            ))
        })();
        (result, changed.then_some(clock))
    }
}
fn long(value: i64) -> Argument {
    Argument::Number(Numeric::Long(value))
}
fn wrap_time(value: i64) -> i32 {
    (value % 2147483647) as i32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_rate_pause_and_saved_state_preserve_clock_values() {
        let mut state = ClockState {
            rate: 0.5,
            ..ClockState::default()
        };
        state.tick();
        assert_eq!((state.total_ticks, state.partial_tick), (0, 0.5));
        state.tick();
        assert_eq!((state.total_ticks, state.partial_tick), (1, 0.));
        state.paused = true;
        state.tick();
        assert_eq!(state.total_ticks, 1);
        assert_eq!(state.network(true).rate, 0.);
        let states = PackedClockStates(BTreeMap::from([("minecraft:overworld".into(), state)]));
        assert_eq!(
            PackedClockStates::decode(&states.encode().unwrap()).unwrap(),
            states
        );
    }
}
