//! Vanilla 26.3 world storage. Disk IO belongs on an [`IoWorker`], never a tick thread.
mod compression;
mod error;
mod files;
mod level;
mod region;
mod saved_data;
mod schema;
mod worker;

pub use compression::RegionFileVersion;
pub use error::{Error, Result};
pub use level::{
    DimensionPaths, DirectoryLock, LevelData, LevelStorageAccess, LevelStorageSource, LevelSummary,
    PlayerDataStorage, PlayerJsonKind,
};
pub use region::{ChunkPos, RegionBitmap, RegionFile, RegionFileStorage};
pub use saved_data::{SavedData, SavedDataCodec, SavedDataStorage};
pub use schema::*;
pub use worker::{IoTask, IoWorker};

/// Vanilla 26.3's data version. Older saves require a future DataFixer port.
pub const DATA_VERSION: i32 = 5023;

/// Hook for a future DataFixer. Callers must not silently relabel an old save.
pub fn check_data_version(version: i32) -> Result<()> {
    if version == DATA_VERSION {
        Ok(())
    } else {
        Err(Error::DataVersion {
            found: version,
            expected: DATA_VERSION,
        })
    }
}

pub(crate) fn check_tag_version(tag: &ferraloom_nbt::CompoundTag) -> Result<()> {
    check_data_version(tag.get_int_or("DataVersion", -1))
}

#[cfg(test)]
mod tests;
