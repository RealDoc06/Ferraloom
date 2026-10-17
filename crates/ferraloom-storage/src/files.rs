use crate::{Error, Result};
use ferraloom_nbt::{CompoundTag, Compression, NbtAccounter, NbtIo};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) fn read_nbt(path: &Path) -> Result<CompoundTag> {
    let bytes = fs::read(path)?;
    let compression = if bytes.starts_with(&[0x1f, 0x8b]) {
        Compression::Gzip
    } else {
        Compression::None
    };
    Ok(NbtIo::read_compound(
        bytes.as_slice(),
        compression,
        &mut NbtAccounter::unlimited(),
    )?)
}
pub(crate) fn temp_file(dir: &Path, prefix: &str) -> Result<(PathBuf, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let path = dir.join(format!(
            "{prefix}-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
pub(crate) fn sync_directory(dir: &Path) -> Result<()> {
    File::open(dir)?.sync_all()?;
    Ok(())
}
pub(crate) fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Port of Util#safeReplaceFile, including restoration if installing the new file fails.
pub(crate) fn replace(path: &Path, tmp: &Path, backup: Option<&Path>) -> Result<()> {
    let retry = |op: &mut dyn FnMut() -> io::Result<()>| {
        let mut result = op();
        for _ in 1..10 {
            if result.is_ok() {
                break;
            }
            result = op();
        }
        result
    };
    if let Some(backup) = backup.filter(|_| path.exists()) {
        remove_if_exists(backup)?;
        retry(&mut || fs::rename(path, backup))?;
    }
    if let Err(e) = retry(&mut || fs::rename(tmp, path)) {
        if let Some(backup) = backup.filter(|p| p.exists())
            && let Err(restore) = retry(&mut || fs::rename(backup, path))
        {
            log::error!("Failed to restore {}: {restore}", path.display());
        }
        return Err(e.into());
    }
    sync_directory(
        path.parent()
            .ok_or_else(|| Error::Invalid("file has no parent directory".into()))?,
    )
}
pub(crate) fn write_nbt(path: &Path, tag: &CompoundTag, backup: Option<&Path>) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Invalid("file has no parent directory".into()))?;
    fs::create_dir_all(dir)?;
    let (tmp, mut file) = temp_file(dir, "nbt")?;
    let result = (|| {
        NbtIo::write_compound(&mut file, Compression::Gzip, tag)?;
        file.sync_all()?;
        drop(file);
        replace(path, &tmp, backup)
    })();
    if result.is_err() {
        let _ = remove_if_exists(&tmp);
    }
    result
}
pub(crate) fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Vanilla stats and advancements use a direct UTF-8 write, without .dat_old backups.
    let mut file = File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn identifier_path(root: &Path, id: &str) -> Result<PathBuf> {
    let (namespace, name) = id.split_once(':').unwrap_or(("minecraft", id));
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
    };
    if !valid(namespace) || !name.split('/').all(|p| valid(p) && p != "." && p != "..") {
        return Err(Error::Invalid(format!("invalid storage identifier {id}")));
    }
    Ok(root.join(namespace).join(name))
}

/// Port of Files.copy(..., COPY_ATTRIBUTES) used by PlayerDataStorage#backup.
pub(crate) fn copy_with_attributes(from: &Path, to: &Path) -> Result<()> {
    let metadata = fs::metadata(from)?;
    fs::copy(from, to)?;
    let times = fs::FileTimes::new()
        .set_modified(metadata.modified()?)
        .set_accessed(metadata.accessed()?);
    OpenOptions::new().write(true).open(to)?.set_times(times)?;
    Ok(())
}
