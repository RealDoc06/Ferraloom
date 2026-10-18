//! Timeline's immutable argument tracks. Clock values are supplied by the live
//! server owner; no timeline sampler advances time or owns an independent clock.
use super::{AttributeEntry, ScalarValue, UnsupportedAttribute};
use crate::{Error, Identifier, codec::*};
use ferraloom_data::registries::environment_attribute::Id;
use ferraloom_util::java;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct ClockSnapshot(BTreeMap<Identifier, i64>);
impl ClockSnapshot {
    pub fn new(values: impl IntoIterator<Item = (Identifier, i64)>) -> Self {
        Self(values.into_iter().collect())
    }
    pub fn ticks(&self, id: &Identifier) -> Option<i64> {
        self.0.get(id).copied()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SampleError {
    Unsupported(UnsupportedAttribute),
    MissingClock(Identifier),
}
#[derive(Clone, Debug)]
enum Easing {
    Constant,
    Linear,
    Bezier([f32; 4]),
    Unported(String),
}
impl Easing {
    fn decode(value: Option<&Value>) -> Result<Self, Error> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Ok(Self::Linear);
        };
        if let Some(name) = value.as_str() {
            return match name {
                "constant" => Ok(Self::Constant),
                "linear" => Ok(Self::Linear),
                name if [
                    "in_back",
                    "in_bounce",
                    "in_circ",
                    "in_cubic",
                    "in_elastic",
                    "in_expo",
                    "in_quad",
                    "in_quart",
                    "in_quint",
                    "in_sine",
                    "in_out_back",
                    "in_out_bounce",
                    "in_out_circ",
                    "in_out_cubic",
                    "in_out_elastic",
                    "in_out_expo",
                    "in_out_quad",
                    "in_out_quart",
                    "in_out_quint",
                    "in_out_sine",
                    "out_back",
                    "out_bounce",
                    "out_circ",
                    "out_cubic",
                    "out_elastic",
                    "out_expo",
                    "out_quad",
                    "out_quart",
                    "out_quint",
                    "out_sine",
                ]
                .contains(&name) =>
                {
                    Ok(Self::Unported(name.into()))
                }
                _ => Err(Error(format!("unknown easing type {name}"))),
            };
        }
        let controls = list(field(value, "cubic_bezier")?)?;
        if controls.len() != 4 {
            return Err(Error("cubic_bezier requires four controls".into()));
        }
        let controls: [f32; 4] = controls
            .iter()
            .map(float)
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();
        if !(0.0..=1.0).contains(&controls[0]) || !(0.0..=1.0).contains(&controls[2]) {
            return Err(Error("Bezier x controls outside [0;1]".into()));
        }
        Ok(Self::Bezier(controls))
    }
    fn apply(&self, x: f32) -> Option<f32> {
        match self {
            Self::Constant => Some(0.0),
            Self::Linear => Some(x),
            Self::Unported(_) => None,
            Self::Bezier([x1, y1, x2, y2]) => {
                let curve =
                    |v1: f32, v2: f32| [3.0 * v1 - 3.0 * v2 + 1.0, -6.0 * v1 + 3.0 * v2, 3.0 * v1];
                let xc = curve(*x1, *x2);
                let yc = curve(*y1, *y2);
                let sample = |c: [f32; 3], t: f32| ((c[0] * t + c[1]) * t + c[2]) * t;
                let mut t = x;
                for _ in 0..4 {
                    let error = sample(xc, t) - x;
                    if error.abs() < 1.0e-5 {
                        return Some(sample(yc, t));
                    }
                    let gradient = (3.0 * xc[0] * t + 2.0 * xc[1]) * t + xc[2];
                    if gradient < 1.0e-5 {
                        break;
                    }
                    t -= (error / gradient).clamp(-0.25, 0.25);
                }
                let mut lo = 0.0;
                let mut hi = 1.0;
                while lo < hi {
                    let error = sample(xc, t) - x;
                    if error.abs() < 1.0e-5 {
                        return Some(sample(yc, t));
                    }
                    if error < 0.0 {
                        lo = t;
                    } else {
                        hi = t;
                    }
                    t = (hi + lo) / 2.0;
                }
                Some(sample(yc, t))
            }
        }
    }
}
#[derive(Clone, Debug)]
struct Keyframe {
    ticks: i32,
    entry: AttributeEntry,
}
#[derive(Clone, Debug)]
pub struct AttributeTrack {
    attribute: Id,
    keyframes: Vec<Keyframe>,
    segments: Vec<(usize, i32, usize, i32)>,
    period: Option<i32>,
    easing: Easing,
}
impl AttributeTrack {
    pub fn unported_easing(&self) -> Option<&str> {
        if let Easing::Unported(name) = &self.easing {
            Some(name)
        } else {
            None
        }
    }
    fn decode(attribute: Id, value: &Value, period: Option<i32>) -> Result<Self, Error> {
        let modifier = value
            .get("modifier")
            .filter(|value| !value.is_null())
            .map(string)
            .transpose()?
            .unwrap_or("override");
        let mut keyframes = Vec::new();
        let mut same_tick = 0;
        for keyframe in list(field(value, "keyframes")?)? {
            let ticks = int_range(field(keyframe, "ticks")?, 0, i32::MAX)?;
            if let Some(previous) = keyframes.last() {
                let previous: &Keyframe = previous;
                if ticks < previous.ticks {
                    return Err(Error("keyframes must be ordered".into()));
                }
                if ticks == previous.ticks {
                    same_tick += 1;
                } else {
                    same_tick = 0;
                }
                if same_tick >= 2 {
                    return Err(Error("more than two keyframes on same tick".into()));
                }
            }
            if period.is_some_and(|period| ticks > period) {
                return Err(Error("keyframe outside timeline period".into()));
            }
            let entry = AttributeEntry::decode(
                attribute,
                &serde_json::json!({"modifier":modifier,"argument":field(keyframe,"value")?}),
            )?;
            keyframes.push(Keyframe { ticks, entry });
        }
        if keyframes.is_empty() {
            return Err(Error("track has no keyframes".into()));
        }
        let mut segments = Vec::with_capacity(keyframes.len() + 1);
        let last = keyframes.len() - 1;
        if let Some(period) = period {
            segments.push((
                last,
                keyframes[last].ticks.wrapping_sub(period),
                0,
                keyframes[0].ticks,
            ));
        }
        for index in 0..last {
            segments.push((
                index,
                keyframes[index].ticks,
                index + 1,
                keyframes[index + 1].ticks,
            ));
        }
        if let Some(period) = period {
            segments.push((
                last,
                keyframes[last].ticks,
                0,
                keyframes[0].ticks.wrapping_add(period),
            ));
        }
        Ok(Self {
            attribute,
            keyframes,
            segments,
            period,
            easing: Easing::decode(value.get("ease"))?,
        })
    }
    /// Port of KeyframeTrackSampler#sample and bakeSegments, preserving duplicate
    /// keyframes, negative tick floorMod and integer-to-float rounding slots.
    pub fn sample(&self, ticks: i64) -> Result<AttributeEntry, UnsupportedAttribute> {
        let id = self.attribute;
        if self.keyframes.len() == 1 {
            return Ok(self.keyframes[0].entry.clone());
        }
        let sample = self.period.map_or(ticks, |period| {
            java::floor_mod_long(ticks, i64::from(period))
        });
        let &(from, from_tick, to, to_tick) = self
            .segments
            .iter()
            .find(|(_, _, _, to_tick)| sample < i64::from(*to_tick))
            .unwrap_or_else(|| self.segments.last().unwrap());
        let from = &self.keyframes[from];
        let to = &self.keyframes[to];
        if sample <= i64::from(from_tick) {
            return Ok(from.entry.clone());
        }
        if sample >= i64::from(to_tick) {
            return Ok(to.entry.clone());
        }
        let alpha = (sample - i64::from(from_tick)) as f32 / to_tick.wrapping_sub(from_tick) as f32;
        let alpha = self.easing.apply(alpha).ok_or(UnsupportedAttribute(id))?;
        AttributeEntry::interpolate(id, &from.entry, &to.entry, alpha)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TimeMarker {
    pub ticks: i32,
    pub show_in_commands: bool,
}
#[derive(Clone, Debug)]
pub struct Timeline {
    markers: BTreeMap<Identifier, TimeMarker>,
    clock: Identifier,
    period: Option<i32>,
    tracks: Vec<Option<AttributeTrack>>,
}
impl Timeline {
    pub fn markers(&self) -> &BTreeMap<Identifier, TimeMarker> {
        &self.markers
    }
    pub fn attributes(&self) -> impl Iterator<Item = Id> + '_ {
        ferraloom_data::registries::environment_attribute::ALL
            .iter()
            .copied()
            .filter(|id| self.tracks[id.protocol_id() as usize].is_some())
    }
    pub fn clock(&self) -> &Identifier {
        &self.clock
    }
    pub fn track(&self, id: Id) -> Option<&AttributeTrack> {
        self.tracks[id.protocol_id() as usize].as_ref()
    }
    pub fn period(&self) -> Option<i32> {
        self.period
    }
    pub fn decode(value: &Value) -> Result<Self, Error> {
        let period = value
            .get("period_ticks")
            .filter(|value| !value.is_null())
            .map(|value| int_range(value, 1, i32::MAX))
            .transpose()?;
        let mut tracks = vec![None; ferraloom_data::registries::environment_attribute::ALL.len()];
        if let Some(values) = value.get("tracks").filter(|value| !value.is_null()) {
            for (name, value) in values
                .as_object()
                .ok_or_else(|| Error("expected timeline tracks map".into()))?
            {
                let name: Identifier = name.parse()?;
                let id = Id::from_name(&name.to_string())
                    .ok_or_else(|| Error(format!("unknown track attribute {name}")))?;
                let slot = &mut tracks[id.protocol_id() as usize];
                if slot.is_some() {
                    return Err(Error(format!("duplicate track attribute {name}")));
                }
                *slot = Some(AttributeTrack::decode(id, value, period)?);
            }
        }
        let mut markers = BTreeMap::new();
        if let Some(values) = value.get("time_markers").filter(|value| !value.is_null()) {
            for (name, value) in values
                .as_object()
                .ok_or_else(|| Error("expected time markers map".into()))?
            {
                let name: Identifier = name.parse()?;
                let (ticks, show_in_commands) = if value.is_object() {
                    (
                        int_range(field(value, "ticks")?, 0, i32::MAX)?,
                        optional(value, "show_in_commands", false, boolean)?,
                    )
                } else {
                    (int_range(value, 0, i32::MAX)?, false)
                };
                if period.is_some_and(|period| ticks >= period) {
                    return Err(Error("time marker outside timeline period".into()));
                }
                if markers
                    .insert(
                        name,
                        TimeMarker {
                            ticks,
                            show_in_commands,
                        },
                    )
                    .is_some()
                {
                    return Err(Error("duplicate normalized time marker".into()));
                }
            }
        }
        Ok(Self {
            markers,
            clock: id(field(value, "clock")?)?,
            period,
            tracks,
        })
    }
    pub fn apply(
        &self,
        id: Id,
        base: ScalarValue,
        clocks: &ClockSnapshot,
    ) -> Result<ScalarValue, SampleError> {
        let Some(track) = &self.tracks[id.protocol_id() as usize] else {
            return Ok(base);
        };
        let ticks = clocks
            .ticks(&self.clock)
            .ok_or_else(|| SampleError::MissingClock(self.clock.clone()))?;
        track
            .sample(ticks)
            .and_then(|entry| entry.apply(id, base))
            .map_err(SampleError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyframe_period_edges_duplicates_and_bezier_match_real_java() {
        let id = Id::VISUAL_SUN_ANGLE;
        let mut count = 0;
        for line in include_str!("../../tests/java/environment/timelines.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let period: i32 = fields[0].parse().unwrap();
            let ticks = fields[1].parse().unwrap();
            let json: Value = serde_json::from_str(fields[2]).unwrap();
            let track = AttributeTrack::decode(id, &json, (period > 0).then_some(period)).unwrap();
            let entry = track.sample(ticks).unwrap();
            let ScalarValue::Float(value) = entry.apply(id, ScalarValue::Float(0.0)).unwrap()
            else {
                panic!("float track")
            };
            assert_eq!(value.to_bits(), fields[3].parse::<u32>().unwrap(), "{line}");
            count += 1;
        }
        assert_eq!(count, 432);
    }
    #[test]
    fn timeline_uses_only_captured_clock_values_and_requires_missing_context() {
        let key = Id::GAMEPLAY_FAST_LAVA;
        let timeline=Timeline::decode(&serde_json::json!({"clock":"overworld","period_ticks":20,"time_markers":{"noon":{"ticks":10,"show_in_commands":true}},"tracks":{"gameplay/fast_lava":{"keyframes":[{"ticks":5,"value":true},{"ticks":15,"value":false}]}}})).unwrap();
        assert_eq!(timeline.markers()[&"noon".parse().unwrap()].ticks, 10);
        let base = ScalarValue::Boolean(false);
        assert!(matches!(
            timeline.apply(key, base, &ClockSnapshot::default()),
            Err(SampleError::MissingClock(_))
        ));
        for (tick, expected) in [
            (0, false),
            (5, true),
            (14, true),
            (15, false),
            (25, true),
            (-5, false),
        ] {
            let clocks = ClockSnapshot::new([("overworld".parse().unwrap(), tick)]);
            assert_eq!(
                timeline.apply(key, base, &clocks),
                Ok(ScalarValue::Boolean(expected))
            );
        }
        assert!(Timeline::decode(&serde_json::json!({"clock":"overworld","period_ticks":20,"time_markers":{"noon":20}})).is_err());
    }
}
