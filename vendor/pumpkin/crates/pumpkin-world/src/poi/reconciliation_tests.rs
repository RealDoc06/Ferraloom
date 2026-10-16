//! Stage A source contracts. All six tests are UNRUN until root's native receipt.

use super::persistence_tests::{
    chunk as nbt_chunk, compress_fixture, read_mca_chunk, record, section, write_mca,
};
use super::*;
use crate::chunk::format::anvil::SingleChunkDataSerializer;
use crate::chunk::{ChunkData, ChunkSections, PoiReconciliationStamp, palette::BiomePalette};
use pumpkin_data::{Block, BlockStateId};

fn fixture() -> (tempfile::TempDir, PoiStorage, ChunkData) {
    let directory = tempfile::tempdir().unwrap();
    let mut storage = PoiStorage::new(directory.path().to_owned());
    storage.preload_poi_region(ChunkPos::new(0, 0)).unwrap();
    (directory, storage, ChunkData::empty(0, 0))
}

fn reconcile(
    chunk: &ChunkData,
    storage: &mut PoiStorage,
) -> Result<PoiReconciliationStamp, PoiReconcileError> {
    let mut loaded = storage.loaded_poi_region(ChunkPos::new(chunk.x, chunk.z))?;
    chunk.read_for_poi().reconcile(&mut loaded)
}

fn home_states() -> Vec<BlockStateId> {
    Block::WHITE_BED
        .states
        .iter()
        .map(|state| state.id)
        .filter(|state| PoiType::from_state(*state) == Some(PoiType::Home))
        .collect()
}

fn entry(storage: &PoiStorage, pos: BlockPos) -> &PoiEntry {
    &storage.regions[&PoiStorage::region_coords(&pos)].entries[&(pos.0.x, pos.0.y, pos.0.z)]
}

#[test]
fn actual_writers_mark_changes_and_reject_noops() {
    let (_directory, mut storage, chunk) = fixture();
    let initial = reconcile(&chunk, &mut storage).unwrap();
    let home = home_states()[0];
    assert_eq!(
        chunk.set_block_absolute_y(1, 0, 2, BlockStateId::AIR),
        BlockStateId::AIR
    );
    assert_eq!(
        chunk.set_block_absolute_y_if(1, 0, 2, home, |_| false),
        None
    );
    assert_eq!(
        chunk.set_block_absolute_y_if(1, -65, 2, home, |_| true),
        None
    );
    assert_eq!(
        chunk.set_block_absolute_y_if(1, 320, 2, home, |_| true),
        None
    );
    assert_eq!(
        chunk
            .set_blocks_batch([(1, 0, 2, BlockStateId::AIR), (1, 320, 2, home)])
            .len(),
        1
    );
    assert!(chunk.read_for_poi().is_current(&initial));

    assert_eq!(chunk.set_block_absolute_y(1, 0, 2, home), BlockStateId::AIR);
    assert!(!chunk.read_for_poi().is_current(&initial));
    let single = reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(
        entry(&storage, BlockPos::new(1, 0, 2)).poi_type,
        PoiType::Home.identifier()
    );
    assert_eq!(
        chunk.set_block_absolute_y_if(1, 0, 2, Block::BELL.default_state.id, |old| old == home),
        Some(home)
    );
    assert!(!chunk.read_for_poi().is_current(&single));
    let conditional = reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, BlockPos::new(1, 0, 2)).free_tickets, 32);

    assert_eq!(
        chunk.set_blocks_batch([(1, 0, 2, BlockStateId::AIR), (15, 15, 15, home)]),
        vec![
            (1, 0, 2, Block::BELL.default_state.id),
            (15, 15, 15, BlockStateId::AIR)
        ]
    );
    assert!(!chunk.read_for_poi().is_current(&conditional));
    let batch = reconcile(&chunk, &mut storage).unwrap();
    assert!(!storage.regions[&(0, 0)].entries.contains_key(&(1, 0, 2)));
    assert_eq!(entry(&storage, BlockPos::new(15, 15, 15)).free_tickets, 1);
    // Changed non-POI states also invalidate the captured terrain revision.
    chunk.set_block_absolute_y(3, 0, 2, Block::STONE.default_state.id);
    assert!(!chunk.read_for_poi().is_current(&batch));
}

