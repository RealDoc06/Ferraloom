//! Fallible Java NBT encoding and durable replacement of saved files.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use bytes::Bytes;
use flate2::{Compression, write::GzEncoder};
use pumpkin_nbt::{
    COMPOUND_ID, NbtCompound,
    serializer::{NbtWriteHelper, NbtWriteHelperJava},
    tag::NbtTag,
};

#[derive(Debug, thiserror::Error)]
pub enum NbtFileError {
    #[error("Failed to encode NBT: {0}")]
    Nbt(#[from] pumpkin_nbt::Error),
    #[error("Failed to write NBT file: {0}")]
    Io(#[from] io::Error),
}

impl From<NbtFileError> for io::Error {
    fn from(error: NbtFileError) -> Self {
        match error {
            NbtFileError::Nbt(error) => Self::new(io::ErrorKind::InvalidData, error),
            NbtFileError::Io(error) => error,
        }
    }
}

/// Encode a named Java root with an empty name, propagating nested tag errors.
pub fn encode_compound(root: NbtCompound) -> Result<Bytes, pumpkin_nbt::Error> {
    encode(root, true)
}

/// PUMP containers omit the root name, but retain the compound type byte.
pub fn encode_unnamed_compound(root: NbtCompound) -> Result<Bytes, pumpkin_nbt::Error> {
    encode(root, false)
}

fn encode(root: NbtCompound, named: bool) -> Result<Bytes, pumpkin_nbt::Error> {
    validate_end_tags(&root)?;
    let mut bytes = Vec::new();
    let mut writer = NbtWriteHelperJava::new(&mut bytes);
    writer.write_u8(COMPOUND_ID)?;
    if named {
        writer.write_string("")?;
    }
    root.serialize_content(&mut writer)?;
    Ok(bytes.into())
}

// The upstream writer accepts End as a named child, which terminates the
// compound before its name is read. Inspect the original tree before encoding;
// an encode/decode comparison could already have discarded the offending data.
fn validate_end_tags(root: &NbtCompound) -> Result<(), pumpkin_nbt::Error> {
    let mut pending: Vec<_> = root.child_tags.values().collect();
    while let Some(tag) = pending.pop() {
        match tag {
            NbtTag::End => {
                return Err(pumpkin_nbt::Error::UnsupportedType(
                    "TAG_End is only valid as a compound terminator or empty list type".into(),
                ));
            }
            NbtTag::Compound(compound) => pending.extend(compound.child_tags.values()),
            NbtTag::List(items) => pending.extend(items),
            _ => {}
        }
    }
    Ok(())
}

pub fn write_compound_atomic(path: &Path, root: NbtCompound) -> Result<(), NbtFileError> {
    let bytes = encode_compound(root)?;
    write_atomic(path, &bytes)?;
    Ok(())
}

pub fn write_gzip_compound_atomic(path: &Path, root: NbtCompound) -> Result<(), NbtFileError> {
    write_gzip(path, None, root)
}

/// Save the previous `level.dat` as `level.dat_old` before replacing the main file.
pub(crate) fn write_gzip_compound_atomic_with_backup(
    path: &Path,
    backup: &Path,
    root: NbtCompound,
) -> Result<(), NbtFileError> {
    write_gzip(path, Some(backup), root)
}

fn write_gzip(path: &Path, backup: Option<&Path>, root: NbtCompound) -> Result<(), NbtFileError> {
    // Finish encoding before creating any file, including the backup.
    let bytes = encode_compound(root)?;
    write_atomic_with_backup(path, backup, |writer| {
        let mut encoder = GzEncoder::new(writer, Compression::default());
        encoder.write_all(&bytes)?;
        encoder.finish()?.flush()
    })?;
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |writer| writer.write_all(bytes))
}

pub(crate) fn write_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<&File>) -> io::Result<()>,
) -> io::Result<()> {
    write_atomic_with_backup(path, None, write)
}

