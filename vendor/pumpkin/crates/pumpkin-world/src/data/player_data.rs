use pumpkin_nbt::compound::NbtCompound;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, create_dir_all};
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Stores player NBT with a previous-save backup and durable recovery before login.
pub struct PlayerDataStorage {
    /// Path to the directory where player data is stored
    data_path: PathBuf,
    /// Whether player data saving is enabled
    save_enabled: bool,
    /// Reserved before capture, so queued snapshots retain their original order.
    next_snapshot: AtomicU64,
    /// Orders recovery and replacements, and records successful writes per player.
    committed: Mutex<HashMap<Uuid, u64>>,
}

/// Captured player NBT and its original ordering token. Create and save it using
/// the same storage instance; private fields prevent unversioned save payloads.
pub struct PlayerSnapshot {
    uuid: Uuid,
    sequence: u64,
    data: NbtCompound,
}

impl PlayerSnapshot {
    #[must_use]
    pub const fn uuid(&self) -> Uuid {
        self.uuid
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerDataError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("NBT error: {0}")]
    Nbt(String),
    #[error("Primary player data failed: {primary}; backup player data failed: {backup}")]
    Load {
        primary: Box<Self>,
        backup: Box<Self>,
    },
}

impl PlayerDataStorage {
    /// Creates storage in the specified player data directory.
    pub fn new(data_path: impl Into<PathBuf>, enabled: bool) -> Self {
        let path = data_path.into();
        if !path.exists()
            && let Err(e) = create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }

        Self {
            data_path: path,
            save_enabled: enabled,
            next_snapshot: AtomicU64::new(1),
            committed: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub const fn get_data_path(&self) -> &PathBuf {
        &self.data_path
    }

    #[must_use]
    pub const fn is_save_enabled(&self) -> bool {
        self.save_enabled
    }

    pub const fn set_save_enabled(&mut self, enabled: bool) {
        self.save_enabled = enabled;
    }

    /// Returns the path for a player's data file based on their UUID.
    #[must_use]
    pub fn get_player_data_path(&self, uuid: &Uuid) -> PathBuf {
        self.get_data_path().join(format!("{uuid}.dat"))
    }

    /// Loads `.dat`, falling back to `.dat_old` only on decode failure or absence.
    /// Recovery archives the failed primary and restores the valid backup before
    /// allowing player saves. IO errors refuse login without touching either file.
    pub fn load_player_data(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        if !self.is_save_enabled() {
            return Ok((false, NbtCompound::new()));
        }

        let _guard = self
            .committed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = self.get_player_data_path(uuid);
        let primary_error = match read_player_file(&path) {
            Ok(Some((data, _))) => return Ok((true, data)),
            Ok(None) => None,
            Err(error @ PlayerDataError::Nbt(_)) => Some(error),
            Err(error) => return Err(error),
        };
        let backup_path = path.with_extension("dat_old");
        let backup = match read_player_file(&backup_path) {
            Ok(Some(backup)) => backup,
            Ok(None) => {
                if let Some(error) = primary_error {
                    return Err(error);
                }
                debug!("No player data file found for {uuid}");
                return Ok((false, NbtCompound::new()));
            }
            Err(backup) => {
                return Err(if let Some(primary) = primary_error {
                    PlayerDataError::Load {
                        primary: Box::new(primary),
                        backup: Box::new(backup),
                    }
                } else {
                    backup
                });
            }
        };

        if let Some(error) = primary_error {
            // If the original cannot be copied, refuse recovery rather than overwrite it.
            let archive = self.archive_failed_primary(uuid, &path)?;
            warn!(
                "Failed to load player data for {uuid}: {error}; preserved at {}",
                archive.display()
            );
        }

        // Do not rotate the failed primary into the only valid backup. Restoring
        // it now also makes recovery safe across a restart or a later failed save.
        crate::nbt_io::write_atomic(&path, &backup.1)?;
        warn!(
            "Recovered player data for {uuid} from {}",
            backup_path.display()
        );
        Ok((true, backup.0))
    }

    fn archive_failed_primary(&self, uuid: &Uuid, path: &Path) -> io::Result<PathBuf> {
        let bytes = fs::read(path)?;
        loop {
            let archive = self
                .data_path
                .join(format!("{uuid}_corrupted_{}.dat", Uuid::new_v4()));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&archive)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let result = (|| {
                file.write_all(&bytes)?;
                file.sync_all()?;
                #[cfg(unix)]
                File::open(&self.data_path)?.sync_all()?;
                Ok(())
            })();
            if let Err(error) = result {
                drop(file);
                let _ = fs::remove_file(&archive);
                return Err(error);
            }
            return Ok(archive);
        }
    }