#[test]
fn same_type_changes_retain_depleted_and_unclamped_tickets() {
    let (_directory, mut storage, chunk) = fixture();
    let homes = home_states();
    chunk.set_blocks_batch([
        (1, 0, 1, homes[0]),
        (2, 0, 1, Block::CAULDRON.default_state.id),
    ]);
    reconcile(&chunk, &mut storage).unwrap();
    let bed = BlockPos::new(1, 0, 1);
    let cauldron = BlockPos::new(2, 0, 1);
    for (index, tickets) in [0, -7, i32::MAX].into_iter().enumerate() {
        storage
            .add_with_free_tickets(bed, PoiType::Home.identifier(), tickets)
            .unwrap();
        storage
            .add_with_free_tickets(cauldron, PoiType::Leatherworker.identifier(), tickets)
            .unwrap();
        let form = if index % 2 == 0 {
            Block::WATER_CAULDRON.default_state.id
        } else {
            Block::CAULDRON.default_state.id
        };
        chunk.set_blocks_batch([(1, 0, 1, homes[index + 1]), (2, 0, 1, form)]);
        reconcile(&chunk, &mut storage).unwrap();
        assert_eq!(entry(&storage, bed).free_tickets, tickets);
        assert_eq!(entry(&storage, cauldron).free_tickets, tickets);
    }
}

#[test]
fn home_to_meeting_replaces_type_and_keeps_sibling_tickets() {
    let (_directory, mut storage, chunk) = fixture();
    let home = home_states()[0];
    chunk.set_blocks_batch([(1, 0, 1, home), (1, 16, 1, home)]);
    reconcile(&chunk, &mut storage).unwrap();
    for y in [0, 16] {
        storage
            .add_with_free_tickets(BlockPos::new(1, y, 1), PoiType::Home.identifier(), 0)
            .unwrap();
    }
    chunk.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    reconcile(&chunk, &mut storage).unwrap();
    let region = &storage.regions[&(0, 0)];
    let encoded = PoiRegion::compress_chunk_data(&region.get_chunk_data(0, 0).unwrap()).unwrap();
    let decoded = PoiRegion::decompress_chunk_data(&encoded).unwrap();
    assert_eq!(decoded.sections["0"].valid, 1);
    assert_eq!(
        decoded.sections["0"].records[0].poi_type,
        PoiType::Meeting.identifier()
    );
    assert_eq!(decoded.sections["0"].records[0].free_tickets, 32);
    assert_eq!(
        decoded.sections["1"].records[0].poi_type,
        PoiType::Home.identifier()
    );
    assert_eq!(decoded.sections["1"].records[0].free_tickets, 0);
}

