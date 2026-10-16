//! Java advancement JSON is a disk format, independent of runtime registry lookup.
//! Unknown advancements and criteria are retained here, not registered as triggers.

use super::{AdvancementDataError, AdvancementProgressMap, CriterionProgress};
use pumpkin_data::Advancement;
use pumpkin_util::identifier::Identifier;
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

const DATE_FORMAT: &[time::format_description::FormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
);

#[derive(Clone, Copy)]
pub(super) enum LoadState {
    Unloaded,
    // Absence was checked in an existing, readable directory.
    New,
    Loaded,
    // Player file reads and writes are disabled.
    Disabled,
    Failed,
}

#[derive(Clone, Serialize)]
pub(super) struct AdvancementFile {
    #[serde(rename = "DataVersion")]
    data_version: i32,
    #[serde(flatten)]
    advancements: BTreeMap<String, DiskProgress>,
}

impl Default for AdvancementFile {
    fn default() -> Self {
        Self {
            data_version: MAXIMUM_SUPPORTED_WORLD_DATA_VERSION,
            advancements: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct DiskProgress {
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_criteria"
    )]
    criteria: BTreeMap<String, Timestamp>,
    // Official 26.3's codec defaults a missing done to true, then recomputes
    // known progress from requirements. Unknown progress keeps the disk value.
    #[serde(default = "default_done")]
    done: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

const fn default_done() -> bool {
    true
}

/// Keep the original spelling/offset when the instant has not changed.
#[derive(Clone)]
struct Timestamp {
    instant: SystemTime,
    text: String,
}

impl Timestamp {
    fn parse(text: String) -> Result<Self, AdvancementDataError> {
        let date = OffsetDateTime::parse(&text, DATE_FORMAT)
            .map_err(|error| AdvancementDataError::Timestamp(error.to_string()))?;
        validate_date(date)?;
        let seconds = date.unix_timestamp();
        let duration = Duration::from_secs(seconds.unsigned_abs());
        let instant = if seconds < 0 {
            UNIX_EPOCH.checked_sub(duration)
        } else {
            UNIX_EPOCH.checked_add(duration)
        }
        .ok_or_else(|| AdvancementDataError::Timestamp("outside SystemTime range".into()))?;
        Ok(Self { instant, text })
    }

    fn from_time(instant: SystemTime) -> Result<Self, AdvancementDataError> {
        let nanos = match instant.duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos() as i128,
            Err(error) => -(error.duration().as_nanos() as i128),
        };
        let date = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|error| AdvancementDataError::Timestamp(error.to_string()))?;
        validate_date(date)?;
        // Java's disk format has whole-second precision. UTC is a valid numeric
        // offset and avoids depending on process-local timezone discovery.
        let text = date
            .format(DATE_FORMAT)
            .map_err(|error| AdvancementDataError::Timestamp(error.to_string()))?;
        Ok(Self { instant, text })
    }
}

fn validate_date(date: OffsetDateTime) -> Result<(), AdvancementDataError> {
    if !(1..=9999).contains(&date.year())
        || date.offset().whole_seconds().unsigned_abs() > 18 * 3600
    {
        return Err(AdvancementDataError::Timestamp(
            "requires an AD year from 0001 to 9999 and a Java-compatible offset".into(),
        ));
    }
    Ok(())
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

// Reject duplicate advancement IDs, versions and criteria instead of silently
// selecting one history. Parse directly from bytes, not a lossy Value map.
impl<'de> Deserialize<'de> for AdvancementFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FileVisitor;
        impl<'de> Visitor<'de> for FileVisitor {
            type Value = AdvancementFile;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a versioned advancement object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut data_version = None;
                let mut advancements = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key == "DataVersion" {
                        if data_version.is_some() {
                            return Err(serde::de::Error::duplicate_field("DataVersion"));
                        }
                        data_version = Some(map.next_value()?);
                    } else {
                        if !valid_id(&key) || advancements.contains_key(&key) {
                            return Err(serde::de::Error::custom(format!(
                                "invalid or duplicate advancement ID {key}"
                            )));
                        }
                        advancements.insert(key, map.next_value()?);
                    }
                }
                Ok(AdvancementFile {
                    data_version: data_version
                        .ok_or_else(|| serde::de::Error::missing_field("DataVersion"))?,
                    advancements,
                })
            }
        }
        deserializer.deserialize_map(FileVisitor)
    }
}

