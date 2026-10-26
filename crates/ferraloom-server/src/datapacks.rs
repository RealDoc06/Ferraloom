//! Main's early world access and WorldLoader.PackConfig resource preparation.
use crate::properties::ServerProperties;
use anyhow::{Context, Result};
use ferraloom_data::datapack::{
    self, DataPackConfig, FeatureFlags, ResourceManager, WorldDataConfiguration,
    metadata::MetadataCodecs, repository::PackRepository,
};
use ferraloom_protocol::{chat::Component, component::Persistent};
use ferraloom_storage::{LevelData, LevelStorageAccess};
use std::{path::Path, sync::Arc};

/// Owned startup inputs move into WorldRoot once, after pack configuration.
/// The same access holds the session lock from discovery through shutdown.
pub struct WorldStartup {
    pub(crate) access: Arc<LevelStorageAccess>,
    pub(crate) saved_level_data: Option<LevelData>,
    pub(crate) configuration: WorldDataConfiguration,
}
impl WorldStartup {
    /// Port of Main#loadOrCreateConfig. Read the existing level record exactly
    /// once, before registries; WorldRoot consumes this value without rereading.
    pub fn read(root: &Path, properties: &ServerProperties) -> Result<Self> {
        let access = Arc::new(LevelStorageAccess::open(
            root.join(properties.get("level-name")),
        )?);
        let saved_level_data = if access.path().join("level.dat").exists()
            || access.path().join("level.dat_old").exists()
        {
            Some(access.read_level_data()?)
        } else {
            None
        };
        let configuration = saved_level_data.as_ref().map_or_else(
            || WorldDataConfiguration {
                data_packs: Arc::new(DataPackConfig::from_properties(
                    properties.get("initial-enabled-packs"),
                    properties.get("initial-disabled-packs"),
                )),
                enabled_features: FeatureFlags::VANILLA,
            },
            |data| WorldDataConfiguration::from_nbt(&data.data),
        );
        Ok(Self {
            access,
            saved_level_data,
            configuration,
        })
    }
}

pub(crate) struct ServerMetadataCodecs;
impl MetadataCodecs for ServerMetadataCodecs {
    type Description = Component;
    fn description(
        &self,
        value: &serde_json::Value,
    ) -> std::result::Result<Component, datapack::Error> {
        <Component as Persistent>::from_json(value)
            .map_err(|error| datapack::Error::InvalidData(error.to_string()))
    }
    fn integer(&self, value: &serde_json::Value) -> std::result::Result<i32, datapack::Error> {
        <i32 as Persistent>::from_json(value)
            .map_err(|error| datapack::Error::InvalidData(error.to_string()))
    }
}

pub(crate) fn load_resources(
    root: &Path,
    jar: &Path,
    startup: &mut WorldStartup,
) -> Result<(PackRepository<Component>, ResourceManager)> {
    let allowlist = root.join("allowed_symlinks.txt");
    let custom_allowlist = if allowlist.exists() {
        let text = std::fs::read_to_string(&allowlist)
            .with_context(|| format!("read {}", allowlist.display()))?;
        text.lines()
            .any(|line| !line.is_empty() && !line.starts_with('#'))
            .then_some(allowlist)
    } else {
        None
    };
    let mut repository = PackRepository::discover(
        jar,
        &startup.access.path().join("datapacks"),
        custom_allowlist.as_deref(),
        &ServerMetadataCodecs,
    )?;
    startup.configuration = repository.configure(
        &startup.configuration,
        startup.saved_level_data.is_none(),
        false,
    )?;
    for diagnostic in repository.diagnostics() {
        println!("{}", diagnostic.message);
    }
    let resources = repository.open_selected()?;
    Ok((repository, resources))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::datapack::metadata::{PackCompatibility, PackMetadata};
    use serde_json::Value;
    #[test]
    fn native_pack_metadata_codecs_fallback_features_and_overlays() {
        let rows: Value = serde_json::from_str(include_str!(
            "../../ferraloom-data/tests/java/datapacks/metadata.json"
        ))
        .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 40);
        for row in rows.as_array().unwrap() {
            let decoded = PackMetadata::decode(&row["input"], &ServerMetadataCodecs);
            assert_eq!(decoded.is_ok(), row["loaded"].as_bool().unwrap(), "{row}");
            let Ok(decoded) = decoded else { continue };
            let compatibility = match decoded.compatibility {
                PackCompatibility::TooOld => "TOO_OLD",
                PackCompatibility::TooNew => "TOO_NEW",
                PackCompatibility::Compatible => "COMPATIBLE",
            };
            assert_eq!(
                compatibility,
                row["compatibility"].as_str().unwrap(),
                "{row}"
            );
            assert_eq!(
                decoded.description.to_json().unwrap(),
                row["description"],
                "{row}"
            );
            assert_eq!(
                serde_json::json!(decoded.features.names()),
                row["features"],
                "{row}"
            );
            assert_eq!(
                serde_json::json!(decoded.overlays),
                row["overlays"],
                "{row}"
            );
            assert_eq!(
                decoded.format_fallback.is_some(),
                row["fallback"].as_bool().unwrap_or(false),
                "{row}"
            );
            if let Some(range) = row.get("range") {
                assert_eq!(
                    serde_json::json!([
                        [decoded.formats.min.major, decoded.formats.min.minor],
                        [decoded.formats.max.major, decoded.formats.max.minor]
                    ]),
                    *range,
                    "{row}"
                );
            }
        }
    }
}