#[test]
fn coalesced_home_aba_resets_and_late_writer_invalidates_stamp() {
    let (_directory, mut storage, chunk) = fixture();
    let home = home_states()[0];
    let pos = BlockPos::new(15, 15, 15);
    chunk.set_block_absolute_y(15, 15, 15, home);
    reconcile(&chunk, &mut storage).unwrap();
    storage
        .add_with_free_tickets(pos, PoiType::Home.identifier(), 0)
        .unwrap();
    chunk.set_blocks_batch(
        (0..256).flat_map(|_| [(15, 15, 15, BlockStateId::AIR), (15, 15, 15, home)]),
    );
    let prepared = reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, pos).free_tickets, 1);
    storage
        .add_with_free_tickets(pos, PoiType::Home.identifier(), 0)
        .unwrap();

    std::thread::scope(|scope| {
        let terrain = chunk.read_for_poi();
        let (ready, receiver) = std::sync::mpsc::channel();
        let chunk = &chunk;
        let writer = scope.spawn(move || {
            ready.send(()).unwrap();
            chunk.set_blocks_batch([(15, 15, 15, BlockStateId::AIR), (15, 15, 15, home)]);
        });
        receiver.recv().unwrap();
        assert!(chunk.section.block_sections.try_write().is_err());
        assert_eq!(terrain.palettes()[4].get(15, 15, 15), home);
        let mut loaded = storage.loaded_poi_region(ChunkPos::new(0, 0)).unwrap();
        assert_eq!(terrain.reconcile(&mut loaded).unwrap(), prepared);
        drop(terrain);
        writer.join().unwrap();
    });
    assert!(!chunk.read_for_poi().is_current(&prepared));
    assert_eq!(entry(&storage, pos).free_tickets, 0);
    reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, pos).free_tickets, 1);
}

#[test]
fn initial_palettes_empty_sections_and_final_removal_are_complete() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.-1.-1.mca");
    // A previously written Valid1 with missing records must still be repaired.
    write_mca(
        &path,
        &[(
            1023,
            &compress_fixture(nbt_chunk(&[(
                "-1",
                section(
                    Some(1),
                    vec![record([-2, -1, -1], PoiType::Home.identifier(), Some(0))],
                ),
            )])),
        )],
    );
    let mut storage = PoiStorage::new(directory.path().to_owned());
    storage.preload_poi_region(ChunkPos::new(-1, -1)).unwrap();
    let mut chunk = ChunkData::empty(-1, -1);
    let mut palette = BlockPalette::default();
    palette.set(14, 15, 15, home_states()[0]);
    palette.set(15, 15, 15, home_states()[0]);
    chunk.section = ChunkSections::from_palettes(
        vec![palette, BlockPalette::default()].into_boxed_slice(),
        vec![BiomePalette::default(); 2].into_boxed_slice(),
        -16,
    );
    reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, BlockPos::new(-1, -1, -1)).free_tickets, 1);
    assert_eq!(entry(&storage, BlockPos::new(-2, -1, -1)).free_tickets, 0);
    let data = storage.regions[&(-1, -1)].get_chunk_data(-1, -1).unwrap();
    assert_eq!(data.sections["0"].valid, 1);
    assert!(data.sections["0"].records.is_empty());
    // The actual terrain decoder must also start unreconciled, regardless of
    // the old in-memory stamp or a previously saved POI Valid1.
    let encoded = chunk.to_bytes().unwrap();
    let chunk = ChunkData::from_bytes(&encoded, ChunkPos::new(-1, -1)).unwrap();
    assert!(storage.remove(&BlockPos::new(-1, -1, -1)).unwrap());
    reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, BlockPos::new(-1, -1, -1)).free_tickets, 1);
    assert_eq!(entry(&storage, BlockPos::new(-2, -1, -1)).free_tickets, 0);
    chunk.set_blocks_batch([
        (14, -1, 15, BlockStateId::AIR),
        (15, -1, 15, BlockStateId::AIR),
    ]);
    reconcile(&chunk, &mut storage).unwrap();
    storage.save_all().unwrap();
    let root = read_mca_chunk(&path, 1023);
    for y in ["-1", "0"] {
        let section = root
            .get_compound("Sections")
            .unwrap()
            .get_compound(y)
            .unwrap();
        assert_eq!(section.get_byte("Valid"), Some(1));
        assert!(section.get_list("Records").unwrap().is_empty());
    }
}

