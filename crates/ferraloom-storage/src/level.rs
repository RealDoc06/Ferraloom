use crate::{Error, Result, check_tag_version, files};
use ferraloom_nbt::{CompoundTag, NbtString, Tag};
use rustix::fs::{FlockOperation, fcntl_lock};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
fn locks() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    LOCKS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Port of DirectoryLock. Uses Java-compatible POSIX record locks, not BSD flock.
/// The snowman is a marker; ownership comes exclusively from the OS lock.
pub struct DirectoryLock {
    file: Option<File>,
    path: PathBuf,
}
impl DirectoryLock {
    pub fn create(dir: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        let path = fs::canonicalize(dir.as_ref())?.join("session.lock");
        let mut held = locks();
        if held.contains(&path) {
            return Err(locked(&path));
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.write_all("☃".as_bytes())?;
        file.sync_all()?;
        fcntl_lock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
            if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::ACCESS {
                locked(&path)
            } else {
                Error::Io(e.into())
            }
        })?;
        held.insert(path.clone());
        Ok(Self {
            file: Some(file),
            path,
        })
    }
    pub fn is_valid(&self) -> bool {
        self.file.is_some()
    }
    pub fn is_locked(dir: impl AsRef<Path>) -> Result<bool> {
        let path = match fs::canonicalize(dir.as_ref()) {
            Ok(p) => p.join("session.lock"),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let held = locks();
        if held.contains(&path) {
            return Ok(true);
        }
        let file = match OpenOptions::new().write(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Ok(true),
            Err(e) => return Err(e.into()),
        };
        match fcntl_lock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {
                fcntl_lock(&file, FlockOperation::Unlock).map_err(io::Error::from)?;
                Ok(false)
            }
            Err(e) if e == rustix::io::Errno::AGAIN || e == rustix::io::Errno::ACCESS => Ok(true),
            Err(e) => Err(io::Error::from(e).into()),
        }
    }
    pub fn close(&mut self) -> Result<()> {
        let mut held = locks();
        if let Some(file) = self.file.take() {
            let result = fcntl_lock(&file, FlockOperation::Unlock).map_err(io::Error::from);
            drop(file);
            held.remove(&self.path);
            result?;
        }
        Ok(())
    }
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Failed to release session.lock: {e}");
        }
    }
}
fn locked(path: &Path) -> Error {
    Error::Invalid(format!(
        "{}: already locked (possibly by other Minecraft instance?)",
        path.display()
    ))
}

/// Port of DimensionType#getStorageFolder and LevelResource (26.3 has no DIM-1/DIM1 exceptions).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DimensionPaths {
    pub root: PathBuf,
    pub region: PathBuf,
    pub entities: PathBuf,
    pub poi: PathBuf,
    pub data: PathBuf,
}
impl DimensionPaths {
    pub fn new(world: &Path, dimension: &str) -> Result<Self> {
        let root = files::identifier_path(&world.join("dimensions"), dimension)?;
        Ok(Self {
            region: root.join("region"),
            entities: root.join("entities"),
            poi: root.join("poi"),
            data: root.join("data"),
            root,
        })
    }
}