/// Stage beside the destination so rename stays on the same filesystem. Every
/// failure before rename leaves the destination untouched and removes the stage.
fn write_atomic_with_backup(
    path: &Path,
    backup: Option<&Path>,
    write: impl FnOnce(&mut BufWriter<&File>) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    #[cfg(unix)]
    let directory = File::open(parent)?;
    let stage = StagedFile::create(path, parent)?;
    {
        let mut writer = BufWriter::new(&stage.file);
        write(&mut writer)?;
        writer.flush()?;
    };
    stage.file.sync_all()?;

    if let Some(backup) = backup {
        copy_previous(path, backup)?;
    }

    fs::rename(&stage.path, path)?;
    // A failure here is reported even though rename has already committed.
    #[cfg(unix)]
    directory.sync_all()?;
    Ok(())
}

fn copy_previous(path: &Path, backup: &Path) -> io::Result<()> {
    match File::open(path) {
        Ok(mut previous) => write_atomic_with(backup, |writer| {
            io::copy(&mut previous, writer)?;
            Ok(())
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

struct StagedFile {
    path: PathBuf,
    file: File,
}

impl StagedFile {
    fn create(path: &Path, parent: &Path) -> io::Result<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Saved file has no file name")
        })?;
        loop {
            let mut staged_name = OsString::from(".");
            staged_name.push(name);
            staged_name.push(format!(
                ".{}.{}.tmp",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let path = parent.join(staged_name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_nbt::{Nbt, deserializer::NbtReadHelperJava, tag::NbtTag};

    #[test]
    fn durable_nbt_end_values_are_refused_inside_compounds_and_lists() {
        let mut compound = NbtCompound::new();
        compound.put("private-component-name", NbtTag::End);
        for value in [
            NbtTag::End,
            NbtTag::Compound(compound.clone()),
            NbtTag::List(vec![NbtTag::End]),
            NbtTag::List(vec![NbtTag::Int(1), NbtTag::End]),
            NbtTag::List(vec![NbtTag::List(vec![NbtTag::Compound(compound)])]),
        ] {
            let mut root = NbtCompound::new();
            root.put("private-root-name", value);
            for named in [false, true] {
                let error = encode(root.clone(), named).unwrap_err();
                assert!(matches!(error, pumpkin_nbt::Error::UnsupportedType(_)));
                assert!(!error.to_string().contains("private-"));
            }
        }
    }

    #[test]
    fn durable_nbt_instrument_placeholder_refuses_before_commit_or_backup() {
        use pumpkin_data::item_stack::ItemStack;

        // Java 26.3's goat_horn prototype uses a registry string holder. Use a
        // different holder so this represents an explicit saved component patch.
        let mut components = NbtCompound::new();
        components.put_string("minecraft:instrument", "minecraft:sing_goat_horn".into());
        let mut item = NbtCompound::new();
        item.put_string("id", "minecraft:goat_horn".into());
        item.put_int("count", 1);
        item.put_compound("components", components);
        let stack = ItemStack::read_item_stack(&item).unwrap();
        let mut written = NbtCompound::new();
        stack.write_item_stack(&mut written);
        assert_eq!(
            written
                .get_compound("components")
                .unwrap()
                .get("minecraft:instrument"),
            Some(&NbtTag::End)
        );
        let mut invalid = NbtCompound::new();
        invalid.put_list("Items", vec![NbtTag::Compound(written)]);
        for named in [false, true] {
            assert!(matches!(
                encode(invalid.clone(), named),
                Err(pumpkin_nbt::Error::UnsupportedType(_))
            ));
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("save.dat");
        let backup = directory.path().join("save.dat_old");
        let mut previous = NbtCompound::new();
        previous.put_int("marker", 17);
        write_gzip_compound_atomic(&path, previous.clone()).unwrap();
        previous.put_int("marker", 16);
        write_gzip_compound_atomic(&backup, previous).unwrap();
        let before = fs::read(&path).unwrap();
        let backup_before = fs::read(&backup).unwrap();
        for result in [
            write_compound_atomic(&path, invalid.clone()),
            write_gzip_compound_atomic(&path, invalid.clone()),
            write_gzip_compound_atomic_with_backup(&path, &backup, invalid.clone()),
            write_compound_atomic(&directory.path().join("missing-parent/save.dat"), invalid),
        ] {
            assert!(matches!(
                result,
                Err(NbtFileError::Nbt(pumpkin_nbt::Error::UnsupportedType(_)))
            ));
        }
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(&backup).unwrap(), backup_before);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn durable_nbt_rejects_oversized_ascii_and_java_cesu8_keys_and_values() {
        // Supplementary characters and NUL need more Java CESU-8 bytes than UTF-8.
        for (text, encoded_len) in [
            ("a".repeat(65_536), 65_536),
            ("\0".repeat(32_768), 65_536),
            ("😀".repeat(10_923), 65_538),
        ] {
            for oversized_key in [false, true] {
                let mut inner = NbtCompound::new();
                if oversized_key {
                    inner.put_int(&text, 42);
                } else {
                    inner.put_string("value", text.clone());
                }
                let mut root = NbtCompound::new();
                root.put_list("items", vec![NbtTag::Compound(inner)]);
                for named in [false, true] {
                    assert!(matches!(
                        encode(root.clone(), named),
                        Err(pumpkin_nbt::Error::LargeLength(length)) if length == encoded_len
                    ));
                }
            }
        }
    }

    #[test]
    fn durable_nbt_valid_named_unnamed_and_gzip_roundtrip_unknown_tags() {
        let mut custom = NbtCompound::new();
        custom.put_string("unknown:unicode", "a\0😀".into());
        custom.put("unknown:array", NbtTag::LongArray(vec![i64::MIN, i64::MAX]));
        custom.put_string("max_length", "a".repeat(65_535));
        let mut root = NbtCompound::new();
        root.put_list("custom_data", vec![NbtTag::Compound(custom)]);
        root.put_list("empty_end_typed_list", vec![]);
        root.put_compound("empty_compound", NbtCompound::new());
        root.put_list(
            "mixed_list",
            vec![NbtTag::Int(1), NbtTag::String("kept".into())],
        );
        for named in [false, true] {
            let bytes = encode(root.clone(), named).unwrap();
            let expected = if named {
                Nbt::from(root.clone()).write()
            } else {
                Nbt::from(root.clone()).write_unnamed()
            };
            assert_eq!(bytes, expected);
            let mut cursor = io::Cursor::new(bytes.as_ref());
            let mut reader = NbtReadHelperJava::new(&mut cursor);
            let decoded = if named {
                Nbt::read(&mut reader)
            } else {
                Nbt::read_unnamed(&mut reader)
            }
            .unwrap();
            assert_eq!(decoded.root_tag, root);
            assert_eq!(cursor.position(), bytes.len() as u64);
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data.dat");
        write_gzip_compound_atomic(&path, root.clone()).unwrap();
        let decoded =
            pumpkin_nbt::nbt_compress::read_gzip_compound_tag(File::open(path).unwrap()).unwrap();
        assert_eq!(decoded, root);
    }

    #[test]
    fn durable_nbt_failed_stage_and_rename_preserve_destination_and_clean_up() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("save.dat");
        fs::write(&path, b"previous save").unwrap();
        let error = write_atomic_with(&path, |writer| {
            writer.write_all(b"partial new save")?;
            // Exercise cleanup after bytes reached the stage, independent of permissions.
            writer.flush()?;
            Err(io::Error::other("injected write failure"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "injected write failure");
        assert_eq!(fs::read(&path).unwrap(), b"previous save");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        let blocked = directory.path().join("blocked.dat");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("sentinel"), b"preserve").unwrap();
        assert!(write_atomic(&blocked, b"replacement").is_err());
        assert_eq!(fs::read(blocked.join("sentinel")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn durable_nbt_backup_failure_preserves_previous_main_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("level.dat");
        let backup = directory.path().join("level.dat_old");
        let mut root = NbtCompound::new();
        root.put_int("marker", 1);
        write_gzip_compound_atomic(&path, root.clone()).unwrap();
        let previous = fs::read(&path).unwrap();
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("sentinel"), b"preserve").unwrap();
        root.put_int("marker", 2);
        assert!(matches!(
            write_gzip_compound_atomic_with_backup(&path, &backup, root),
            Err(NbtFileError::Io(_))
        ));
        assert_eq!(fs::read(path).unwrap(), previous);
        assert_eq!(fs::read(backup.join("sentinel")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