    /// Reserve the snapshot's order before reading any player fields. Carry the
    /// returned value through background queues instead of reserving at write time.
    pub fn snapshot_player_data(
        &self,
        uuid: Uuid,
        capture: impl FnOnce(&mut NbtCompound),
    ) -> PlayerSnapshot {
        let sequence = self.next_snapshot.fetch_add(1, Ordering::Relaxed);
        let mut data = NbtCompound::new();
        capture(&mut data);
        PlayerSnapshot {
            uuid,
            sequence,
            data,
        }
    }

    /// Atomically saves a captured snapshot unless a newer one already succeeded,
    /// retaining the previous `.dat` as `.dat_old` only when writing a replacement.
    pub fn save_player_data(&self, snapshot: PlayerSnapshot) -> Result<(), PlayerDataError> {
        // Skip saving if disabled in config
        if !self.is_save_enabled() {
            return Ok(());
        }

        let PlayerSnapshot {
            uuid,
            sequence,
            data,
        } = snapshot;
        let mut committed = self
            .committed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if committed
            .get(&uuid)
            .is_some_and(|latest| *latest >= sequence)
        {
            debug!("Skipping superseded player snapshot for {uuid}");
            return Ok(());
        }
        let path = self.get_player_data_path(&uuid);

        // Ensure parent directory exists
        if let Some(parent) = path.parent()
            && let Err(e) = create_dir_all(parent)
        {
            error!("Failed to create player data directory for {uuid}: {e}");
            return Err(PlayerDataError::Io(e));
        }

        crate::nbt_io::write_gzip_compound_atomic_with_backup(
            &path,
            &path.with_extension("dat_old"),
            data,
        )
        .map_err(|error| {
            error!("Failed to save player data for {uuid}: {error}");
            match error {
                crate::nbt_io::NbtFileError::Nbt(error) => PlayerDataError::Nbt(error.to_string()),
                crate::nbt_io::NbtFileError::Io(error) => PlayerDataError::Io(error),
            }
        })?;
        committed.insert(uuid, sequence);
        debug!("Saved player data for {uuid} to disk");
        Ok(())
    }
}

