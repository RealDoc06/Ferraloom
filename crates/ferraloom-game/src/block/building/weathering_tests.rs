use super::{tests::POS, weathering_copper};
use crate::{
    block::{BlockMethod, UPDATE_LIMIT, UpdateFlags},
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    BlockPos,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{collections::BTreeSet, io::Read, sync::LazyLock};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/weathering-copper.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
fn position(value: &str) -> BlockPos {
    let xyz: Vec<i32> = value.split(',').map(|v| v.parse().unwrap()).collect();
    BlockPos::new(xyz[0], xyz[1], xyz[2])
}
fn state(value: &str) -> BlockState {
    BlockState::from_id(value.parse().unwrap()).unwrap()
}
fn cells(value: &str) -> impl Iterator<Item = (BlockPos, BlockState)> + '_ {
    value.split(';').map(|cell| {
        let (pos, id) = cell.split_once(':').unwrap();
        (position(pos), state(id))
    })
}
fn level() -> Level {
    let mut level = super::amethyst_tests::level(POS);
    level.install_shared(Box::default());
    crate::level::drop_fixtures::install(&mut level);
    level
}
fn set(level: &mut Level, pos: BlockPos, state: BlockState) {
    level
        .set_block(pos, state, UpdateFlags::SKIP_ALL_SIDEEFFECTS, UPDATE_LIMIT)
        .unwrap();
}
fn replay(modes: &[&str], expected: usize) {
    let mut level = level();
    let mut previous = BTreeSet::new();
    let mut checked = 0;
    for line in VECTORS
        .lines()
        .filter(|line| modes.contains(&line.split('\t').next().unwrap()))
    {
        let f: Vec<_> = line.split('\t').collect();
        for pos in std::mem::take(&mut previous) {
            set(&mut level, pos, Block::AIR.default_state());
        }
        for (pos, state) in cells(f[5]) {
            set(&mut level, pos, state);
            previous.insert(pos);
        }
        if f[4] == "3" {
            level.remove_block_entity(POS).unwrap();
        }
        let before = level.block_entity_ref(POS).unwrap();
        level.take_effects();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        let original = state(f[1]);
        let pos = position(f[2]);
        let result = match f[0] {
            "next" => weathering_copper::next_state(original, pos, &mut level).unwrap(),
            "change" => {
                weathering_copper::change_over_time(original, pos, &mut level).unwrap();
                None
            }
            "random" | "guard" => {
                level
                    .required_block_behavior(original.block(), BlockMethod::RandomTick)
                    .unwrap()
                    .random_tick(original, pos, &mut level)
                    .unwrap();
                None
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result.map_or(-1, |s| i32::from(s.id())).to_string(),
            f[6],
            "{line}"
        );
        let trace = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| {
                if let LevelEffect::BlockUpdated { pos, old, new } = effect {
                    previous.insert(pos);
                    Some(format!(
                        "{},{},{}:{}>{}",
                        pos.x,
                        pos.y,
                        pos.z,
                        old.id(),
                        new.id()
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(trace, f[8], "{line}");
        for (pos, expected) in cells(f[7]) {
            assert_eq!(
                level.get_block_state(pos).unwrap(),
                expected,
                "{line}, {pos:?}"
            );
            previous.insert(pos);
        }
        let after = level.block_entity_ref(POS).unwrap();
        assert_eq!(after.is_some(), f[9] == "true", "{line}");
        assert_eq!(before == after, f[10] == "true", "{line}");
        assert_eq!(f[11], "9");
        assert!(!level.has_chunk_at(BlockPos::new(82, 64, -24)), "{line}");
        assert_eq!(level.random().next_long().to_string(), f[12], "{line}");
        checked += 1;
    }
    assert_eq!(checked, expected);
}
#[test]
fn shared_weathering_chance_matches_native_neighbor_ages_boundaries_wax_and_rng() {
    replay(&["next"], 9240);
}
#[test]
fn weathering_class_ticks_match_native_states_write_order_and_receiver_identity() {
    replay(&["change", "random"], 14664);
}
#[test]
fn upper_doors_right_chests_and_non_chest_receivers_skip_before_queries_and_rng() {
    replay(&["guard"], 320);
}
