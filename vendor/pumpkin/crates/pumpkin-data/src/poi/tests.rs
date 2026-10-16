use std::collections::{BTreeMap, BTreeSet};

use super::PoiType;
use crate::BlockStateId;

#[path = "primary_fixture.rs"]
mod primary_fixture;
use primary_fixture::{CATALOG, EXCLUDED_BEDS};

#[test]
fn all_21_identifiers_and_parameters_match_pinned_primary() {
    let expected: BTreeMap<_, _> = CATALOG
        .iter()
        .map(|definition| {
            (
                definition.identifier,
                (definition.max_tickets, definition.valid_range),
            )
        })
        .collect();
    let actual: BTreeMap<_, _> = PoiType::ALL
        .into_iter()
        .map(|poi| (poi.identifier(), (poi.max_tickets(), poi.valid_range())))
        .collect();

    assert_eq!(expected.len(), 21);
    assert_eq!(actual.len(), PoiType::ALL.len());
    assert_eq!(actual, expected);
}

#[test]
fn all_generated_states_match_496_state_primary_witness() {
    let mut expected = BTreeMap::new();
    let mut blocks = BTreeSet::new();
    for definition in CATALOG {
        for &(block, states) in definition.blocks {
            assert!(blocks.insert(block), "duplicate registered block {block}");
            for &raw_id in states {
                let state = BlockStateId::new(raw_id).expect("valid report state ID");
                assert_eq!(state.to_block().name, block);
                assert!(
                    expected.insert(raw_id, definition.identifier).is_none(),
                    "overlapping primary state {raw_id}"
                );
            }
        }
    }
    assert_eq!(blocks.len(), 46);
    assert_eq!(expected.len(), 496);

    // Covers every generated state, including every state outside the POI catalog.
    for raw_id in 0..BlockStateId::COUNT {
        let state = BlockStateId::new(raw_id).unwrap();
        assert_eq!(
            PoiType::from_state(state).map(PoiType::identifier),
            expected.get(&raw_id).copied(),
            "{state}"
        );
    }
}

#[test]
fn colored_bed_feet_and_all_straw_bed_states_are_unmapped() {
    let mut feet = 0;
    let mut straw = 0;
    for &(block, states) in EXCLUDED_BEDS {
        for &raw_id in states {
            let state = BlockStateId::new(raw_id).unwrap();
            assert_eq!(state.to_block().name, block);
            assert_eq!(PoiType::from_state(state), None, "{state}");
        }
        if block == "straw_bed" {
            straw += states.len();
        } else {
            feet += states.len();
        }
    }
    assert_eq!(EXCLUDED_BEDS.len(), 17);
    assert_eq!(feet, 128);
    assert_eq!(straw, 16);
}

#[test]
fn all_cauldrons_and_rod_variants_register_their_complete_state_sets() {
    for (identifier, block_count, state_count) in [
        ("minecraft:leatherworker", 4, 8),
        ("minecraft:lightning_rod", 8, 192),
    ] {
        let definition = CATALOG
            .iter()
            .find(|definition| definition.identifier == identifier)
            .unwrap();
        assert_eq!(definition.blocks.len(), block_count);
        let mut total = 0;
        for &(block, raw_ids) in definition.blocks {
            let first = BlockStateId::new(raw_ids[0]).unwrap();
            let generated: BTreeSet<_> = first
                .to_block()
                .states
                .iter()
                .map(|state| state.id.as_u16())
                .collect();
            let expected: BTreeSet<_> = raw_ids.iter().copied().collect();
            assert_eq!(generated, expected, "complete state set for {block}");
            for &raw_id in raw_ids {
                let state = BlockStateId::new(raw_id).unwrap();
                assert_eq!(
                    PoiType::from_state(state).map(PoiType::identifier),
                    Some(identifier),
                    "{state}"
                );
            }
            total += raw_ids.len();
        }
        assert_eq!(total, state_count);
    }
}