#[test]
fn refusals_preserve_tickets_reset_history_and_original_region_bytes() {
    let (directory, mut storage, chunk) = fixture();
    let home = home_states()[0];
    let pos = BlockPos::new(1, 0, 1);
    chunk.set_block_absolute_y(1, 0, 1, home);
    let initial = reconcile(&chunk, &mut storage).unwrap();
    storage
        .add_with_free_tickets(pos, PoiType::Home.identifier(), 0)
        .unwrap();
    chunk.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
    let mut unavailable = PoiStorage::new(directory.path().join("not-loaded"));
    assert!(matches!(
        reconcile(&chunk, &mut unavailable),
        Err(PoiReconcileError::RegionNotLoaded { .. })
    ));
    let mut wrong = storage.loaded_poi_region(ChunkPos::new(1, 0)).unwrap();
    assert!(matches!(
        chunk.read_for_poi().reconcile(&mut wrong),
        Err(PoiReconcileError::WrongChunk)
    ));

    let unknown = BlockPos::new(2, 0, 1);
    let untouched = BlockPos::new(2, 32, 1);
    for at in [unknown, untouched] {
        storage.add_with_free_tickets(at, "future:poi", -9).unwrap();
    }
    assert!(matches!(
        reconcile(&chunk, &mut storage),
        Err(PoiReconcileError::UnknownType { .. })
    ));
    assert!(!chunk.read_for_poi().is_current(&initial));
    assert_eq!(entry(&storage, pos).free_tickets, 0);
    assert_eq!(entry(&storage, unknown).free_tickets, -9);
    // Explicitly remove the unsupported target record, then retry. This is not
    // automatic recovery, and the unknown record in the clean sibling survives.
    assert!(storage.remove(&unknown).unwrap());
    reconcile(&chunk, &mut storage).unwrap();
    assert_eq!(entry(&storage, pos).free_tickets, 1);
    storage.save_all().unwrap();
    let reloaded = PoiRegion::load(&directory.path().join("r.0.0.mca")).unwrap();
    let retained = &reloaded.entries[&(2, 32, 1)];
    assert_eq!(retained.poi_type, "future:poi");
    assert_eq!(retained.free_tickets, -9);

    // Every extension level is refused before load publication, for both exact
    // admitted layouts. Healthy regions still flush despite the sticky failure.
    for version in [DATA_VERSION, LEGACY_XYZ_DATA_VERSION] {
        for scope in ["root", "section", "record"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("r.8.0.mca");
            let mut record = record([4098, 0, 3], PoiType::Home.identifier(), Some(0));
            if version == LEGACY_XYZ_DATA_VERSION {
                record.child_tags.remove("pos");
                for (axis, value) in [("x", 4098), ("y", 0), ("z", 3)] {
                    record.put_int(axis, value);
                }
            }
            if scope == "record" {
                record.put_int("future", 7);
            }
            let mut poi_section = section(Some(1), vec![record]);
            if scope == "section" {
                poi_section.put_int("future", 7);
            }
            let mut root = nbt_chunk(&[("0", poi_section)]);
            root.put_int("DataVersion", version);
            if scope == "root" {
                root.put_int("future", 7);
            }
            write_mca(&path, &[(0, &compress_fixture(root))]);
            let before = std::fs::read(&path).unwrap();
            let mut storage = PoiStorage::new(directory.path().to_owned());
            let refused = ChunkPos::new(256, 0);
            assert_eq!(
                storage.preload_poi_region(refused).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert!(matches!(
                storage.loaded_poi_region(refused),
                Err(PoiReconcileError::RegionUnavailable(_))
            ));
            assert_eq!(storage.loaded_region_count(), 0);
            let healthy = BlockPos::new(1, 0, 1);
            storage.add_portal(healthy).unwrap();
            assert_eq!(
                storage.save_all().unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                PoiRegion::load(&directory.path().join("r.0.0.mca"))
                    .unwrap()
                    .get_all()[0]
                    .pos(),
                healthy
            );
            write_mca(
                &path,
                &[(
                    0,
                    &compress_fixture(nbt_chunk(&[("0", section(Some(1), vec![]))])),
                )],
            );
            assert_eq!(
                storage.preload_poi_region(refused).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}
