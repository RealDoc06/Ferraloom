pub use pumpkin_data::statistic::{CustomStatistic, StatisticCategory};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use rustc_hash::FxHashMap;
use std::{io, path::Path};

mod persistence;
mod registry;

use persistence::StatisticsFile;

#[derive(Debug, thiserror::Error)]
pub enum StatisticsDataError {
    #[error("Statistics IO: {0}")]
    Io(#[from] io::Error),
    #[error("Statistics JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported statistics schema: {0}")]
    Schema(String),
    #[error("Unsupported statistics DataVersion {0}; no statistics data fixer is implemented")]
    UnsupportedVersion(i32),
    #[error(
        "Legacy player NBT contains a nonempty or malformed Statistics tag; no migration is implemented. Preserve the player .dat and canonical statistics JSON; login and statistics saves are disabled, and their counters are never merged"
    )]
    LegacyPrivateStatistics,
    #[error("Statistics have not been successfully loaded")]
    NotLoaded,
}

#[derive(Default)]
enum LoadState {
    #[default]
    Unloaded,
    New,
    Loaded,
    Disabled,
    Failed,
}

#[derive(Default)]
pub struct Statistics {
    // Only checked, known registry pairs enter runtime state or award_stats.
    stats: FxHashMap<(StatisticCategory, i32), i32>,
    disk: StatisticsFile,
    load_state: LoadState,
    // Preserve the typed tree, never rewrite private counters from live JSON.
    legacy: Option<NbtTag>,
}

impl Statistics {
    pub fn increment(&mut self, category: StatisticCategory, stat: i32, amount: i32) {
        if registry::stat_key(category, stat).is_some() {
            let entry = self.stats.entry((category, stat)).or_insert(0);
            *entry = entry.saturating_add(amount);
        }
    }

    pub fn increment_custom(&mut self, stat: CustomStatistic, amount: i32) {
        self.increment(StatisticCategory::Custom, stat as i32, amount);
    }

    pub fn set(&mut self, category: StatisticCategory, stat: i32, value: i32) {
        if registry::stat_key(category, stat).is_some() {
            self.stats.insert((category, stat), value);
        }
    }

    #[must_use]
    pub fn get(&self, category: StatisticCategory, stat: i32) -> i32 {
        *self.stats.get(&(category, stat)).unwrap_or(&0)
    }

    pub(crate) fn network_entries(
        &self,
    ) -> impl Iterator<Item = (StatisticCategory, i32, i32)> + '_ {
        self.stats
            .iter()
            .map(|(&(category, stat), &value)| (category, stat, value))
    }

    /// No new private Statistics tag is emitted. Even a refused legacy load
    /// retains its original typed tree if an external caller serializes the NBT.
    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(legacy) = &self.legacy {
            nbt.put("Statistics", legacy.clone());
        }
    }

    pub fn read_nbt(&mut self, nbt: &NbtCompound) {
        self.legacy = nbt.get("Statistics").cloned();
        self.load_state = LoadState::Unloaded;
    }

    /// Parse the entire canonical file before replacing live counters. Private
    /// legacy history is deliberately unsupported, including equal coexistence:
    /// the three persistence families do not share an atomic snapshot.
    pub(crate) fn load(&mut self, path: &Path, enabled: bool) -> Result<(), StatisticsDataError> {
        if !enabled {
            self.load_state = LoadState::Disabled;
            return Ok(());
        }
        self.load_state = LoadState::Failed;
        if let Some(legacy) = &self.legacy
            && !matches!(legacy, NbtTag::Compound(compound) if compound.is_empty())
        {
            return Err(StatisticsDataError::LegacyPrivateStatistics);
        }
        let file = persistence::read_file(path)?;
        let load_state = if file.is_some() {
            LoadState::Loaded
        } else {
            LoadState::New
        };
        let disk = file.unwrap_or_default();
        let mut stats = FxHashMap::default();
        for (category_name, counters) in &disk.stats {
            let Some(category) = registry::category(category_name) else {
                continue;
            };
            for (name, value) in counters {
                if let Some(stat) = registry::stat_id(category, name) {
                    stats.insert((category, stat), *value);
                }
            }
        }
        self.stats = stats;
        self.disk = disk;
        self.load_state = load_state;
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> Result<Option<Vec<u8>>, StatisticsDataError> {
        match self.load_state {
            LoadState::Disabled => return Ok(None),
            LoadState::Unloaded | LoadState::Failed => return Err(StatisticsDataError::NotLoaded),
            LoadState::New | LoadState::Loaded => {}
        }
        let mut disk = self.disk.clone();
        for (category, stat, value) in self.network_entries() {
            let key = registry::stat_key(category, stat).ok_or_else(|| {
                StatisticsDataError::Schema("runtime statistic has no registry name".into())
            })?;
            disk.stats
                .entry(registry::category_name(category).into())
                .or_default()
                .insert(format!("minecraft:{key}"), value);
        }
        Ok(Some(persistence::encode(&disk)?))
    }
}

#[cfg(test)]
mod tests;
