#![allow(clippy::type_complexity)]

use crate::*;
use ferraloom_data::{Block, BlockState};
use std::sync::Arc;

mod vanilla {
    include!("vanilla_vectors.rs");
}

fn registry() -> Arc<IdMap<u16>> {
    Arc::new(IdMap::new((0..600).collect()).unwrap())
}
fn sample(raw: &[u64]) -> [u64; 6] {
    let mut values = [0; 6];
    for (dest, &value) in values[..4].iter_mut().zip(raw) {
        *dest = value;
    }
    if raw.len() >= 2 {
        values[4] = raw[raw.len() - 2];
    }
    if let Some(&last) = raw.last() {
        values[5] = last;
    }
    values
}
fn prefix(values: &[u16]) -> [u16; 8] {
    let mut result = [u16::MAX; 8];
    for (dest, &value) in result.iter_mut().zip(values) {
        *dest = value;
    }
    result
}

#[test]
fn bit_layout_and_fill_match_real_jar() {
    for &(bits, expected_raw, expected_fill) in vanilla::BITS {
        let mask = (1_u64 << bits) - 1;
        let values: Vec<_> = (0..37)
            .map(|i| ((i as u64 * 0x9e3779b9 + 17) & mask) as u32)
            .collect();
        let mut storage = SimpleBitStorage::from_values(bits, &values).unwrap();
        assert_eq!(storage.raw(), expected_raw, "bits={bits}");
        for (i, &value) in values.iter().enumerate() {
            assert_eq!(storage.get(i), value);
        }
        assert_eq!(storage.get_and_set(17, 0), values[17]);
        storage.fill((mask / 3) as u32);
        assert!(
            storage.raw().iter().all(|&word| word == expected_fill),
            "fill bits={bits}"
        );
    }
    let mut zero = ZeroBitStorage::new(4096);
    assert_eq!(zero.get_and_set(4095, 0), 0);
    assert!(BitStorage::new(0, 4096).unwrap().raw().is_empty());
}

#[test]
fn all_palette_transitions_and_disk_reencoding_match_real_jar() {
    for biome in [false, true] {
        let strategy = Strategy::new(
            if biome {
                ContainerKind::Biomes
            } else {
                ContainerKind::BlockStates
            },
            registry(),
        );
        let mut container = PalettedContainer::new(strategy.clone(), 599);
        let mut written = 0;
        for &(
            is_biome,
            count,
            bits,
            palette_size,
            expected_palette,
            raw,
            packed_bits,
            packed_size,
            packed_prefix,
            packed_raw,
        ) in vanilla::PALETTES
        {
            if is_biome != biome {
                continue;
            }
            while written < count {
                container.set_by_index(written * 13 % strategy.entry_count(), written as u16);
                written += 1;
            }
            assert_eq!(
                container.bits_per_entry(),
                bits,
                "biome={biome} writes={count}"
            );
            let palette = if container.palette_kind() == PaletteKind::Global {
                strategy.registry().values()
            } else {
                container.palette_entries()
            };
            assert_eq!(palette.len(), palette_size);
            assert_eq!(prefix(palette), expected_palette);
            assert_eq!(sample(container.raw()), raw);
            let packed = container.pack();
            assert_eq!(packed.bits_per_entry, Some(packed_bits));
            assert_eq!(packed.palette_entries.len(), packed_size);
            assert_eq!(prefix(&packed.palette_entries), packed_prefix);
            assert_eq!(
                sample(packed.storage.as_deref().unwrap_or_default()),
                packed_raw
            );
            let restored = PalettedContainer::unpack(strategy.clone(), packed).unwrap();
            for index in 0..strategy.entry_count() {
                assert_eq!(restored.get_by_index(index), container.get_by_index(index));
            }
        }
    }
}

