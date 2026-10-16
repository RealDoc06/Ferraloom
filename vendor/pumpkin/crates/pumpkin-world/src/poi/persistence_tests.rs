use super::*;

// Exact zlib payloads from stopped player-data-q53khzzp source worlds, slot 0,
// byte offset 8192 of dimensions/minecraft/overworld/poi/r.8.0.mca.
// Java MCA SHA256: 1456555003422ac4558138c33beb185b2e29fb178b39262a87ee8abf056be50f
// Rust MCA SHA256: c0ade9f5a74a86818dd175fa3e3e8893db20ff17f2c94327b023e5ca207ebecc
// Official 26.3 inner JAR SHA256:
// a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6.
// PoiRecord$Packed.lambda$static$0 requires pos/type and defaults tickets to 0;
// BlockPos.lambda$static$0 enforces three Ints; PoiSection$Packed defaults Valid
// to false. These persistence fixtures do not establish full POI/DFU parity.
const OFFICIAL_CHUNK: &[u8] = &[
    0x78, 0x9c, 0x1d, 0x8d, 0x41, 0x0a, 0xc2, 0x30, 0x14, 0x44, 0x27, 0x8d, 0x62, 0x4d, 0x5d, 0x08,
    0x3d, 0x89, 0x5d, 0xba, 0xf6, 0x04, 0x0a, 0xdd, 0xca, 0x27, 0xfd, 0xc1, 0x8f, 0x36, 0x29, 0xc9,
    0xdf, 0x78, 0x1a, 0xaf, 0xe3, 0xb1, 0x4c, 0x5d, 0xcd, 0x63, 0x1e, 0xc3, 0x38, 0xc0, 0xa1, 0xbd,
    0xb1, 0x57, 0x49, 0xb1, 0x38, 0x34, 0xa7, 0xc1, 0x60, 0x3b, 0xd2, 0x4b, 0x26, 0xb3, 0xc7, 0xee,
    0xca, 0x3e, 0xe5, 0xa9, 0xf6, 0x80, 0xe9, 0x60, 0x97, 0x54, 0x2a, 0x59, 0xe0, 0xd8, 0xd4, 0xfc,
    0xae, 0x6c, 0x71, 0x08, 0x99, 0xf9, 0xae, 0xe2, 0x9f, 0xac, 0xab, 0x36, 0x2d, 0x36, 0xfa, 0x5e,
    0x18, 0xfd, 0x2c, 0x91, 0x7d, 0xa6, 0xa0, 0xe7, 0x20, 0xe5, 0xc1, 0x79, 0xa6, 0xf8, 0x9f, 0x77,
    0x17, 0x52, 0x1a, 0x39, 0x97, 0xfa, 0x09, 0xf4, 0x1f, 0xfc, 0x00, 0x3c, 0x5b, 0x1e, 0xb1,
];

const LEGACY_CHUNK: &[u8] = &[
    0x78, 0x9c, 0x2d, 0xcd, 0xc1, 0x0a, 0xc2, 0x30, 0x10, 0x04, 0xd0, 0x49, 0xa3, 0x58, 0xa3, 0xa0,
    0xe0, 0x97, 0xe8, 0xd1, 0xb3, 0x5f, 0xa0, 0xd0, 0xab, 0x84, 0x74, 0x83, 0x41, 0x9b, 0xca, 0xee,
    0x1e, 0xac, 0x5f, 0xe8, 0x67, 0xb9, 0x87, 0x9e, 0x1e, 0x33, 0x30, 0x4c, 0x00, 0x02, 0xda, 0x1b,
    0x25, 0x2d, 0x63, 0x95, 0x80, 0xe6, 0x78, 0x5a, 0x63, 0x75, 0xa5, 0x34, 0x72, 0x6f, 0x11, 0x70,
    0x1e, 0x6e, 0x32, 0x7f, 0x2d, 0x16, 0x3a, 0xbd, 0x09, 0x87, 0xa1, 0x54, 0x4a, 0x1c, 0xb3, 0x9e,
    0x73, 0x91, 0x07, 0xf1, 0x10, 0xab, 0xc7, 0x36, 0x33, 0xd1, 0x5d, 0x4b, 0x7a, 0x92, 0xca, 0x3c,
    0xfb, 0x9a, 0xde, 0xfc, 0x00, 0xfb, 0x06, 0x0e, 0xcb, 0x2e, 0xbe, 0x4a, 0xef, 0xac, 0xc4, 0xe6,
    0x12, 0x35, 0x76, 0xc4, 0x62, 0xaf, 0xc0, 0x4e, 0xf0, 0x07, 0x3e, 0x48, 0x1e, 0x95,
];

