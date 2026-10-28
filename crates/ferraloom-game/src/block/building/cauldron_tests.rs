use super::{cauldron, tests::POS};
use crate::{
    block::BlockMethod,
    level::{
        Level,
        distance_manager::FullChunkStatus,
        effects::LevelEffect,
        game_event::test_support::{self, Observation},
    },
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    ChunkPos, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use ferraloom_worldgen::biome_manager::Precipitation;
use std::{collections::BTreeSet, io::Read, sync::LazyLock};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/cauldron-precipitation.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
fn level() -> Level {
    let mut level = super::fire_powder_tests::level();
    level.install_shared(Box::default());
    crate::level::drop_fixtures::install(&mut level);
    level
        .set_chunk_loading_status(ChunkPos::new(0, 0), FullChunkStatus::BlockTicking, true)
        .unwrap();
    level
        .set_block_state(POS.below(1), Block::STONE.default_state())
        .unwrap();
    test_support::observe(&mut level);
    test_support::take(&mut level);
    level
}
fn precipitation(name: &str) -> Precipitation {
    match name {
        "none" => Precipitation::None,
        "rain" => Precipitation::Rain,
        "snow" => Precipitation::Snow,
        _ => panic!("unexpected native precipitation {name}"),
    }
}
fn trace(level: &mut Level) -> String {
    test_support::take(level)
        .into_iter()
        .filter_map(|event| match event {
            Observation::Effect(LevelEffect::BlockUpdated { pos, old, new }) => {
                assert_eq!(pos, POS);
                Some(format!("B:{}>{}", old.id(), new.id()))
            }
            Observation::GameEvent {
                event,
                position,
                source,
                affected_state,
            } => {
                assert_eq!(position, Vec3::at_center_of(POS.into()));
                assert!(source.is_none());
                Some(format!(
                    "G:{},{}",
                    event.protocol_id(),
                    affected_state.map_or(-1, |s| i32::from(s.id()))
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn cauldron_fill_chances_match_native_draws_at_threshold_neighbors_and_none_gate() {
    let mut level = level();
    let mut outcomes = BTreeSet::new();
    let mut checked = 0;
    for line in VECTORS.lines().filter(|line| line.starts_with("chance\t")) {
        let f: Vec<_> = line.split('\t').collect();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
            0,
        );
        let actual = cauldron::should_handle_precipitation(&mut level, precipitation(f[1]));
        assert_eq!(actual, f[3] == "true", "{line}");
        assert_eq!(level.random().next_long().to_string(), f[4], "{line}");
        outcomes.insert((f[1], actual));
        checked += 1;
    }
    assert_eq!(checked, 780);
    assert_eq!(
        outcomes,
        BTreeSet::from([
            ("none", false),
            ("rain", false),
            ("rain", true),
            ("snow", false),
            ("snow", true)
        ])
    );
}
#[test]
fn cauldron_precipitation_matches_native_levels_event_contexts_and_rng_order() {
    let mut level = level();
    let mut checked = 0;
    let mut receivers = BTreeSet::new();
    for line in VECTORS.lines().filter(|line| line.starts_with("handle\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        level.set_block_state(POS, state).unwrap();
        test_support::take(&mut level);
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        level
            .required_block_behavior(state.block(), BlockMethod::HandlePrecipitation)
            .unwrap()
            .handle_precipitation(state, POS, precipitation(f[2]), &mut level)
            .unwrap();
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            f[4],
            "{line}"
        );
        assert_eq!(trace(&mut level), f[5], "{line}");
        assert_eq!(level.random().next_long().to_string(), f[6], "{line}");
        receivers.insert(state.block().protocol_id());
        checked += 1;
    }
    assert_eq!(checked, 6240);
    assert_eq!(receivers.len(), 4);
}