/// Missing files are new-player candidates only when the directory exists
/// and no entry (including a dangling symlink) occupies the requested name.
fn read_player_file(path: &Path) -> Result<Option<(NbtCompound, Vec<u8>)>, PlayerDataError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().unwrap_or(Path::new("."));
            if !fs::metadata(parent)?.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "Invalid player data directory",
                )
                .into());
            }
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let metadata = if metadata.file_type().is_symlink() {
        fs::metadata(path)?
    } else {
        metadata
    };
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Player data is not a regular file",
        )
        .into());
    }
    let bytes = fs::read(path)?;
    let data = pumpkin_nbt::nbt_compress::read_gzip_compound_tag(Cursor::new(&bytes))
        .map_err(|error| PlayerDataError::Nbt(error.to_string()))?;
    Ok(Some((data, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(
        storage: &PlayerDataStorage,
        uuid: Uuid,
        data: NbtCompound,
    ) -> Result<(), PlayerDataError> {
        let snapshot = storage.snapshot_player_data(uuid, |nbt| *nbt = data);
        storage.save_player_data(snapshot)
    }

    fn player_data(marker: i32) -> NbtCompound {
        let mut data = NbtCompound::new();
        data.put_int("marker", marker);
        data.put_string("unknown:tag", "keep\0😀".into());
        data
    }

    fn read_saved(path: &Path) -> NbtCompound {
        read_player_file(path).unwrap().unwrap().0
    }

    fn archives(storage: &PlayerDataStorage, uuid: &Uuid) -> Vec<PathBuf> {
        fs::read_dir(storage.get_data_path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{uuid}_corrupted_"))
            })
            .collect()
    }

    #[test]
    fn player_delayed_snapshot_cannot_replace_newer_primary_or_backup() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(20);
        let other_uuid = Uuid::from_u128(21);
        save(&storage, uuid, player_data(0)).unwrap();
        let older = storage.snapshot_player_data(uuid, |nbt| *nbt = player_data(1));
        let other_player = storage.snapshot_player_data(other_uuid, |nbt| *nbt = player_data(7));
        save(&storage, uuid, player_data(2)).unwrap();
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        let primary_bytes = fs::read(&path).unwrap();
        let backup_bytes = fs::read(&backup).unwrap();
        assert_eq!(read_saved(&path), player_data(2));
        assert_eq!(read_saved(&backup), player_data(0));

        storage.save_player_data(older).unwrap();
        assert_eq!(fs::read(&path).unwrap(), primary_bytes);
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        // A later commit for one UUID must not suppress another player's capture.
        storage.save_player_data(other_player).unwrap();
        assert_eq!(
            storage.load_player_data(&other_uuid).unwrap(),
            (true, player_data(7))
        );
    }

    #[test]
    fn player_version_is_reserved_before_capture_and_stale_nbt_is_not_encoded() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(22);
        save(&storage, uuid, player_data(0)).unwrap();
        let older = storage.snapshot_player_data(uuid, |nbt| {
            // Complete another capture while this one is still reading fields.
            save(&storage, uuid, player_data(2)).unwrap();
            *nbt = player_data(1);
            nbt.put_string("oversized", "😀".repeat(10_923));
        });
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        let primary_bytes = fs::read(&path).unwrap();
        let backup_bytes = fs::read(&backup).unwrap();
        storage.save_player_data(older).unwrap();
        assert_eq!(fs::read(&path).unwrap(), primary_bytes);
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        assert_eq!(read_saved(&path), player_data(2));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn player_failed_newer_write_does_not_suppress_an_older_valid_capture() {
        for encoding_failure in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let storage = PlayerDataStorage::new(directory.path(), true);
            let uuid = Uuid::from_u128(23);
            save(&storage, uuid, player_data(0)).unwrap();
            let older = storage.snapshot_player_data(uuid, |nbt| *nbt = player_data(1));
            let newer = storage.snapshot_player_data(uuid, |nbt| {
                *nbt = player_data(2);
                if encoding_failure {
                    nbt.put_string("oversized", "😀".repeat(10_923));
                }
            });
            let path = storage.get_player_data_path(&uuid);
            let backup = path.with_extension("dat_old");
            if !encoding_failure {
                fs::create_dir(&backup).unwrap();
            }
            assert!(storage.save_player_data(newer).is_err());
            assert_eq!(read_saved(&path), player_data(0));
            if !encoding_failure {
                fs::remove_dir(&backup).unwrap();
            }
            storage.save_player_data(older).unwrap();
            assert_eq!(read_saved(&path), player_data(1));
            assert_eq!(read_saved(&backup), player_data(0));
        }
    }

    #[test]
    fn player_saves_keep_previous_backup_and_prefer_valid_primary() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(7);
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        save(&storage, uuid, player_data(1)).unwrap();
        let previous = fs::read(&path).unwrap();
        save(&storage, uuid, player_data(2)).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), previous);
        assert_eq!(read_saved(&backup), player_data(1));
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, player_data(2))
        );

        fs::write(&backup, b"invalid backup").unwrap();
        let primary = fs::read(&path).unwrap();
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, player_data(2))
        );
        assert_eq!(fs::read(&path).unwrap(), primary);
        assert_eq!(fs::read(&backup).unwrap(), b"invalid backup");
        assert!(archives(&storage, &uuid).is_empty());
    }

    #[test]
    fn player_recovery_archives_corruption_restores_backup_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let uuid = Uuid::from_u128(8);
        let storage = PlayerDataStorage::new(directory.path(), true);
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        save(&storage, uuid, player_data(1)).unwrap();
        save(&storage, uuid, player_data(2)).unwrap();
        let valid_backup = fs::read(&backup).unwrap();
        fs::write(&path, b"broken primary").unwrap();

        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, player_data(1))
        );
        assert_eq!(fs::read(&path).unwrap(), valid_backup);
        assert_eq!(fs::read(&backup).unwrap(), valid_backup);
        let first_archive = archives(&storage, &uuid).pop().unwrap();
        assert_eq!(fs::read(&first_archive).unwrap(), b"broken primary");

        // Recovery must leave durable valid state, with no per-instance flag needed.
        drop(storage);
        let storage = PlayerDataStorage::new(directory.path(), true);
        let mut invalid = player_data(3);
        invalid.put_string("oversized", "😀".repeat(10_923));
        assert!(matches!(
            save(&storage, uuid, invalid),
            Err(PlayerDataError::Nbt(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), valid_backup);
        assert_eq!(fs::read(&backup).unwrap(), valid_backup);
        save(&storage, uuid, player_data(3)).unwrap();
        assert_eq!(read_saved(&path), player_data(3));
        assert_eq!(fs::read(&backup).unwrap(), valid_backup);
        assert_eq!(fs::read(&first_archive).unwrap(), b"broken primary");

        fs::write(&path, b"broken again").unwrap();
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, player_data(1))
        );
        let preserved = archives(&storage, &uuid);
        assert_eq!(preserved.len(), 2);
        assert!(
            preserved
                .iter()
                .any(|path| fs::read(path).unwrap() == b"broken primary")
        );
        assert!(
            preserved
                .iter()
                .any(|path| fs::read(path).unwrap() == b"broken again")
        );
    }

    #[test]
    fn player_missing_primary_recovers_backup_but_only_both_missing_are_new() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(9);
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (false, NbtCompound::new())
        );
        save(&storage, uuid, player_data(1)).unwrap();
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        fs::rename(&path, &backup).unwrap();
        let previous = fs::read(&backup).unwrap();
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, player_data(1))
        );
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read(&backup).unwrap(), previous);
        assert!(archives(&storage, &uuid).is_empty());

        fs::remove_file(&path).unwrap();
        fs::write(&backup, b"invalid backup").unwrap();
        assert!(matches!(
            storage.load_player_data(&uuid),
            Err(PlayerDataError::Nbt(_))
        ));
        assert!(!path.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"invalid backup");
    }

    #[test]
    fn player_invalid_primary_and_backup_refuse_load_without_modifying_files() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(10);
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        fs::write(&path, b"invalid primary").unwrap();
        assert!(matches!(
            storage.load_player_data(&uuid),
            Err(PlayerDataError::Nbt(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"invalid primary");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        fs::write(&backup, b"invalid backup").unwrap();
        assert!(matches!(
            storage.load_player_data(&uuid),
            Err(PlayerDataError::Load { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), b"invalid primary");
        assert_eq!(fs::read(&backup).unwrap(), b"invalid backup");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);

        fs::remove_file(&backup).unwrap();
        fs::create_dir(&backup).unwrap();
        let Err(PlayerDataError::Load {
            primary,
            backup: error,
        }) = storage.load_player_data(&uuid)
        else {
            panic!("decode failure and backup IO failure must both be reported");
        };
        assert!(matches!(*primary, PlayerDataError::Nbt(_)));
        assert!(matches!(*error, PlayerDataError::Io(_)));
        assert_eq!(fs::read(&path).unwrap(), b"invalid primary");
        assert!(backup.is_dir());
        assert!(archives(&storage, &uuid).is_empty());
    }

    #[test]
    fn player_directory_error_cannot_be_treated_as_absence_or_overwritten_by_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(11);
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        save(&storage, uuid, player_data(1)).unwrap();
        fs::rename(&path, &backup).unwrap();
        let previous = fs::read(&backup).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("sentinel"), b"preserve").unwrap();
        assert!(matches!(
            storage.load_player_data(&uuid),
            Err(PlayerDataError::Io(_))
        ));
        assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"preserve");
        assert_eq!(fs::read(&backup).unwrap(), previous);
        assert!(archives(&storage, &uuid).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn player_broken_symlink_and_missing_directory_are_errors() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(12);
        let path = storage.get_player_data_path(&uuid);
        let backup = path.with_extension("dat_old");
        save(&storage, uuid, player_data(1)).unwrap();
        fs::rename(&path, &backup).unwrap();
        let valid_backup = fs::read(&backup).unwrap();
        let target = directory.path().join("missing.dat");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(matches!(
            storage.load_player_data(&uuid),
            Err(PlayerDataError::Io(_))
        ));
        assert_eq!(fs::read_link(&path).unwrap(), target);
        assert_eq!(fs::read(&backup).unwrap(), valid_backup);
        assert!(archives(&storage, &uuid).is_empty());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        fs::remove_file(&path).unwrap();
        fs::remove_file(&backup).unwrap();
        fs::remove_dir(directory.path()).unwrap();
        let Err(PlayerDataError::Io(error)) = storage.load_player_data(&uuid) else {
            panic!("missing player directory must refuse load before trying the backup");
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn player_backup_replace_failure_preserves_primary_and_backup_entry() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(13);
        save(&storage, uuid, player_data(1)).unwrap();
        let path = storage.get_player_data_path(&uuid);
        let previous = fs::read(&path).unwrap();
        let backup = path.with_extension("dat_old");
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("sentinel"), b"preserve").unwrap();
        assert!(matches!(
            save(&storage, uuid, player_data(2)),
            Err(PlayerDataError::Io(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read(backup.join("sentinel")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn player_disabled_storage_ignores_existing_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), false);
        let uuid = Uuid::from_u128(14);
        let path = storage.get_player_data_path(&uuid);
        fs::write(&path, b"preserve").unwrap();
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (false, NbtCompound::new())
        );
        save(&storage, uuid, player_data(1)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn durable_nbt_player_encode_failure_keeps_previous_file_and_can_retry() {
        let directory = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(directory.path(), true);
        let uuid = Uuid::from_u128(42);
        let mut data = NbtCompound::new();
        data.put_string("unknown:tag", "keep\0😀".into());
        save(&storage, uuid, data.clone()).unwrap();
        let path = storage.get_player_data_path(&uuid);
        let previous = std::fs::read(&path).unwrap();

        let mut item = NbtCompound::new();
        item.put_string("minecraft:custom_data", "😀".repeat(10_923));
        let mut invalid = data.clone();
        invalid.put_list("Inventory", vec![pumpkin_nbt::tag::NbtTag::Compound(item)]);
        assert!(matches!(
            save(&storage, uuid, invalid),
            Err(PlayerDataError::Nbt(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert_eq!(
            storage.load_player_data(&uuid).unwrap(),
            (true, data.clone())
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

        data.put_int("updated", 42);
        save(&storage, uuid, data.clone()).unwrap();
        assert_eq!(storage.load_player_data(&uuid).unwrap(), (true, data));
    }
}