// Build actual MCA framing independently of PoiRegion's writer.
pub(super) fn write_mca(path: &Path, chunks: &[(usize, &[u8])]) {
    let mut bytes = vec![0; HEADER_SIZE];
    for &(slot, compressed) in chunks {
        let sector = bytes.len() / SECTOR_SIZE;
        let count = (compressed.len() + 5).div_ceil(SECTOR_SIZE);
        let location = ((sector as u32) << 8) | count as u32;
        bytes[slot * 4..slot * 4 + 4].copy_from_slice(&location.to_be_bytes());
        bytes.extend_from_slice(&((compressed.len() + 1) as u32).to_be_bytes());
        bytes.push(2);
        bytes.extend_from_slice(compressed);
        bytes.resize((sector + count) * SECTOR_SIZE, 0);
    }
    std::fs::write(path, bytes).unwrap();
}

pub(super) fn compress_fixture(root: NbtCompound) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&crate::nbt_io::encode_compound(root).unwrap())
        .unwrap();
    encoder.finish().unwrap()
}

pub(super) fn read_mca_chunk(path: &Path, slot: usize) -> NbtCompound {
    let bytes = std::fs::read(path).unwrap();
    let location = u32::from_be_bytes(bytes[slot * 4..slot * 4 + 4].try_into().unwrap());
    let offset = (location >> 8) as usize * SECTOR_SIZE;
    let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    assert!(offset >= HEADER_SIZE);
    assert!(length <= (location & 255) as usize * SECTOR_SIZE - 4);
    assert_eq!(bytes[offset + 4], 2);
    let mut decoded = Vec::new();
    ZlibDecoder::new(&bytes[offset + 5..offset + 4 + length])
        .read_to_end(&mut decoded)
        .unwrap();
    let mut cursor = Cursor::new(decoded);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    let nbt = pumpkin_nbt::Nbt::read(&mut reader).unwrap();
    assert_eq!(cursor.position(), cursor.get_ref().len() as u64);
    nbt.root_tag
}

pub(super) fn record(pos: [i32; 3], poi_type: &str, tickets: Option<i32>) -> NbtCompound {
    let mut record = NbtCompound::new();
    record.put("pos", NbtTag::IntArray(pos.to_vec()));
    record.put_string("type", poi_type.to_owned());
    if let Some(tickets) = tickets {
        record.put_int("free_tickets", tickets);
    }
    record
}

pub(super) fn section(valid: Option<i8>, records: Vec<NbtCompound>) -> NbtCompound {
    let mut section = NbtCompound::new();
    if let Some(valid) = valid {
        section.put_byte("Valid", valid);
    }
    section.put_list(
        "Records",
        records.into_iter().map(NbtTag::Compound).collect(),
    );
    section
}

pub(super) fn chunk(sections: &[(&str, NbtCompound)]) -> NbtCompound {
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", 5023);
    let mut compound = NbtCompound::new();
    for (key, section) in sections {
        compound.put_compound(key, section.clone());
    }
    root.put_compound("Sections", compound);
    root
}

fn section_at<'a>(root: &'a NbtCompound, section_y: &str) -> &'a NbtCompound {
    assert_eq!(root.get("DataVersion"), Some(&NbtTag::Int(5023)));
    root.get_compound("Sections")
        .unwrap()
        .get_compound(section_y)
        .unwrap()
}

