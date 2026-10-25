use super::*;
use crate::{
    Behaviors,
    block::entity::{BlockEntityDataView, create, load_static, new_block_entity, supports},
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::parse_compound;
use ferraloom_util::{BlockPos, ChunkPos};
use std::{io::Read, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 64, 8);

fn transient_fields(entity: &CreakingHeartBlockEntity) -> String {
    let target = entity.emitter_target.map_or("null".into(), |target| {
        format!("({}, {}, {})", target.x, target.y, target.z)
    });
    format!(
        "{};{};{};{};{}",
        entity.ticks_existed, entity.ticker, entity.emitter, target, entity.output_signal
    )
}
fn level(state: BlockState, entity: BlockEntityState) -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..8,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level.set_block_state(POS, state).unwrap();
    level.set_block_entity(entity).unwrap();
    level.take_effects();
    level.try_mark_chunk_saved(ChunkPos::new(0, 0)).unwrap();
    level
}

#[test]
fn native_creaking_heart_factory_load_save_updates_and_ticker_gate_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/creaking-heart.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = [0; 2];
    for row in vectors.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        assert!(supports(state.block()));
        let mut entity = new_block_entity(POS, state).unwrap().unwrap();
        match fields[0] {
            "heart-default" => {
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[2]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    transient_fields(CreakingHeartBlockEntity::view(&entity).unwrap()),
                    fields[3],
                    "{row}"
                );
                assert_eq!(
                    entity.has_server_ticker(None).unwrap(),
                    fields[4] == "true",
                    "{row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(fields[5]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    fields[6] == "true",
                    "{row}"
                );
                checked[0] += 1;
            }
            "heart-load" => {
                let data = CreakingHeartBlockEntity::view_mut(&mut entity).unwrap();
                data.creaking_uuid = Some(Uuid::new(9, 10));
                data.ticks_existed = 73;
                data.ticker = -7;
                data.emitter = 57;
                data.emitter_target = Some(Vec3::new(1.25, 2.5, -3.75));
                data.output_signal = 9;
                let input = parse_compound(fields[3]).unwrap();
                if fields[2] == "true" {
                    let mut level = level(state, entity);
                    let handle = level.block_entity_ref(POS).unwrap().unwrap();
                    handle.load_with_components(&input, &mut level).unwrap();
                    assert!(
                        level.chunk(ChunkPos::new(0, 0)).unwrap().is_unsaved(),
                        "live load must call setChanged: {row}"
                    );
                    assert_eq!(fields[10], "dirty,analog", "{row}");
                    assert!(
                        level.take_effects().is_empty(),
                        "loading itself sends no update packet"
                    );
                    entity = level.block_entity_state(handle).unwrap().clone();
                } else {
                    entity.load_with_components(
                        &input,
                        crate::level::LevelContext::fixture(0, 63)
                            .registry_context()
                            .unwrap(),
                    );
                    assert!(fields[10].is_empty(), "{row}");
                }
                let saved = entity.save_with_full_metadata().unwrap();
                assert_eq!(saved, parse_compound(fields[4]).unwrap(), "{row}");
                assert_eq!(
                    transient_fields(CreakingHeartBlockEntity::view(&entity).unwrap()),
                    fields[5],
                    "{row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(fields[6]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    fields[7] == "true",
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    crate::block::entity::components::load(
                        Some(&parse_compound(fields[8]).unwrap()),
                        crate::level::LevelContext::fixture(0, 63)
                            .registry_context()
                            .unwrap()
                    ),
                    "{row}"
                );
                let reloaded = load_static(
                    POS,
                    state,
                    &saved,
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    reloaded.save_with_full_metadata().unwrap(),
                    parse_compound(fields[9]).unwrap(),
                    "{row}"
                );
                checked[1] += 1;
            }
            _ => panic!("unknown creaking-heart native vector: {row}"),
        }
    }
    assert_eq!(checked, [18, 576]);
}

#[test]
fn live_heart_loading_keeps_the_original_generation_and_components() {
    let state = Block::CREAKING_HEART.default_state();
    let mut entity = create(POS, state).unwrap();
    entity.data_mut().components = crate::block::entity::components::load(
        Some(&parse_compound("{'minecraft:custom_name':'kept'}").unwrap()),
        crate::level::LevelContext::fixture(0, 63)
            .registry_context()
            .unwrap(),
    );
    let mut level = level(state, entity);
    let original = level.block_entity_ref(POS).unwrap().unwrap();
    level.retain_block_entity(original).unwrap();
    level.set_block_entity(create(POS, state).unwrap()).unwrap();
    let replacement = level.block_entity_ref(POS).unwrap().unwrap();
    assert_ne!(original, replacement);
    original
        .load_custom_only(
            &parse_compound("{creaking:[I;1,2,3,4],components:{'minecraft:max_stack_size':7}}")
                .unwrap(),
            &mut level,
        )
        .unwrap();
    assert_eq!(
        level
            .block_entity::<CreakingHeartBlockEntity>(original)
            .unwrap()
            .creaking_uuid,
        Some(Uuid::new((1i64 << 32) | 2, (3i64 << 32) | 4))
    );
    assert_eq!(
        level
            .block_entity::<CreakingHeartBlockEntity>(replacement)
            .unwrap()
            .creaking_uuid,
        None
    );
    assert_eq!(
        level
            .block_entity_state(original)
            .unwrap()
            .data()
            .components,
        crate::block::entity::components::load(
            Some(&parse_compound("{'minecraft:custom_name':'kept'}").unwrap()),
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap()
        )
    );
    assert!(
        !level
            .block_entity_state(original)
            .unwrap()
            .has_server_ticker(None)
            .unwrap()
    );
    let behavior = level.block_entity_state(original).unwrap().behavior();
    assert_eq!(
        behavior.tick(original, &mut level),
        Err(LevelError::UnsupportedBlockEntity(Block::CREAKING_HEART))
    );
    level.release_block_entity(original);
}

#[test]
fn removing_a_heart_without_a_protector_is_empty_but_uuid_resolution_is_required() {
    let state = Block::CREAKING_HEART.default_state();
    let mut level = level(state, create(POS, state).unwrap());
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    let behavior = level.block_entity_state(handle).unwrap().behavior();
    let before = level
        .block_entity_state(handle)
        .unwrap()
        .save_custom_only()
        .unwrap();
    assert_eq!(behavior.pre_remove_side_effects(handle, &mut level), Ok(()));
    assert_eq!(
        level
            .block_entity_state(handle)
            .unwrap()
            .save_custom_only()
            .unwrap(),
        before
    );
    assert!(level.take_effects().is_empty());
    level
        .block_entity_mut::<CreakingHeartBlockEntity>(handle)
        .unwrap()
        .creaking_uuid = Some(Uuid::new(1, 2));
    assert_eq!(
        behavior.pre_remove_side_effects(handle, &mut level),
        Err(LevelError::UnsupportedEntity(
            ferraloom_data::EntityType::CREAKING
        ))
    );
    assert_eq!(
        level
            .block_entity::<CreakingHeartBlockEntity>(handle)
            .unwrap()
            .creaking_uuid,
        Some(Uuid::new(1, 2))
    );
    assert!(level.take_effects().is_empty());
}
