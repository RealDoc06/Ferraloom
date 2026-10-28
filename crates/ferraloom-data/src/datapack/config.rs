//! Ports of FeatureFlagSet, DataPackConfig and WorldDataConfiguration.
use super::{Error, Identifier};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::collections::{HashSet, JavaHash};
use std::sync::{Arc, LazyLock};

/// The feature universe bootstrapped by FeatureFlags in 26.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureFlags(u8);
impl Default for FeatureFlags {
    fn default() -> Self {
        Self::VANILLA
    }
}
impl FeatureFlags {
    pub const EMPTY: Self = Self(0);
    pub const VANILLA: Self = Self(1);
    pub const ALL: Self = Self(15);
    const NAMES: [&'static str; 4] = [
        "minecraft:vanilla",
        "minecraft:trade_rebalance",
        "minecraft:redstone_experiments",
        "minecraft:minecart_improvements",
    ];
    pub fn from_names(names: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Self, Error> {
        let mut flags = Self::EMPTY;
        for name in names {
            let id: Identifier = name.as_ref().parse()?;
            let index = Self::NAMES
                .iter()
                .position(|name| *name == id.to_string())
                .ok_or_else(|| Error::InvalidData(format!("Unknown feature id: {id}")))?;
            flags.0 |= 1 << index;
        }
        Ok(flags)
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn join(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub fn subtract(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
    /// Port of FeatureFlagRegistry#toNames. Native serialization traverses a JDK
    /// HashSet of Identifier values, populated in feature registration order.
    pub fn names(self) -> Vec<String> {
        let mut names = HashSet::new();
        for (index, name) in Self::NAMES.iter().enumerate() {
            if self.0 & (1 << index) != 0 {
                names.insert(name.parse::<Identifier>().expect("registered feature id"));
            }
        }
        names.iter().map(ToString::to_string).collect()
    }
}
impl JavaHash for Identifier {
    fn java_hash_code(&self) -> i32 {
        self.namespace()
            .java_hash_code()
            .wrapping_mul(31)
            .wrapping_add(self.path().java_hash_code())
    }
    fn java_compare(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
pub struct DataPackConfig {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
}
impl Default for DataPackConfig {
    fn default() -> Self {
        Self {
            enabled: vec!["vanilla".into()],
            disabled: Vec::new(),
        }
    }
}
impl DataPackConfig {
    fn default_reference() -> Arc<Self> {
        // DataPackConfig has identity equality in Java. Optional-field encoding
        // omits only DEFAULT, not a separately decoded equal-valued instance.
        static DEFAULT: LazyLock<Arc<DataPackConfig>> =
            LazyLock::new(|| Arc::new(DataPackConfig::default()));
        DEFAULT.clone()
    }

    /// Port of DedicatedServerProperties#initialDataPackConfiguration. Empty
    /// entries are retained, and Guava Splitter trims whitespace around each id.
    pub fn from_properties(enabled: &str, disabled: &str) -> Self {
        let split = |value: &str| value.split(',').map(str::trim).map(str::to_owned).collect();
        Self {
            enabled: split(enabled),
            disabled: split(disabled),
        }
    }
    fn from_nbt(value: &Tag) -> Option<Self> {
        let value = value.as_compound()?;
        Some(Self {
            enabled: string_list(value.get("Enabled")?)?,
            disabled: string_list(value.get("Disabled")?)?,
        })
    }
    fn to_nbt(&self) -> CompoundTag {
        let mut value = CompoundTag::new();
        value.insert("Enabled", strings(&self.enabled));
        value.insert("Disabled", strings(&self.disabled));
        value
    }
}

#[derive(Clone, Debug)]
pub struct WorldDataConfiguration {
    pub data_packs: Arc<DataPackConfig>,
    pub enabled_features: FeatureFlags,
}
impl Default for WorldDataConfiguration {
    fn default() -> Self {
        Self {
            data_packs: DataPackConfig::default_reference(),
            enabled_features: FeatureFlags::default(),
        }
    }
}
impl WorldDataConfiguration {
    /// Port of WorldDataConfiguration.CODEC's independent lenient optional
    /// fields, as used by LevelStorageSource#readDataConfig before registry load.
    pub fn from_nbt(value: &CompoundTag) -> Self {
        let data_packs = value
            .get("DataPacks")
            .and_then(DataPackConfig::from_nbt)
            .map(Arc::new)
            .unwrap_or_else(DataPackConfig::default_reference);
        let enabled_features = value
            .get("enabled_features")
            .and_then(string_list)
            .and_then(|names| FeatureFlags::from_names(names).ok())
            .unwrap_or_default();
        Self {
            data_packs,
            enabled_features,
        }
    }
    /// PrimaryLevelData#encode writes the normalized selection into the same
    /// level.dat record, retaining every unrelated field.
    pub fn write_nbt(&self, value: &mut CompoundTag) {
        value.remove("DataPacks");
        value.remove("enabled_features");
        if !Arc::ptr_eq(&self.data_packs, &DataPackConfig::default_reference()) {
            value.insert("DataPacks", Tag::Compound(self.data_packs.to_nbt()));
        }
        if self.enabled_features != FeatureFlags::VANILLA {
            value.insert("enabled_features", strings(&self.enabled_features.names()));
        }
    }
}
fn strings(values: &[String]) -> Tag {
    Tag::List(ListTag(
        values
            .iter()
            .map(|value| Tag::String(value.clone().into()))
            .collect(),
    ))
}
fn string_list(value: &Tag) -> Option<Vec<String>> {
    match value {
        Tag::List(values) => values
            .iter()
            .map(|value| value.as_string().map(|value| value.to_string_lossy()))
            .collect(),
        Tag::ByteArray(values) if values.is_empty() => Some(Vec::new()),
        Tag::IntArray(values) if values.is_empty() => Some(Vec::new()),
        Tag::LongArray(values) if values.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn expected_strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }
    fn check(config: &WorldDataConfiguration, row: &Value) {
        assert_eq!(config.data_packs.enabled, expected_strings(&row["enabled"]));
        assert_eq!(
            config.data_packs.disabled,
            expected_strings(&row["disabled"])
        );
        assert_eq!(
            config.enabled_features.names(),
            expected_strings(&row["features"])
        );
        let mut saved = CompoundTag::new();
        config.write_nbt(&mut saved);
        let expected = ferraloom_nbt::parse_compound(row["encoded"].as_str().unwrap()).unwrap();
        assert_eq!(saved, expected, "{row}");
    }
    #[test]
    fn native_world_data_configuration_defaults_features_encoding_and_properties() {
        let rows: Value =
            serde_json::from_str(include_str!("../../tests/java/datapacks/config.json")).unwrap();
        assert_eq!(rows["decode"].as_array().unwrap().len(), 22);
        for row in rows["decode"].as_array().unwrap() {
            let input = ferraloom_nbt::parse_compound(row["input"].as_str().unwrap()).unwrap();
            check(&WorldDataConfiguration::from_nbt(&input), row);
        }
        assert_eq!(rows["properties"].as_array().unwrap().len(), 6);
        for row in rows["properties"].as_array().unwrap() {
            let config = DataPackConfig::from_properties(
                row["input"][0].as_str().unwrap(),
                row["input"][1].as_str().unwrap(),
            );
            assert_eq!(config.enabled, expected_strings(&row["enabled"]));
            assert_eq!(config.disabled, expected_strings(&row["disabled"]));
        }
        assert_eq!(rows["flags"].as_array().unwrap().len(), 16);
        for row in rows["flags"].as_array().unwrap() {
            let features = FeatureFlags::from_names(expected_strings(&row["input"])).unwrap();
            check(
                &WorldDataConfiguration {
                    enabled_features: features,
                    ..Default::default()
                },
                row,
            );
        }
    }
}