/// Lossless envelope for PrimaryLevelData#createTag. Registry-dependent codecs remain NBT.
/// `data` includes every 26.3 field, including spawn, difficulty_settings, version_history,
/// DataPacks and enabled_features. Unknown fields at both envelope levels are retained.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelData {
    pub data: CompoundTag,
    pub root_extra: CompoundTag,
}
impl LevelData {
    pub fn from_nbt(mut root: CompoundTag) -> Result<Self> {
        let data = match root.remove("Data") {
            Some(Tag::Compound(data)) => data,
            _ => return Err(Error::Invalid("level.dat has no Data compound".into())),
        };
        check_tag_version(&data)?;
        Ok(Self {
            data,
            root_extra: root,
        })
    }
    pub fn to_nbt(&self) -> Result<CompoundTag> {
        check_tag_version(&self.data)?;
        let mut root = self.root_extra.clone();
        root.insert("Data", Tag::Compound(self.data.clone()));
        Ok(root)
    }
    pub fn summary(&self, id: impl Into<String>, locked: bool) -> LevelSummary {
        let settings = self.data.get_compound("difficulty_settings");
        LevelSummary {
            id: id.into(),
            name: self
                .data
                .get_string("LevelName")
                .cloned()
                .unwrap_or_default(),
            last_played: self.data.get_long_or("LastPlayed", 0),
            game_type: self.data.get_int_or("GameType", 0),
            hardcore: settings.is_some_and(|s| s.get_boolean_or("hardcore", false)),
            allow_commands: self.data.get_boolean_or("allowCommands", false),
            data_version: self.data.get_int_or("DataVersion", -1),
            version: self
                .data
                .get_compound("Version")
                .cloned()
                .unwrap_or_default(),
            locked,
            requires_conversion: self.data.get_int_or("version", 0) != 19133,
        }
    }
}
/// Port of LevelSummary's storage-facing fields.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelSummary {
    pub id: String,
    pub name: NbtString,
    pub last_played: i64,
    pub game_type: i32,
    pub hardcore: bool,
    pub allow_commands: bool,
    pub data_version: i32,
    pub version: CompoundTag,
    pub locked: bool,
    pub requires_conversion: bool,
}