#[test]
fn packing_compacts_but_mutation_and_copy_preserve_palette_state() {
    let strategy = Strategy::new(ContainerKind::BlockStates, registry());
    let mut original = PalettedContainer::new(strategy.clone(), 599);
    original.set(15, 15, 15, 7);
    let mut copy = original.clone();
    copy.set(15, 15, 15, 599);
    assert_eq!(original.get(15, 15, 15), 7);
    assert_eq!(copy.bits_per_entry(), 4);
    assert!(copy.maybe_has(|v| v == 7)); // unused in cells, still in the memory palette
    let packed = copy.pack();
    assert_eq!(packed.palette_entries, [599]);
    assert_eq!(packed.storage, None);
    assert_eq!(
        PalettedContainer::unpack(strategy, packed)
            .unwrap()
            .bits_per_entry(),
        0
    );
    for i in 0..300 {
        copy.set_by_index(i, i as u16);
    }
    assert_eq!(copy.palette_kind(), PaletteKind::Global);
    assert!(copy.maybe_has(|_| false)); // GlobalPalette#maybeHas is conservative
    assert_eq!(copy.recreate().get_by_index(0), 0); // global valueFor(0), not cell 0's palette
}

#[test]
fn uniform_section_counts_and_packing_match_real_jar() {
    let biomes = Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap()));
    for &(id, expected_counts, bits, has_storage) in vanilla::UNIFORM_SECTIONS {
        let state = BlockState::from_id(id).unwrap();
        let states = PalettedContainer::new(Strategy::block_states(), state);
        assert_eq!(states.counts(), [(state, 4096)]);
        let packed = states.pack();
        assert_eq!(packed.palette_entries, [state]);
        assert_eq!(packed.bits_per_entry, Some(bits));
        assert_eq!(packed.storage.is_some(), has_storage);
        let mut section = LevelChunkSection::from_containers(
            states,
            PalettedContainer::new(biomes.clone(), BiomeId(0)),
        );
        let [non_empty_blocks, fluids, ticking_blocks, ticking_fluids] = expected_counts;
        let expected = SectionCounts {
            non_empty_blocks,
            fluids,
            ticking_blocks,
            ticking_fluids,
        };
        assert_eq!(section.counts(), expected, "state={id}");
        section.recalc_block_counts();
        assert_eq!(section.counts(), expected, "recount state={id}");
    }
}

#[test]
fn section_incremental_and_recalculated_counts_match_real_jar() {
    let biomes = Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap()));
    let mut section = LevelChunkSection::new(Strategy::block_states(), biomes, BiomeId(0));
    let assert_counts = |section: &LevelChunkSection, expected: [u16; 4]| {
        let c = section.counts();
        assert_eq!(
            [
                c.non_empty_blocks,
                c.fluids,
                c.ticking_blocks,
                c.ticking_fluids
            ],
            expected
        );
    };
    for (i, &state) in vanilla::SECTION_STATES.iter().enumerate() {
        section.set_block_state(i as u8, 0, 0, BlockState::from_id(state).unwrap());
        assert_counts(&section, vanilla::SECTION_COUNTS[i]);
    }
    section.set_block_state(2, 0, 0, Block::AIR.default_state());
    assert_counts(&section, vanilla::SECTION_COUNTS[10]);
    section.set_block_state(7, 0, 0, Block::STONE.default_state());
    assert_counts(&section, vanilla::SECTION_COUNTS[11]);
    section.recalc_block_counts();
    assert_counts(&section, vanilla::SECTION_COUNTS[12]);
    assert!(section.has_fluid());
    assert!(section.is_randomly_ticking());
}

#[test]
fn malformed_storage_reports_width_length_and_missing_values() {
    assert!(matches!(
        SimpleBitStorage::new(33, 64),
        Err(ContainerError::InvalidBits(33))
    ));
    assert!(matches!(
        SimpleBitStorage::from_raw(5, 64, vec![0; 5]),
        Err(ContainerError::InvalidLength {
            expected: 6,
            actual: 5
        })
    ));
    let strategy = Strategy::new(ContainerKind::BlockStates, registry());
    let mut packed = PalettedContainer::new(strategy.clone(), 0).pack();
    packed.bits_per_entry = Some(4);
    assert!(matches!(
        PalettedContainer::unpack(strategy.clone(), packed),
        Err(ContainerError::DeclaredBits {
            expected: 0,
            actual: 4
        })
    ));
    let packed = PackedData {
        palette_entries: vec![0, 1],
        storage: None,
        bits_per_entry: None,
    };
    assert!(matches!(
        PalettedContainer::unpack(strategy, packed),
        Err(ContainerError::MissingStorage)
    ));
}