fn assert_record(section: &NbtCompound, pos: [i32; 3], poi_type: &str, tickets: i32) {
    let records = section.get_list("Records").unwrap();
    let found: Vec<_> = records
        .iter()
        .map(|tag| tag.extract_compound().unwrap())
        .filter(|record| record.get_int_array("pos") == Some(pos.as_slice()))
        .collect();
    assert_eq!(found.len(), 1, "expected exactly one POI at {pos:?}");
    assert_eq!(found[0], &record(pos, poi_type, Some(tickets)));
}

#[test]
fn stopped_official_poi_mca_import_and_rust_export() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    write_mca(&path, &[(0, OFFICIAL_CHUNK)]);
    let mut loaded = PoiRegion::load(&path).unwrap();
    assert!(!loaded.is_dirty());
    assert_eq!(loaded.get_all().len(), 1);
    let fisherman = loaded.get_all()[0];
    assert_eq!(fisherman.pos(), BlockPos::new(4098, 200, 3));
    assert_eq!(fisherman.poi_type, "minecraft:fisherman");
    assert_eq!(fisherman.free_tickets, 1);

    loaded.add(PoiEntry::new_portal(BlockPos::new(4099, 200, 3)));
    loaded.save(&path).unwrap();
    let root = read_mca_chunk(&path, 0);
    let section = section_at(&root, "12");
    assert_eq!(section.get("Valid"), Some(&NbtTag::Byte(1)));
    assert_eq!(section.get_list("Records").unwrap().len(), 2);
    assert_record(section, [4098, 200, 3], "minecraft:fisherman", 1);
    assert_record(section, [4099, 200, 3], POI_TYPE_NETHER_PORTAL, 0);
    assert_eq!(PoiRegion::load(&path).unwrap().get_all().len(), 2);
}

#[test]
fn multiple_negative_pois_keep_sections_validity_types_and_tickets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.-1.-1.mca");
    let first = compress_fixture(chunk(&[
        (
            "-2",
            section(
                Some(0),
                vec![
                    record([-1, -17, -1], "minecraft:fisherman", Some(1)),
                    record([-2, -17, -1], POI_TYPE_NETHER_PORTAL, None),
                ],
            ),
        ),
        (
            "0",
            section(
                Some(1),
                vec![record([-1, 1, -1], "minecraft:meeting", Some(7))],
            ),
        ),
        ("4", section(None, vec![])),
    ]));
    let second = compress_fixture(chunk(&[(
        "2",
        section(
            Some(1),
            vec![record([-17, 32, -1], "minecraft:home", Some(0))],
        ),
    )]));
    write_mca(&path, &[(1023, &first), (1022, &second)]);
    let mut loaded = PoiRegion::load(&path).unwrap();
    assert_eq!(loaded.get_all().len(), 4);
    let added = BlockPos::new(-3, -17, -1);
    loaded.add(PoiEntry::new_portal(added));
    loaded.save(&path).unwrap();
    let root = read_mca_chunk(&path, 1023);
    let negative = section_at(&root, "-2");
    assert_eq!(negative.get("Valid"), Some(&NbtTag::Byte(0)));
    assert_eq!(negative.get_list("Records").unwrap().len(), 3);
    assert_record(negative, [-1, -17, -1], "minecraft:fisherman", 1);
    assert_record(negative, [-2, -17, -1], POI_TYPE_NETHER_PORTAL, 0);
    assert_record(negative, [-3, -17, -1], POI_TYPE_NETHER_PORTAL, 0);
    assert_record(section_at(&root, "0"), [-1, 1, -1], "minecraft:meeting", 7);
    let empty = section_at(&root, "4");
    assert_eq!(empty.get("Valid"), Some(&NbtTag::Byte(0)));
    assert!(empty.get_list("Records").unwrap().is_empty());
    let sibling = read_mca_chunk(&path, 1022);
    assert_record(
        section_at(&sibling, "2"),
        [-17, 32, -1],
        "minecraft:home",
        0,
    );
    let mut storage = PoiStorage::new(directory.path().to_owned());
    assert_eq!(
        storage
            .get_in_square(BlockPos::new(-8, 0, -8), 16, Some("minecraft:fisherman"))
            .unwrap(),
        vec![BlockPos::new(-1, -17, -1)]
    );
    assert_eq!(storage.total_poi_count(), 5);

    // Removing the final record must persist an empty section, not resurrect it.
    assert!(storage.remove(&BlockPos::new(-1, 1, -1)).unwrap());
    storage.save_all().unwrap();
    let root = read_mca_chunk(&path, 1023);
    assert!(
        section_at(&root, "0")
            .get_list("Records")
            .unwrap()
            .is_empty()
    );
    assert_eq!(PoiRegion::load(&path).unwrap().get_all().len(), 4);
}