/// Port of LevelStorageSource. Opening a world holds its session lock until access is dropped.
pub struct LevelStorageSource {
    base: PathBuf,
}
impl LevelStorageSource {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }
    pub fn open(&self, id: &str) -> Result<LevelStorageAccess> {
        if id.is_empty() || Path::new(id).components().count() != 1 || id == "." || id == ".." {
            return Err(Error::Invalid("world id must be one directory name".into()));
        }
        LevelStorageAccess::open(self.base.join(id))
    }
    pub fn summaries(&self) -> Result<Vec<Result<LevelSummary>>> {
        if !self.base.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.base)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            if !path.join("level.dat").is_file() && !path.join("level.dat_old").is_file() {
                continue;
            }
            out.push((|| {
                let data = load_level(&path)?;
                Ok(data.summary(
                    entry.file_name().to_string_lossy(),
                    DirectoryLock::is_locked(&path)?,
                ))
            })());
        }
        out.sort_by_key(|result| std::cmp::Reverse(result.as_ref().map_or(0, |s| s.last_played)));
        Ok(out)
    }
}
pub struct LevelStorageAccess {
    path: PathBuf,
    lock: DirectoryLock,
}
impl LevelStorageAccess {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let lock = DirectoryLock::create(&path)?;
        Ok(Self { path, lock })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn dimension(&self, id: &str) -> Result<DimensionPaths> {
        DimensionPaths::new(&self.path, id)
    }
    pub fn read_level_data(&self) -> Result<LevelData> {
        self.check_lock()?;
        load_level(&self.path)
    }
    pub fn save_level_data(&self, data: &LevelData) -> Result<()> {
        self.check_lock()?;
        files::write_nbt(
            &self.path.join("level.dat"),
            &data.to_nbt()?,
            Some(&self.path.join("level.dat_old")),
        )
    }
    pub fn player_data(&self) -> PlayerDataStorage {
        PlayerDataStorage::new(&self.path)
    }
    fn check_lock(&self) -> Result<()> {
        if self.lock.is_valid() {
            Ok(())
        } else {
            Err(Error::Closed)
        }
    }
    pub fn close(&mut self) -> Result<()> {
        self.lock.close()
    }
}
fn load_level(path: &Path) -> Result<LevelData> {
    match files::read_nbt(&path.join("level.dat")) {
        Ok(root) => LevelData::from_nbt(root),
        Err(e) => {
            log::warn!("Could not read level.dat, trying level.dat_old: {e}");
            LevelData::from_nbt(files::read_nbt(&path.join("level.dat_old"))?)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PlayerJsonKind {
    Stats,
    Advancements,
}
impl PlayerJsonKind {
    fn directory(self) -> &'static str {
        match self {
            Self::Stats => "stats",
            Self::Advancements => "advancements",
        }
    }
}
/// Port of PlayerDataStorage, ServerStatsCounter and PlayerAdvancements's persistence.
pub struct PlayerDataStorage {
    world: PathBuf,
}
impl PlayerDataStorage {
    pub fn new(world: impl Into<PathBuf>) -> Self {
        Self {
            world: world.into(),
        }
    }
    fn player_path(&self, uuid: &str, suffix: &str) -> Result<PathBuf> {
        Ok(self
            .world
            .join("players/data")
            .join(format!("{}{suffix}", canonical_uuid(uuid)?)))
    }
    pub fn save(&self, uuid: &str, tag: &CompoundTag) -> Result<()> {
        check_tag_version(tag)?;
        files::write_nbt(
            &self.player_path(uuid, ".dat")?,
            tag,
            Some(&self.player_path(uuid, ".dat_old")?),
        )
    }
    pub fn load(&self, uuid: &str) -> Result<Option<CompoundTag>> {
        let path = self.player_path(uuid, ".dat")?;
        let loaded = if path.is_file() {
            match files::read_nbt(&path) {
                Ok(tag) => Some(tag),
                Err(e) => {
                    log::warn!("Failed to load player data {uuid}: {e}");
                    None
                }
            }
        } else {
            None
        };
        let tag = if let Some(tag) = loaded {
            Some(tag)
        } else {
            if path.is_file() {
                let now = time::OffsetDateTime::now_local()
                    .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
                let timestamp = format!(
                    "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
                    now.year(),
                    u8::from(now.month()),
                    now.day(),
                    now.hour(),
                    now.minute(),
                    now.second()
                );
                let backup = self.player_path(uuid, &format!("_corrupted_{timestamp}.dat"))?;
                if let Err(e) = files::copy_with_attributes(&path, &backup) {
                    log::warn!("Failed to back up corrupted player {uuid}: {e}");
                }
            }
            let old = self.player_path(uuid, ".dat_old")?;
            if old.is_file() {
                match files::read_nbt(&old) {
                    Ok(tag) => Some(tag),
                    Err(e) => {
                        log::warn!("Failed to load old player data {uuid}: {e}");
                        None
                    }
                }
            } else {
                None
            }
        };
        if let Some(tag) = &tag {
            check_tag_version(tag)?;
        }
        Ok(tag)
    }
    fn json_path(&self, uuid: &str, kind: PlayerJsonKind) -> Result<PathBuf> {
        Ok(self
            .world
            .join("players")
            .join(kind.directory())
            .join(format!("{}.json", canonical_uuid(uuid)?)))
    }
    pub fn load_json(&self, uuid: &str, kind: PlayerJsonKind) -> Result<Option<serde_json::Value>> {
        let path = self.json_path(uuid, kind)?;
        match fs::read(path) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)?;
                check_json_version(&value)?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save_json(
        &self,
        uuid: &str,
        kind: PlayerJsonKind,
        value: &serde_json::Value,
    ) -> Result<()> {
        check_json_version(value)?;
        files::write_json(&self.json_path(uuid, kind)?, value)
    }
}
fn check_json_version(value: &serde_json::Value) -> Result<()> {
    let version = value
        .get("DataVersion")
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(-1);
    crate::check_data_version(version)
}
fn canonical_uuid(uuid: &str) -> Result<String> {
    if uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
    {
        return Err(Error::Invalid("invalid player UUID".into()));
    }
    Ok(uuid.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimension_layout_and_paths() {
        let paths = DimensionPaths::new(Path::new("world"), "minecraft:overworld").unwrap();
        assert_eq!(
            paths.region,
            Path::new("world/dimensions/minecraft/overworld/region")
        );
        assert!(DimensionPaths::new(Path::new("world"), "evil:../escape").is_err());
        assert!(canonical_uuid("../../escape").is_err());
    }
    #[test]
    fn old_and_future_data_versions_are_not_relabelled() {
        assert!(crate::check_data_version(crate::DATA_VERSION - 1).is_err());
        assert!(crate::check_data_version(crate::DATA_VERSION + 1).is_err());
    }
}
