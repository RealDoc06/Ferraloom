use super::{pointed_dripstone, tests::POS};
use crate::{
    block::BlockMethod,
    level::{
        Level,
        effects::LevelEffect,
        environment::{ClockSnapshot, EnvironmentDefinitions},
        game_event::test_support::{self, Observation},
    },
};
use ferraloom_data::{
    Block, BlockState, datapack::ResourceManager, registries::fluid::Id as Fluid,
};
use ferraloom_protocol::play::clientbound::Packet;
use ferraloom_util::{
    BlockPos, ChunkPos, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};

const LOW: i32 = -24;
const HIGH: i32 = 3;
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/dripstone.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static ENVIRONMENT: LazyLock<Arc<EnvironmentDefinitions>> = LazyLock::new(|| {
    Arc::new(EnvironmentDefinitions::load(&ResourceManager::new(Vec::new())).unwrap())
});
fn pos(at: BlockPos) -> String {
    format!("{},{},{}", at.x, at.y, at.z)
}
fn level(initial: &str, evaporates: bool) -> Level {
    let mut level = super::fire_powder_tests::level();
    level.install_shared(Box::default());
    crate::level::drop_fixtures::install(&mut level);
    level
        .set_chunk_loading_status(
            ChunkPos::new(0, 0),
            crate::level::distance_manager::FullChunkStatus::BlockTicking,
            true,
        )
        .unwrap();
    let dimension = ENVIRONMENT
        .dimension(&serde_json::json!({
            "attributes": {"gameplay/water_evaporates": evaporates}, "timelines": []
        }))
        .unwrap();
    level
        .set_environment_attributes(
            Arc::clone(&ENVIRONMENT),
            &dimension,
            "minecraft:overworld".parse().unwrap(),
            &[],
            ClockSnapshot::default(),
        )
        .unwrap();
    let column: Vec<_> = initial
        .split(',')
        .map(|id| BlockState::from_id(id.parse().unwrap()).unwrap())
        .collect();
    assert_eq!(column.len(), (HIGH - LOW + 1) as usize);
    for (y, state) in (LOW..=HIGH).zip(column) {
        level.set_block_state(POS.above(y), state).unwrap();
    }
    test_support::observe(&mut level);
    test_support::take(&mut level);
    level
}
fn column(level: &Level) -> String {
    (LOW..=HIGH)
        .map(|y| {
            level
                .get_block_state(POS.above(y))
                .unwrap()
                .id()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn trace(level: &mut Level) -> String {
    test_support::take(level)
        .into_iter()
        .filter_map(|event| match event {
            Observation::Effect(LevelEffect::BlockUpdated { pos: at, old, new }) => {
                Some(format!("B:{}:{}>{}", pos(at), old.id(), new.id()))
            }
            Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => match *packet {
                Packet::LevelEvent(event) => Some(format!(
                    "E:{}:{},{}",
                    pos(event.pos),
                    event.event_type,
                    event.data
                )),
                _ => None,
            },
            Observation::GameEvent {
                event,
                position,
                source,
                affected_state,
            } => {
                assert!(source.is_none());
                let at = BlockPos::containing(position.x, position.y, position.z);
                assert_eq!(position, Vec3::at_center_of(at.into()));
                Some(format!(
                    "G:{}:{},{}",
                    pos(at),
                    event.protocol_id(),
                    affected_state.map_or(-1, |s| i32::from(s.id()))
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
/// Compare native LevelChunkTicks#pack order, including delays and priorities;
/// do not sort the observations or run the still-independent cauldron lifecycle.
fn scheduled(level: &Level) -> String {
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
        .unwrap();
    let mut rows = Vec::new();
    for tick in saved.block_ticks.unwrap_or_default() {
        let block = Block::from_name(&tick.id.unwrap().to_string_checked().unwrap()).unwrap();
        rows.push(format!(
            "B:{},{},{}:{},{},{}",
            tick.x.unwrap(),
            tick.y.unwrap(),
            tick.z.unwrap(),
            block.protocol_id(),
            tick.delay.unwrap(),
            tick.priority.unwrap()
        ));
    }
    for tick in saved.fluid_ticks.unwrap_or_default() {
        let fluid = Fluid::from_name(&tick.id.unwrap().to_string_checked().unwrap()).unwrap();
        rows.push(format!(
            "F:{},{},{}:{},{},{}",
            tick.x.unwrap(),
            tick.y.unwrap(),
            tick.z.unwrap(),
            fluid.protocol_id(),
            tick.delay.unwrap(),
            tick.priority.unwrap()
        ));
    }
    rows.join(";")
}
fn compare(mode: &str) -> usize {
    let mut matched = 0;
    for line in VECTORS.lines().filter(|line| line.starts_with(mode)) {
        let f: Vec<_> = line.split('\t').collect();
        let mut level = level(f[1], f[2] == "true");
        let state = level.get_block_state(POS).unwrap();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        match mode {
            "random" => level
                .required_block_behavior(state.block(), BlockMethod::RandomTick)
                .unwrap()
                .random_tick(state, POS, &mut level)
                .unwrap(),
            "transfer" => pointed_dripstone::maybe_transfer_fluid(
                state,
                POS,
                f32::from_bits(f[4].parse::<i32>().unwrap() as u32),
                &mut level,
            )
            .unwrap(),
            "grow" => pointed_dripstone::grow_if_possible(state, POS, &mut level).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(column(&level), f[5], "{line}");
        assert_eq!(trace(&mut level), f[6], "{line}");
        assert_eq!(scheduled(&level), f[7], "{line}");
        assert_eq!(level.random().next_long().to_string(), f[8], "{line}");
        matched += 1;
    }
    matched
}
#[test]
fn dripstone_random_ticks_match_native_mutations_events_queues_and_rng() {
    assert_eq!(compare("random"), 3636);
}
#[test]
fn dripstone_transfers_match_native_probability_edges_roots_gaps_and_collisions() {
    assert_eq!(compare("transfer"), 12832);
}
#[test]
fn dripstone_growth_matches_native_search_limits_stalagmites_and_merge_order() {
    assert_eq!(compare("grow"), 3208);
}