#[test]
fn stopped_private_xyz_migrates_on_save_without_gameplay_edits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    write_mca(&path, &[(0, LEGACY_CHUNK)]);
    let mut storage = PoiStorage::new(directory.path().to_owned());
    assert_eq!(
        storage
            .get_in_square(BlockPos::new(4098, 200, 3), 0, None)
            .unwrap(),
        vec![BlockPos::new(4098, 200, 3)]
    );
    assert!(storage.regions[&(8, 0)].is_dirty());
    storage.save_all().unwrap();
    assert!(!storage.regions[&(8, 0)].is_dirty());
    let root = read_mca_chunk(&path, 0);
    assert_record(
        section_at(&root, "12"),
        [4098, 200, 3],
        "minecraft:fisherman",
        1,
    );
    assert!(!PoiRegion::load(&path).unwrap().is_dirty());
}

#[test]
fn malformed_positions_and_mixed_legacy_layouts_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    let base = record([4098, 200, 3], "minecraft:fisherman", Some(1));
    let mut invalid = Vec::new();
    for pos in [
        None,
        Some(NbtTag::Int(4098)),
        Some(NbtTag::List(vec![
            NbtTag::Int(4098),
            NbtTag::Int(200),
            NbtTag::Int(3),
        ])),
        Some(NbtTag::ByteArray(vec![1, 2, 3].into())),
        Some(NbtTag::LongArray(vec![4098, 200, 3])),
        Some(NbtTag::IntArray(vec![])),
        Some(NbtTag::IntArray(vec![4098, 200])),
        Some(NbtTag::IntArray(vec![4098, 200, 3, 4])),
        Some(NbtTag::IntArray(vec![4098, 199, 3])), // valid, same section control below
    ] {
        let mut rec = base.clone();
        rec.child_tags.remove("pos");
        if let Some(pos) = pos {
            rec.put("pos", pos);
        }
        invalid.push(rec);
    }
    // A nearby position is accepted; exact coordinate values are not hardcoded.
    let control = invalid.pop().unwrap();
    write_mca(
        &path,
        &[(
            0,
            &compress_fixture(chunk(&[("12", section(Some(1), vec![control]))])),
        )],
    );
    assert_eq!(PoiRegion::load(&path).unwrap().get_all()[0].y, 199);
    for axis in ["x", "y", "z"] {
        let mut rec = base.clone();
        rec.put_int(axis, 0);
        invalid.push(rec);
    }
    let mut wrong_section = base.clone();
    wrong_section.put("pos", NbtTag::IntArray(vec![4098, -1, 3]));
    invalid.push(wrong_section);
    let mut wrong_chunk = base.clone();
    wrong_chunk.put("pos", NbtTag::IntArray(vec![4114, 200, 3]));
    invalid.push(wrong_chunk);
    let mut no_type = base.clone();
    no_type.child_tags.remove("type");
    invalid.push(no_type);
    let mut bad_tickets = base;
    bad_tickets.put_string("free_tickets", "1".to_owned());
    invalid.push(bad_tickets);
    for rec in invalid {
        let root = chunk(&[("12", section(Some(1), vec![rec]))]);
        write_mca(&path, &[(0, &compress_fixture(root))]);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            PoiRegion::load(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn private_xyz_requires_legacy_version_complete_ints_and_no_pos() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    let mut base = record([4098, 200, 3], "minecraft:fisherman", Some(1));
    base.child_tags.remove("pos");
    for (axis, value) in [("x", 4098), ("y", 200), ("z", 3)] {
        base.put_int(axis, value);
    }
    let mut cases = vec![(base.clone(), true)];
    for axis in ["x", "y", "z"] {
        let mut missing = base.clone();
        missing.child_tags.remove(axis);
        cases.push((missing, false));
        let mut wrong_type = base.clone();
        wrong_type.put_string(axis, "0".to_owned());
        cases.push((wrong_type, false));
    }
    for pos in [
        NbtTag::IntArray(vec![4098, 200, 3]),
        NbtTag::IntArray(vec![4098, 200]),
    ] {
        let mut mixed = base.clone();
        mixed.put("pos", pos);
        cases.push((mixed, false));
    }
    for version in [3955, 5023] {
        for (record, complete_xyz) in &cases {
            let mut root = chunk(&[("12", section(Some(1), vec![record.clone()]))]);
            root.put_int("DataVersion", version);
            write_mca(&path, &[(0, &compress_fixture(root))]);
            assert_eq!(
                PoiRegion::load(&path).is_ok(),
                version == 3955 && *complete_xyz
            );
        }
    }
}

#[test]
fn malformed_structure_versions_and_duplicate_positions_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    let record = record([4098, 200, 3], "minecraft:fisherman", Some(1));
    let base_section = section(Some(1), vec![record.clone()]);
    let base = chunk(&[("12", base_section.clone())]);
    let mut invalid = Vec::new();
    for version in [
        None,
        Some(NbtTag::Int(3955)),
        Some(NbtTag::Int(5022)),
        Some(NbtTag::Int(5024)),
        Some(NbtTag::String("5023".into())),
    ] {
        let mut root = base.clone();
        root.child_tags.remove("DataVersion");
        if let Some(version) = version {
            root.put("DataVersion", version);
        }
        invalid.push(root);
    }
    let mut root = base;
    root.child_tags.remove("Sections");
    invalid.push(root);
    invalid.push(chunk(&[(
        "12",
        section(Some(1), vec![record.clone(), record]),
    )]));
    for name in ["wrong", "012"] {
        invalid.push(chunk(&[(name, base_section.clone())]));
    }
    for (key, value) in [
        ("Valid", NbtTag::Int(1)),
        ("Records", NbtTag::Int(1)),
        ("Records", NbtTag::List(vec![NbtTag::Int(1)])),
    ] {
        let mut section = base_section.clone();
        section.put(key, value);
        invalid.push(chunk(&[("12", section)]));
    }
    for root in invalid {
        write_mca(&path, &[(0, &compress_fixture(root))]);
        assert_eq!(
            PoiRegion::load(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn corrupt_region_refuses_mutation_but_healthy_region_still_saves() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    write_mca(&path, &[(0, OFFICIAL_CHUNK)]);
    let base = std::fs::read(&path).unwrap();
    let mut corruptions = vec![base[..HEADER_SIZE - 1].to_vec()];
    for location in [1u32, 0x101, 0x200, 0x301] {
        let mut bytes = base.clone();
        bytes[..4].copy_from_slice(&location.to_be_bytes());
        corruptions.push(bytes);
    }
    for length in [0u32, 1, 4093] {
        let mut bytes = base.clone();
        bytes[8192..8196].copy_from_slice(&length.to_be_bytes());
        corruptions.push(bytes);
    }
    let mut unsupported = base.clone();
    unsupported[8196] = 1;
    corruptions.push(unsupported);
    let mut bad_zlib = base.clone();
    bad_zlib[8197] = 0;
    corruptions.push(bad_zlib);
    let mut overlap = base;
    overlap[4..8].copy_from_slice(&0x201u32.to_be_bytes());
    corruptions.push(overlap);
    // A healthy first chunk must not hide a malformed second chunk.
    let bad = compress_fixture(chunk(&[("12", section(Some(1), vec![NbtCompound::new()]))]));
    write_mca(&path, &[(0, OFFICIAL_CHUNK), (1, &bad)]);
    corruptions.push(std::fs::read(&path).unwrap());

    for bytes in corruptions {
        std::fs::write(&path, &bytes).unwrap();
        let mut storage = PoiStorage::new(directory.path().to_owned());
        let healthy = BlockPos::new(1, 64, 1);
        storage.add_portal(healthy).unwrap();
        let error = PoiRegion::load(&path).unwrap_err();
        let failed_pos = BlockPos::new(4099, 200, 3);
        assert_eq!(
            storage.add_portal(failed_pos).unwrap_err().kind(),
            error.kind()
        );
        assert_eq!(
            storage
                .add(failed_pos, "minecraft:home")
                .unwrap_err()
                .kind(),
            error.kind()
        );
        assert_eq!(
            storage
                .add_with_free_tickets(failed_pos, "minecraft:home", 1)
                .unwrap_err()
                .kind(),
            error.kind()
        );
        assert_eq!(
            storage
                .get_in_square(failed_pos, 0, None)
                .unwrap_err()
                .kind(),
            error.kind()
        );
        assert_eq!(
            storage
                .find_closest_matching(failed_pos, 0, |_| true)
                .unwrap_err()
                .kind(),
            error.kind()
        );
        assert_eq!(storage.loaded_region_count(), 1);
        assert_eq!(storage.total_poi_count(), 1);
        // Even a later readable file must not silently clear the failure in
        // this storage instance; reopening is an explicit recovery boundary.
        write_mca(&path, &[(0, OFFICIAL_CHUNK)]);
        assert_eq!(
            storage
                .remove(&BlockPos::new(4098, 200, 3))
                .unwrap_err()
                .kind(),
            error.kind()
        );
        std::fs::write(&path, &bytes).unwrap();
        storage.add_portal(healthy).unwrap();
        assert_eq!(storage.save_all().unwrap_err().kind(), error.kind());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let saved = PoiRegion::load(&directory.path().join("r.0.0.mca")).unwrap();
        assert_eq!(saved.get_all()[0].pos(), healthy);
    }
}

#[test]
fn wrong_region_write_preserves_previous_mca_and_dirty_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.8.0.mca");
    write_mca(&path, &[(0, OFFICIAL_CHUNK)]);
    let before = std::fs::read(&path).unwrap();
    let mut loaded = PoiRegion::load(&path).unwrap();
    // Same local MCA slot in another region must not alias the existing chunk.
    loaded.add(PoiEntry::new_portal(BlockPos::new(4610, 200, 3)));
    assert_eq!(
        loaded.save(&path).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(loaded.is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn save_all_persists_healthy_regions_when_another_region_cannot_encode() {
    let directory = tempfile::tempdir().unwrap();
    let mut storage = PoiStorage::new(directory.path().to_owned());
    let bad_pos = BlockPos::new(1, 64, 1);
    let healthy_pos = BlockPos::new(513, 64, 1);
    storage.add_portal(bad_pos).unwrap();
    storage.save_all().unwrap();
    let bad_path = directory.path().join("r.0.0.mca");
    let previous = std::fs::read(&bad_path).unwrap();
    storage.add(bad_pos, &"😀".repeat(10_923)).unwrap();
    storage.add_portal(healthy_pos).unwrap();
    assert_eq!(
        storage.save_all().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(std::fs::read(&bad_path).unwrap(), previous);
    assert!(storage.regions[&(0, 0)].is_dirty());
    assert!(!storage.regions[&(1, 0)].is_dirty());
    let healthy = PoiRegion::load(&directory.path().join("r.1.0.mca")).unwrap();
    assert_eq!(healthy.get_all()[0].pos(), healthy_pos);
}