fn unique_criteria<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Timestamp>, D::Error> {
    struct CriteriaVisitor;
    impl<'de> Visitor<'de> for CriteriaVisitor {
        type Value = BTreeMap<String, Timestamp>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map of unique criteria to timestamps")
        }

        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut criteria = BTreeMap::new();
            while let Some((name, timestamp)) = map.next_entry::<String, Timestamp>()? {
                if criteria.insert(name.clone(), timestamp).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate criterion {name}"
                    )));
                }
            }
            Ok(criteria)
        }
    }
    deserializer.deserialize_map(CriteriaVisitor)
}

fn valid_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && Identifier::is_valid_namespace(namespace)
        && Identifier::is_valid_path(path)
}

// This exact old Rust shape is diagnostic only: it has no done field for
// unknown definitions, and migrating timestamps loses subsecond precision.
// Refuse it without touching the original until an explicit migration exists.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTimestamp {
    secs_since_epoch: u64,
    nanos_since_epoch: u32,
}

pub(super) fn decode(bytes: &[u8]) -> Result<AdvancementFile, AdvancementDataError> {
    let shape: serde_json::Value =
        serde_json::from_slice(bytes).map_err(AdvancementDataError::Json)?;
    if !shape
        .as_object()
        .is_some_and(|map| map.contains_key("DataVersion"))
    {
        type LegacyFile = BTreeMap<String, BTreeMap<String, Option<LegacyTimestamp>>>;
        if let Ok(legacy) = serde_json::from_slice::<LegacyFile>(bytes)
            && !legacy.is_empty()
            && legacy.keys().all(|id| valid_id(id))
            && legacy
                .values()
                .flat_map(BTreeMap::values)
                .any(Option::is_some)
            && legacy
                .values()
                .flat_map(BTreeMap::values)
                .flatten()
                .all(|time| {
                    time.nanos_since_epoch < 1_000_000_000
                        && UNIX_EPOCH
                            .checked_add(Duration::new(
                                time.secs_since_epoch,
                                time.nanos_since_epoch,
                            ))
                            .is_some()
                })
        {
            return Err(AdvancementDataError::LegacyRustFormat);
        }
        return Err(AdvancementDataError::Schema(
            "missing DataVersion; no unambiguous supported schema".into(),
        ));
    }
    let disk: AdvancementFile =
        serde_json::from_slice(bytes).map_err(AdvancementDataError::Json)?;
    if disk.data_version != MAXIMUM_SUPPORTED_WORLD_DATA_VERSION {
        return Err(AdvancementDataError::UnsupportedVersion(disk.data_version));
    }
    Ok(disk)
}

/// Only a missing leaf in an existing directory represents a new player.
/// Parent directory symlinks are allowed; a symlinked leaf is never overwritten.
pub(super) fn read_file(path: &Path) -> Result<Option<AdvancementFile>, AdvancementDataError> {
    for parent in path
        .ancestors()
        .skip(1)
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let metadata = fs::metadata(parent).map_err(AdvancementDataError::Io)?;
        if !metadata.file_type().is_dir() {
            return Err(AdvancementDataError::Io(io::Error::other(format!(
                "advancement parent is not a directory: {}",
                parent.display()
            ))));
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let bytes = fs::read(path).map_err(AdvancementDataError::Io)?;
            decode(&bytes).map(Some)
        }
        Ok(_) => Err(AdvancementDataError::Io(io::Error::other(
            "advancement path is not a regular file",
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AdvancementDataError::Io(error)),
    }
}

impl AdvancementFile {
    pub(super) fn runtime(&self) -> AdvancementProgressMap {
        let mut runtime = AdvancementProgressMap::default();
        for (id, saved) in &self.advancements {
            if let Some(advancement) = Advancement::from_minecraft_name(id) {
                let progress = runtime.get_mut_or_start_progress(advancement);
                for (name, criterion) in &mut progress.criteria {
                    if let Some(timestamp) = saved.criteria.get(name.as_ref()) {
                        *criterion = CriterionProgress(Some(timestamp.instant));
                    }
                }
            }
        }
        runtime
    }

    pub(super) fn merge_runtime(
        &mut self,
        runtime: &AdvancementProgressMap,
    ) -> Result<(), AdvancementDataError> {
        for (advancement, progress) in &runtime.map {
            let id = advancement.id.to_string();
            let saved = self.advancements.entry(id.clone()).or_default();
            for (name, criterion) in &progress.criteria {
                let name = name.as_ref();
                if let Some(instant) = criterion.0 {
                    if !saved
                        .criteria
                        .get(name)
                        .is_some_and(|timestamp| timestamp.instant == instant)
                    {
                        saved
                            .criteria
                            .insert(name.to_owned(), Timestamp::from_time(instant)?);
                    }
                } else {
                    saved.criteria.remove(name);
                }
            }
            saved.done = progress.is_done();
            if saved.criteria.is_empty() && saved.extra.is_empty() {
                self.advancements.remove(&id);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
