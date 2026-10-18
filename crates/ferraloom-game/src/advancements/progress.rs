use super::{Identifier, Result};
use crate::loot::codec;
use ferraloom_protocol::play;
use ferraloom_util::collections::HashMap as JavaHashMap;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Port of AdvancementRequirements: AND across groups, OR within each group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdvancementRequirements(pub Vec<Vec<String>>);
impl AdvancementRequirements {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self(codec::list(v, |v| {
            codec::list(v, |v| Ok(codec::string(v)?.to_owned()))
        })?))
    }
    pub fn all_of(names: impl IntoIterator<Item = String>) -> Self {
        Self(names.into_iter().map(|name| vec![name]).collect())
    }
    pub fn any_of(names: impl IntoIterator<Item = String>) -> Self {
        Self(vec![names.into_iter().collect()])
    }
    pub fn names(&self) -> BTreeSet<&str> {
        self.0.iter().flatten().map(String::as_str).collect()
    }
    pub fn validate<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let expected: BTreeSet<_> = names.into_iter().collect();
        if self.0.iter().any(|group| group.is_empty()) && expected.is_empty() {
            return Err(codec::error("Requirement entry cannot be empty"));
        }
        if self.names() != expected {
            return Err(codec::error(
                "Advancement completion requirements did not exactly match specified criteria",
            ));
        }
        Ok(())
    }
    pub fn test(&self, test: impl Fn(&str) -> bool) -> bool {
        !self.0.is_empty()
            && self
                .0
                .iter()
                .all(|group| group.iter().any(|name| test(name)))
    }
    pub fn count(&self, test: impl Fn(&str) -> bool) -> usize {
        self.0
            .iter()
            .filter(|group| group.iter().any(|name| test(name)))
            .count()
    }
}
/// An Instant, retained at nanosecond precision for progress ordering. Network
/// timestamps are milliseconds; persistence uses vanilla's seconds and offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObtainedTime {
    pub seconds: i64,
    pub nanos: u32,
}
impl ObtainedTime {
    pub fn from_millis(millis: i64) -> Self {
        Self {
            seconds: millis.div_euclid(1000),
            nanos: millis.rem_euclid(1000) as u32 * 1_000_000,
        }
    }
    pub fn millis(self) -> i64 {
        self.seconds * 1000 + i64::from(self.nanos / 1_000_000)
    }
    pub fn now() -> Self {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch");
        Self {
            seconds: elapsed.as_secs() as i64,
            nanos: elapsed.subsec_nanos(),
        }
    }
    /// Port of AdvancementProgress.OBTAINED_TIME_CODEC. Offset is supplied by the
    /// save adapter, matching ZoneId.systemDefault at that instant (including DST).
    pub fn format(self, offset_seconds: i32) -> Result<String> {
        if !(-18 * 3600..=18 * 3600).contains(&offset_seconds) {
            return Err(codec::error("invalid time zone offset"));
        }
        let local = self.seconds + i64::from(offset_seconds);
        let days = local.div_euclid(86400);
        let seconds = local.rem_euclid(86400);
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        year += i64::from(month <= 2);
        let year_of_era = if year <= 0 { 1 - year } else { year };
        let year = if year_of_era > 9999 {
            format!("+{year_of_era}")
        } else {
            format!("{year_of_era:04}")
        };
        // DateTimeFormatter Z drops offset seconds and renders a truncated zero as +0000.
        let minutes = offset_seconds.abs() / 60;
        Ok(format!(
            "{year}-{month:02}-{day:02} {:02}:{:02}:{:02} {}{:02}{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            if offset_seconds < 0 && minutes != 0 {
                '-'
            } else {
                '+'
            },
            minutes / 60,
            minutes % 60
        ))
    }
    pub fn parse(value: &str) -> Result<Self> {
        let invalid = || codec::error("invalid obtained time");
        if !value.is_ascii() {
            return Err(invalid());
        }
        let year_end = value.find('-').ok_or_else(invalid)?;
        let extended = value.starts_with('+');
        if !(year_end == 4 && !extended || extended && year_end > 5) {
            return Err(invalid());
        }
        let year_text = if extended {
            &value[1..year_end]
        } else {
            &value[..year_end]
        };
        if !year_text.bytes().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        let mut year = year_text.parse::<i64>().map_err(|_| invalid())?;
        let tail = &value[year_end..];
        if tail.len() != 21
            || &tail[0..1] != "-"
            || &tail[3..4] != "-"
            || &tail[6..7] != " "
            || &tail[9..10] != ":"
            || &tail[12..13] != ":"
            || &tail[15..16] != " "
        {
            return Err(invalid());
        }
        let n = |start, end| -> Result<i64> {
            let part = &tail[start..end];
            if !part.bytes().all(|c| c.is_ascii_digit()) {
                return Err(invalid());
            }
            part.parse().map_err(|_| invalid())
        };
        let month = n(1, 3)?;
        let mut day = n(4, 6)?;
        let hour = n(7, 9)?;
        let minute = n(10, 12)?;
        let second = n(13, 15)?;
        let oh = n(17, 19)?;
        let om = n(19, 21)?;
        if !(1..=999999999).contains(&year)
            || !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 24
            || hour == 24 && (minute != 0 || second != 0)
            || minute > 59
            || second > 59
            || oh > 18
            || om > 59
            || oh == 18 && om != 0
        {
            return Err(invalid());
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let max_day = match month {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        // DateTimeFormatter's SMART resolver clamps the day and carries 24:00.
        day = day.min(max_day);
        year -= i64::from(month <= 2);
        let era = year.div_euclid(400);
        let yoe = year - era * 400;
        let mp = month + if month > 2 { -3 } else { 9 };
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146097 + doe - 719468;
        let sign = match &tail[16..17] {
            "+" => 1,
            "-" => -1,
            _ => return Err(invalid()),
        };
        Ok(Self {
            seconds: days * 86400 + hour * 3600 + minute * 60 + second
                - sign * (oh * 3600 + om * 60),
            nanos: 0,
        })
    }
}
/// Port of CriterionProgress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CriterionProgress {
    pub obtained: Option<ObtainedTime>,
}
impl CriterionProgress {
    pub fn is_done(&self) -> bool {
        self.obtained.is_some()
    }
}
/// Port of AdvancementProgress. Java hash iteration is retained on the wire.
#[derive(Clone, Debug, Default)]
pub struct AdvancementProgress {
    pub criteria: JavaHashMap<String, CriterionProgress>,
    pub requirements: AdvancementRequirements,
}
impl AdvancementProgress {
    pub fn update(&mut self, requirements: &AdvancementRequirements) {
        let names = requirements.names();
        let mut cursor = self.criteria.cursor();
        while let Some((name, _)) = cursor.next_entry() {
            if !names.contains(name.as_str()) {
                cursor.remove();
            }
        }
        // AdvancementRequirements#names uses ObjectOpenHashSet.
        let mut ordered = ferraloom_util::collections::ObjectOpenHashSet::new();
        for name in requirements.0.iter().flatten() {
            ordered.insert(name.clone());
        }
        for name in ordered.iter() {
            if !self.criteria.contains_key(name) {
                self.criteria
                    .insert(name.clone(), CriterionProgress::default());
            }
        }
        self.requirements = requirements.clone();
    }
    pub fn criterion_done(&self, name: &str) -> bool {
        self.criteria
            .peek(&name.to_owned())
            .is_some_and(CriterionProgress::is_done)
    }
    pub fn is_done(&self) -> bool {
        self.requirements.test(|name| self.criterion_done(name))
    }
    pub fn has_progress(&self) -> bool {
        self.criteria.iter().any(|(_, c)| c.is_done())
    }
    pub fn grant(&mut self, name: &str, now: ObtainedTime) -> bool {
        let Some(c) = self.criteria.get_mut(&name.to_owned()) else {
            return false;
        };
        if c.is_done() {
            return false;
        }
        c.obtained = Some(now);
        true
    }
    pub fn revoke(&mut self, name: &str) -> bool {
        self.criteria
            .get_mut(&name.to_owned())
            .and_then(|c| c.obtained.take())
            .is_some()
    }
    pub fn percent(&self) -> f32 {
        if self.criteria.is_empty() {
            0.0
        } else {
            self.requirements.count(|name| self.criterion_done(name)) as f32
                / self.requirements.0.len() as f32
        }
    }
    pub fn first_progress_date(&self) -> Option<ObtainedTime> {
        self.criteria.iter().filter_map(|(_, c)| c.obtained).min()
    }
    pub fn decode(v: &Value) -> Result<Self> {
        let mut result = Self::default();
        if let Some(criteria) = codec::object(v)?.get("criteria").filter(|v| !v.is_null()) {
            for (name, time) in codec::object(criteria)? {
                result.criteria.insert(
                    name.clone(),
                    CriterionProgress {
                        obtained: Some(ObtainedTime::parse(codec::string(time)?)?),
                    },
                );
            }
        }
        // CRITERIA_CODEC first collects with Collectors.toMap, then the progress
        // constructor copies that HashMap with putMapEntries' exact pre-sizing.
        if !result.criteria.is_empty() {
            let mut copied = JavaHashMap::with_capacity((result.criteria.len() * 4).div_ceil(3));
            for (name, progress) in std::mem::take(&mut result.criteria) {
                copied.insert(name, progress);
            }
            result.criteria = copied;
        }
        if let Some(done) = codec::object(v)?.get("done").filter(|v| !v.is_null()) {
            codec::boolean(done)?;
        }
        Ok(result)
    }
    pub fn to_json(&self, offset: impl Fn(ObtainedTime) -> i32) -> Result<Value> {
        let mut criteria = serde_json::Map::new();
        for (name, c) in self.criteria.iter() {
            if let Some(time) = c.obtained {
                criteria.insert(name.clone(), Value::String(time.format(offset(time))?));
            }
        }
        let mut result = json!({"done":self.is_done()});
        if !criteria.is_empty() {
            result
                .as_object_mut()
                .expect("object")
                .insert("criteria".into(), Value::Object(criteria));
        }
        Ok(result)
    }
    pub fn network(&self, id: Identifier) -> play::AdvancementProgress {
        play::AdvancementProgress {
            id,
            criteria: self
                .criteria
                .iter()
                .map(|(name, c)| play::CriterionProgress {
                    name: name.clone(),
                    obtained: c.obtained.map(ObtainedTime::millis),
                })
                .collect(),
        }
    }
}
