use crate::{
    Behaviors,
    block::{UnportedBehavior, containers::crafter::CrafterBlockEntity, entity::create},
    level::{
        Level, LevelContext, Region,
        chunk::LevelChunk,
        distance_manager::FullChunkStatus,
        effects::LevelEffect,
        environment::{ClockSnapshot, EnvironmentDefinitions},
        weather::ChunkLightSnapshot,
    },
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_util::{BlockPos, ChunkPos};
use std::{collections::BTreeMap, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 64, 8);

fn level(has_skylight: bool) -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level.set_unported_behavior(UnportedBehavior::Strict);
    level.set_has_skylight(has_skylight);
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    level.set_block_tags(Arc::new(TagSet::<Block>::load(&resources).unwrap()));
    let definitions = Arc::new(
        EnvironmentDefinitions::load(&ferraloom_data::datapack::ResourceManager::new(Vec::new()))
            .unwrap(),
    );
    let dimension = definitions.dimension(&serde_json::json!({"attributes":{"visual/sun_angle":0.0,"gameplay/sky_light_level":15.0},"timelines":[]})).unwrap();
    level
        .set_environment_attributes(
            definitions,
            &dimension,
            "test".parse().unwrap(),
            &[],
            ClockSnapshot::default(),
        )
        .unwrap();
    let chunk = ChunkPos::new(0, 0);
    level
        .add_chunk(LevelChunk::new(
            chunk,
            0..16,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    level
        .set_chunk_loading_status(chunk, FullChunkStatus::BlockTicking, true)
        .unwrap();
    level.set_chunk_block_ticking(chunk, true).unwrap();
    level
        .update_entity_chunk_status(chunk, crate::level::entity::Visibility::Ticking)
        .unwrap();
    for request in level.take_entity_storage_requests() {
        if let crate::level::entity::EntityStorageRequest::Load { id, .. } = request {
            level.complete_entity_chunk_load(id, Ok(None)).unwrap();
        }
    }
    level.process_pending_entity_loads().unwrap();
    level.set_light_snapshot(
        chunk,
        ChunkLightSnapshot {
            sky: BTreeMap::from([(4, ferraloom_light::DataLayer::new(13))]),
            block: BTreeMap::new(),
        },
    );
    level
}

#[test]
fn selected_crafter_and_daylight_tickers_match_native_dispatch_and_one_tick() {
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/blocks/entities/server-tickers.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let daylight = f[0] == "tick-daylight";
        let sky = !daylight || f[2] == "true";
        let mut level = level(sky);
        level.set_game_time(if daylight { f[3].parse().unwrap() } else { 1 });
        level.set_block_state(POS, state).unwrap();
        let mut receiver = create(POS, state).unwrap();
        if let crate::block::entity::BlockEntityState::Crafter(crafter) = &mut receiver {
            crafter.crafting_ticks_remaining = f[2].parse().unwrap();
        }
        assert_eq!(
            receiver.has_server_ticker(Some(sky)).unwrap(),
            if daylight { f[4] == "true" } else { true }
        );
        level.set_block_entity(receiver).unwrap();
        let handle = level.block_entity_ref(POS).unwrap().unwrap();
        level.take_effects();
        level.tick_block_entities(true).unwrap();
        let (state_field, count_field) = if daylight {
            (5, 6)
        } else {
            assert_eq!(
                level
                    .block_entity::<CrafterBlockEntity>(handle)
                    .unwrap()
                    .crafting_ticks_remaining,
                f[3].parse::<i32>().unwrap(),
                "{row}"
            );
            (4, 5)
        };
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[state_field].parse::<u16>().unwrap(),
            "{row}"
        );
        let changes = level
            .take_effects()
            .into_iter()
            .filter(|effect| matches!(effect, LevelEffect::BlockUpdated { pos, .. } if *pos == POS))
            .count();
        assert_eq!(changes, f[count_field].parse::<usize>().unwrap(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 22);
}
