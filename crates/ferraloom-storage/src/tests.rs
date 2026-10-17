use crate::*;
use ferraloom_nbt::{CompoundTag, Tag};
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferraloom-storage-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn tag(n: i32) -> CompoundTag {
    [
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("value", Tag::Int(n)),
    ]
    .into_iter()
    .collect()
}

#[test]
fn first_fit_allocation_and_copy_on_write_reuse() {
    let mut bitmap = RegionBitmap::default();
    bitmap.force(0, 2);
    assert_eq!(bitmap.allocate(3), 2);
    assert_eq!(bitmap.allocate(2), 5);
    bitmap.free(2, 3);
    assert_eq!(bitmap.allocate(2), 2);
    assert_eq!(bitmap.allocate(2), 7);
    assert_eq!(bitmap.allocate(1), 4);
    let dir = Scratch::new();
    let path = dir.0.join("r.0.0.mca");
    let pos = ChunkPos::new(0, 0);
    let mut region = RegionFile::open(&path, &dir.0, RegionFileVersion::None, true).unwrap();
    region.write(pos, &tag(1)).unwrap();
    assert_eq!(region.offset(pos), (2 << 8) | 1);
    region.write(pos, &tag(2)).unwrap();
    assert_eq!(region.offset(pos), (3 << 8) | 1);
    region.write(pos, &tag(3)).unwrap();
    assert_eq!(region.offset(pos), (2 << 8) | 1);
    assert_ne!(region.timestamp(pos), 0);
    assert_eq!(region.read(pos).unwrap(), Some(tag(3)));
    region.clear(pos).unwrap();
    assert!(!region.has_chunk(pos));
    assert_eq!(region.read(pos).unwrap(), None);
    region.write(ChunkPos::new(31, 31), &tag(4)).unwrap();
    assert_eq!(region.offset(ChunkPos::new(31, 31)), (2 << 8) | 1);
    region.close().unwrap();
    assert_eq!(fs::metadata(path).unwrap().len() % 4096, 0);
}

#[test]
fn oversized_external_streams_convert_and_delete() {
    let dir = Scratch::new();
    let pos = ChunkPos::new(-1, 32);
    let external = dir.0.join("c.-1.32.mcc");
    let mut region = RegionFile::open(
        dir.0.join("r.-1.1.mca"),
        &dir.0,
        RegionFileVersion::None,
        true,
    )
    .unwrap();
    let bytes = vec![23; 256 * 4096];
    region.write_bytes(pos, &bytes).unwrap();
    assert_eq!(region.offset(pos) & 255, 1);
    assert_eq!(fs::read(&external).unwrap(), bytes);
    assert_eq!(region.read_bytes(pos).unwrap(), Some(bytes));
    assert!(region.does_chunk_exist(pos));
    region.write(pos, &tag(1)).unwrap();
    assert!(!external.exists());
    assert_eq!(region.read(pos).unwrap(), Some(tag(1)));
    region.write_bytes(pos, &vec![7; 256 * 4096]).unwrap();
    region.clear(pos).unwrap();
    assert!(!external.exists());
}

#[test]
fn corrupt_header_recovery_matches_vanilla() {
    let dir = Scratch::new();
    let path = dir.0.join("r.0.0.mca");
    let mut bytes = vec![0u8; 8192];
    for (index, offset) in [(0, 0x101u32), (1, 0x200), (2, 0xffffff01), (3, 0x201)] {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&offset.to_be_bytes());
    }
    fs::write(&path, &bytes).unwrap();
    let mut region = RegionFile::open(&path, &dir.0, RegionFileVersion::Deflate, false).unwrap();
    for x in 0..3 {
        assert!(!region.has_chunk(ChunkPos::new(x, 0)));
    }
    // Vanilla accepts a sector starting exactly at EOF; reading its missing stream recovers.
    assert!(region.has_chunk(ChunkPos::new(3, 0)));
    assert!(region.read(ChunkPos::new(3, 0)).unwrap().is_none());
    region.write(ChunkPos::new(0, 0), &tag(8)).unwrap();
    assert_eq!(region.read(ChunkPos::new(0, 0)).unwrap(), Some(tag(8)));
    region.close().unwrap();
    let truncated = dir.0.join("r.1.0.mca");
    fs::write(&truncated, [0, 0, 1]).unwrap();
    let mut region =
        RegionFile::open(truncated, &dir.0, RegionFileVersion::Deflate, false).unwrap();
    assert!(region.read(ChunkPos::new(32, 0)).unwrap().is_none());
}

