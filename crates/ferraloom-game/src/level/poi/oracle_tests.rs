//! Ordered observations from actual PoiSection/PoiManager bodies in 26.3.
use super::*;
use ferraloom_storage::{IntArray, PoiRecord as PackedRecord, PoiSection as PackedSection};
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{LegacyRandomSource, RandomSource},
};
use std::collections::BTreeMap;

fn position(pos: BlockPos) -> String {
    format!("{},{},{}", pos.x, pos.y, pos.z)
}
fn positions(values: impl IntoIterator<Item = BlockPos>) -> String {
    let result = values
        .into_iter()
        .map(position)
        .collect::<Vec<_>>()
        .join(";");
    if result.is_empty() {
        "-".into()
    } else {
        result
    }
}
fn optional(value: Option<BlockPos>) -> String {
    value.map_or_else(|| "-".into(), position)
}
fn section(section: &PoiSection) -> String {
    let records = section
        .pack()
        .records
        .unwrap()
        .into_iter()
        .map(|record| {
            let pos = record.position.unwrap().0;
            format!(
                "{},{},{}={}/{}",
                pos[0],
                pos[1],
                pos[2],
                record.poi_type.unwrap().to_string_checked().unwrap(),
                record.free_tickets.unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!("{}|{records}", section.valid)
}
fn packed_record(pos: BlockPos, kind: PoiTypeId, free: i32) -> PackedRecord {
    PackedRecord {
        position: Some(IntArray(vec![pos.x, pos.y, pos.z])),
        poi_type: Some(kind.name().into()),
        free_tickets: Some(free),
        ..Default::default()
    }
}
#[test]
fn native_section_pack_refresh_and_manager_queries_preserve_value_defined_order() {
    let mut actual = BTreeMap::<String, String>::new();
    let mut rows = |key: &str, value: String| {
        assert!(actual.insert(key.into(), value).is_none());
    };
    let mut records = PoiSection::default();
    for pos in [
        BlockPos::ZERO,
        BlockPos::new(1, 0, 1),
        BlockPos::new(15, 15, 15),
        BlockPos::new(2, 1, 3),
    ] {
        records.add(PoiRecord::new(pos, PoiTypeId::HOME));
    }
    rows("section.add", section(&records));
    rows(
        "section.duplicate",
        (!records.add(PoiRecord::new(BlockPos::ZERO, PoiTypeId::HOME))).to_string(),
    );
    records.remove(BlockPos::new(1, 0, 1));
    rows("section.remove", section(&records));
    rows(
        "section.release.full",
        records
            .get_mut(BlockPos::ZERO)
            .unwrap()
            .release_ticket()
            .to_string(),
    );
    let mut invalid = PoiSection::unpack(PackedSection {
        valid: Some(false),
        records: Some(vec![
            packed_record(BlockPos::ZERO, PoiTypeId::HOME, 0),
            packed_record(BlockPos::new(1, 0, 1), PoiTypeId::HOME, 1),
        ]),
        ..Default::default()
    })
    .unwrap();
    invalid.refresh([
        (BlockPos::ZERO, PoiTypeId::MEETING),
        (BlockPos::new(2, 1, 3), PoiTypeId::MEETING),
    ]);
    rows("section.refresh", section(&invalid));
    invalid.refresh([]);
    rows("section.valid.no_refresh", section(&invalid));

    let mut manager = PoiManager::new(-1, 0);
    let center = BlockPos::ZERO;
    let homes = |kind| kind == PoiTypeId::HOME;
    for chunk in PoiManager::chunks_in_square(center, 32) {
        manager.complete_chunk_load(chunk, None).unwrap();
    }
    for pos in [
        BlockPos::new(-17, -1, 0),
        BlockPos::new(-1, 0, 0),
        center,
        BlockPos::new(1, 0, 1),
        BlockPos::new(15, 0, 0),
        BlockPos::new(16, 1, 0),
        BlockPos::new(0, 15, 16),
        BlockPos::new(0, -16, 0),
    ] {
        manager.add(pos, PoiTypeId::HOME).unwrap();
    }
    rows(
        "query.chunk",
        positions(
            manager
                .get_in_chunk(ChunkPos::ZERO, homes, Occupancy::Any)
                .unwrap()
                .into_iter()
                .map(|record| record.pos),
        ),
    );
    for radius in [0, 1, 16, 32] {
        rows(
            &format!("query.square.{radius}"),
            positions(
                manager
                    .get_in_square(homes, center, radius, Occupancy::Any)
                    .unwrap()
                    .into_iter()
                    .map(|record| record.pos),
            ),
        );
        rows(
            &format!("query.range.{radius}"),
            positions(
                manager
                    .get_in_range(homes, center, radius, Occupancy::Any)
                    .unwrap()
                    .into_iter()
                    .map(|record| record.pos),
            ),
        );
    }
    rows(
        "query.closest",
        optional(
            manager
                .find_closest(homes, |_| true, center, 16, Occupancy::Any)
                .unwrap(),
        ),
    );
    rows(
        "query.closest.filtered",
        optional(
            manager
                .find_closest(homes, |pos| pos.x > 0, center, 16, Occupancy::Any)
                .unwrap(),
        ),
    );
    rows(
        "query.find",
        optional(
            manager
                .find(homes, |pos| pos.y >= 0, center, 16, Occupancy::Any)
                .unwrap(),
        ),
    );
    rows(
        "query.all",
        positions(
            manager
                .find_all(homes, |pos| pos.x >= 0, center, 16, Occupancy::Any)
                .unwrap(),
        ),
    );
    rows(
        "query.closest_first",
        positions(
            manager
                .find_all_closest_first_with_type(homes, |_| true, center, 32, Occupancy::Any)
                .unwrap()
                .into_iter()
                .map(|(_, pos)| pos),
        ),
    );
    for index in 0..3 {
        rows(
            &format!("tickets.take.{index}"),
            optional(manager.take(homes, |_, _| true, center, 16).unwrap()),
        );
    }
    rows(
        "tickets.occupied",
        positions(
            manager
                .get_in_range(homes, center, 16, Occupancy::IsOccupied)
                .unwrap()
                .into_iter()
                .map(|record| record.pos),
        ),
    );
    rows(
        "tickets.space",
        positions(
            manager
                .get_in_range(homes, center, 16, Occupancy::HasSpace)
                .unwrap()
                .into_iter()
                .map(|record| record.pos),
        ),
    );
    rows(
        "tickets.release",
        manager
            .release(BlockPos::new(-1, 0, 0))
            .unwrap()
            .to_string(),
    );
    rows(
        "tickets.release.full",
        manager
            .release(BlockPos::new(-1, 0, 0))
            .unwrap()
            .to_string(),
    );
    rows(
        "tickets.count",
        manager
            .get_count_in_range(homes, center, 16, Occupancy::HasSpace)
            .unwrap()
            .to_string(),
    );
    let mut random = LegacyRandomSource::new(42);
    rows(
        "query.random",
        optional(
            manager
                .get_random(
                    homes,
                    |pos| pos.x >= 0,
                    center,
                    32,
                    Occupancy::Any,
                    &mut random,
                )
                .unwrap(),
        ),
    );
    rows("query.random.next", random.next_int().to_string());
    rows(
        "query.exists",
        manager.exists(center, homes).unwrap().to_string(),
    );
    rows(
        "query.type",
        manager
            .get_type(center)
            .unwrap()
            .map_or_else(|| "-".into(), |kind| kind.name().to_owned()),
    );
    let expected: BTreeMap<String, String> =
        include_str!("../../../tests/java/level/poi-runtime.tsv")
            .lines()
            .map(|line| {
                let (key, value) = line.split_once('\t').unwrap();
                (key.into(), value.into())
            })
            .collect();
    assert_eq!(actual.len(), expected.len());
    for (key, expected) in expected {
        assert_eq!(
            actual.get(&key),
            Some(&expected),
            "native POI observation {key}"
        );
    }
}
