//! Strict, version-pinned disk DTO. Unknown names remain on disk and never gain
//! runtime IDs. Unsupported shapes refuse admission instead of discarding data.

use super::StatisticsDataError;
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{self, Read},
    path::Path,
};

// Explicit resource boundary, not a claim about vanilla's decoder limits.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct StatisticsFile {
    #[serde(rename = "DataVersion")]
    data_version: i32,
    pub stats: BTreeMap<String, BTreeMap<String, i32>>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Default for StatisticsFile {
    fn default() -> Self {
        Self {
            data_version: MAXIMUM_SUPPORTED_WORLD_DATA_VERSION,
            stats: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

// Reject duplicate keys throughout the tree before Value or the DTO can choose
// one of two histories. This includes opaque extension objects.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = UniqueValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|n| UniqueValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.into())))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!("duplicate key {key}")));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

fn valid_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"/_.-".contains(&b))
}

pub(super) fn encode(disk: &StatisticsFile) -> Result<Vec<u8>, StatisticsDataError> {
    let bytes = serde_json::to_vec_pretty(disk)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(StatisticsDataError::Schema(
            "statistics snapshot exceeds 16 MiB".into(),
        ));
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<StatisticsFile, StatisticsDataError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(StatisticsDataError::Schema(
            "statistics file exceeds 16 MiB".into(),
        ));
    }
    let UniqueValue(value) = serde_json::from_slice(bytes)?;
    let disk: StatisticsFile = serde_json::from_value(value)?;
    if disk.data_version != MAXIMUM_SUPPORTED_WORLD_DATA_VERSION {
        return Err(StatisticsDataError::UnsupportedVersion(disk.data_version));
    }
    for (category, stats) in &disk.stats {
        if !valid_id(category) || stats.keys().any(|key| !valid_id(key)) {
            return Err(StatisticsDataError::Schema(
                "expected namespaced statistic identifiers".into(),
            ));
        }
    }
    Ok(disk)
}

/// Only an absent leaf in real existing directories is a new statistics file.
/// Missing parents, symlinks, directories and unreadable files are failures.
pub(super) fn read_file(path: &Path) -> Result<Option<StatisticsFile>, StatisticsDataError> {
    for parent in path
        .ancestors()
        .skip(1)
        .filter(|p| !p.as_os_str().is_empty())
    {
        if !fs::symlink_metadata(parent)?.file_type().is_dir() {
            return Err(io::Error::other(format!(
                "statistics parent is not a real directory: {}",
                parent.display()
            ))
            .into());
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            decode(&bytes).map(Some)
        }
        Ok(_) => Err(io::Error::other("statistics path is not a regular file").into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