#[test]
fn invalid_chunk_streams_recover_without_panicking() {
    let dir = Scratch::new();
    let path = dir.0.join("r.0.0.mca");
    let pos = ChunkPos::new(0, 0);
    let mut region = RegionFile::open(&path, &dir.0, RegionFileVersion::None, false).unwrap();
    region.write(pos, &tag(1)).unwrap();
    region.close().unwrap();
    drop(region);
    for header in [
        [0, 0, 0, 0, 3],
        [255, 255, 255, 255, 3],
        [0, 0, 32, 0, 3],
        [0, 0, 0, 1, 99],
        [0, 0, 0, 1, 131],
    ] {
        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(8192)).unwrap();
        file.write_all(&header).unwrap();
        drop(file);
        let mut region = RegionFile::open(&path, &dir.0, RegionFileVersion::None, false).unwrap();
        assert!(region.read(pos).unwrap().is_none());
    }
}

#[test]
fn worker_orders_pending_reads_flushes_and_shutdown() {
    let dir = Scratch::new();
    let mut worker = IoWorker::new(&dir.0, RegionFileVersion::Lz4, false).unwrap();
    let pos = ChunkPos::new(-2, -3);
    let tasks: Vec<_> = (0..100).map(|n| worker.store(pos, Some(tag(n)))).collect();
    assert_eq!(worker.load(pos).wait().unwrap(), Some(tag(99)));
    worker.synchronize(true).wait().unwrap();
    for task in tasks {
        task.wait().unwrap();
    }
    let remove = worker.store(pos, None);
    assert_eq!(worker.load(pos).wait().unwrap(), None);
    remove.wait().unwrap();
    let write = worker.store(pos, Some(tag(101)));
    worker.shutdown().wait().unwrap();
    write.wait().unwrap();
    assert!(worker.load(pos).wait().is_err());
    worker.close().unwrap();
    let mut storage = RegionFileStorage::new(&dir.0, RegionFileVersion::Deflate, false);
    assert_eq!(storage.read(pos).unwrap(), Some(tag(101)));
}

#[test]
fn worker_surfaces_write_and_flush_failures() {
    let dir = Scratch::new();
    let file = dir.0.join("not-a-directory");
    fs::write(&file, []).unwrap();
    let mut worker = IoWorker::new(&file, RegionFileVersion::Deflate, false).unwrap();
    let write = worker.store(ChunkPos::new(0, 0), Some(tag(1)));
    assert!(write.wait().is_err());
    assert!(worker.synchronize(true).wait().is_err());
    worker.close().unwrap();
}

#[test]
fn session_lock_backups_and_saved_data_dirty_tracking() {
    let dir = Scratch::new();
    let mut access = LevelStorageAccess::open(&dir.0).unwrap();
    assert!(DirectoryLock::is_locked(&dir.0).unwrap());
    assert!(LevelStorageAccess::open(&dir.0).is_err());
    assert_eq!(
        fs::read(dir.0.join("session.lock")).unwrap(),
        "☃".as_bytes()
    );
    let data = LevelData {
        data: tag(1),
        root_extra: [("opaque", Tag::Long(9))].into_iter().collect(),
    };
    access.save_level_data(&data).unwrap();
    let mut next = data.clone();
    next.data.insert("value", Tag::Int(2));
    access.save_level_data(&next).unwrap();
    assert_eq!(access.read_level_data().unwrap(), next);
    assert_eq!(
        crate::files::read_nbt(&dir.0.join("level.dat_old")).unwrap(),
        data.to_nbt().unwrap()
    );
    fs::write(dir.0.join("level.dat"), b"corrupt").unwrap();
    assert_eq!(access.read_level_data().unwrap(), data);
    let uuid = "00000000-0000-0000-0000-000000000001";
    let players = access.player_data();
    players.save(uuid, &tag(3)).unwrap();
    players.save(uuid, &tag(4)).unwrap();
    fs::write(dir.0.join(format!("players/data/{uuid}.dat")), b"corrupt").unwrap();
    assert_eq!(players.load(uuid).unwrap(), Some(tag(3)));
    assert!(fs::read_dir(dir.0.join("players/data")).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .contains("_corrupted_")
    }));
    let store = SavedDataStorage::new(dir.0.join("data"));
    let mut saved = SavedData::new(Tag::List(vec![Tag::Int(1)].into()));
    assert!(saved.is_dirty());
    store.save("example:folder/test", &mut saved).unwrap();
    assert!(!saved.is_dirty());
    saved.value_mut().clone_from(&Tag::String("changed".into()));
    assert!(saved.is_dirty());
    store.save("example:folder/test", &mut saved).unwrap();
    assert_eq!(
        store
            .load::<Tag>("example:folder/test")
            .unwrap()
            .unwrap()
            .value(),
        saved.value()
    );
    assert!(store.data_file("example:../../escape").is_err());
    access.close().unwrap();
    assert!(!DirectoryLock::is_locked(&dir.0).unwrap());
    assert!(access.read_level_data().is_err());
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn files_below(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files_below(&entry.path(), files);
        } else {
            files.push(entry.path());
        }
    }
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world")
}

/// Reads every committed vanilla file, reserializes typed schemas, and checks all NBT/JSON values.
fn rewrite_fixture(destination: &Path) -> usize {
    copy_tree(&fixture(), destination);
    let mut paths = Vec::new();
    files_below(&fixture(), &mut paths);
    let mut chunks = 0;
    for path in paths {
        let relative = path.strip_prefix(fixture()).unwrap();
        let target = destination.join(relative);
        match path.extension().and_then(|e| e.to_str()) {
            Some("mca") => {
                let name = path.file_stem().unwrap().to_str().unwrap();
                let coords: Vec<_> = name.split('.').collect();
                let rx: i32 = coords[1].parse().unwrap();
                let rz: i32 = coords[2].parse().unwrap();
                let mut source = RegionFile::open(
                    &path,
                    path.parent().unwrap(),
                    RegionFileVersion::Deflate,
                    false,
                )
                .unwrap();
                // Exercise Java LZ4 compatibility when vanilla boots this rewritten copy.
                let mut dest = RegionFile::open(
                    &target,
                    target.parent().unwrap(),
                    RegionFileVersion::Lz4,
                    true,
                )
                .unwrap();
                for z in 0..32 {
                    for x in 0..32 {
                        let pos = ChunkPos::new(rx * 32 + x, rz * 32 + z);
                        if let Some(tag) = source.read(pos).unwrap() {
                            let rewritten = match path
                                .parent()
                                .unwrap()
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap()
                            {
                                "region" => SerializableChunkData::parse(tag.clone())
                                    .unwrap()
                                    .write()
                                    .unwrap(),
                                "entities" => {
                                    EntityChunk::parse(tag.clone()).unwrap().write().unwrap()
                                }
                                "poi" => PoiChunk::parse(tag.clone()).unwrap().write().unwrap(),
                                _ => panic!("unexpected region directory"),
                            };
                            assert_eq!(rewritten, tag, "{} {pos:?}", path.display());
                            dest.write(pos, &rewritten).unwrap();
                            assert_eq!(dest.read(pos).unwrap(), Some(tag));
                            chunks += 1;
                        }
                    }
                }
                dest.close().unwrap();
            }
            Some("dat") => {
                let tag = crate::files::read_nbt(&path).unwrap();
                let rewritten = if relative == Path::new("level.dat") {
                    LevelData::from_nbt(tag.clone()).unwrap().to_nbt().unwrap()
                } else if relative.starts_with("players/data") {
                    crate::check_tag_version(&tag).unwrap();
                    tag.clone()
                } else {
                    SavedData::<Tag>::from_nbt(tag.clone())
                        .unwrap()
                        .to_nbt()
                        .unwrap()
                };
                assert_eq!(rewritten, tag, "{}", path.display());
                crate::files::write_nbt(&target, &rewritten, None).unwrap();
                assert_eq!(crate::files::read_nbt(&target).unwrap(), tag);
            }
            Some("json") => {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                crate::files::write_json(&target, &value).unwrap();
                let read: serde_json::Value =
                    serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
                assert_eq!(read, value);
            }
            _ => {}
        }
    }
    chunks
}

#[test]
fn vanilla_files_round_trip_without_field_loss() {
    let dir = Scratch::new();
    let chunks = rewrite_fixture(&dir.0);
    assert!(
        chunks >= 4,
        "fixtures must contain terrain, entities and POIs"
    );
}

#[test]
#[ignore = "requires vanilla 26.3 server jar and JDK 25; run explicitly on the builder"]
fn vanilla_accepts_rewritten_world() {
    let dir = Scratch::new();
    let chunks = rewrite_fixture(&dir.0);
    let status = std::process::Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vanilla.py"))
        .arg("verify")
        .arg(&dir.0)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "vanilla rejected {chunks} rewritten chunks"
    );
}

#[test]
fn reads_all_vanilla_compression_vectors() {
    let expected = crate::compression::vector_payload();
    for version in [
        RegionFileVersion::Gzip,
        RegionFileVersion::Deflate,
        RegionFileVersion::None,
        RegionFileVersion::Lz4,
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/compression/compression-{}.bin",
            version as u8
        ));
        assert_eq!(
            version.decompress(&fs::read(path).unwrap()).unwrap(),
            expected
        );
    }
}

#[test]
#[ignore = "requires vanilla 26.3 server jar and JDK 25; run explicitly on the builder"]
fn vanilla_accepts_all_rust_compression_streams() {
    let dir = Scratch::new();
    let payload = crate::compression::vector_payload();
    for version in [
        RegionFileVersion::Gzip,
        RegionFileVersion::Deflate,
        RegionFileVersion::None,
        RegionFileVersion::Lz4,
    ] {
        fs::write(
            dir.0.join(format!("compression-{}.bin", version as u8)),
            version.compress(&payload).unwrap(),
        )
        .unwrap();
    }
    let status = std::process::Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vanilla.py"))
        .arg("compression")
        .arg(&dir.0)
        .status()
        .unwrap();
    assert!(status.success());
}
