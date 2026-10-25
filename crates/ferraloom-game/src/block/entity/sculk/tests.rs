use super::*;
use crate::block::entity::{BlockEntityDataView, create, load_static};
use ferraloom_data::Block;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{DataComponentMap, ItemStack, Persistent};
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn flags(entity: &BlockEntityState) -> String {
    let data = match entity {
        BlockEntityState::SculkSensor(entity) => &entity.vibration_data,
        BlockEntityState::CalibratedSculkSensor(entity) => &entity.parent.vibration_data,
        BlockEntityState::SculkShrieker(entity) => &entity.vibration_data,
        _ => return "-".into(),
    };
    format!(
        "{},{},{}",
        data.reload_vibration_particle,
        data.travel_time_in_ticks,
        data.current_vibration.is_some()
    )
}

#[test]
fn native_sculk_listener_cursor_components_and_worldgen_round_trips_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/sculk.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = [0; 3];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let state = Block::from_name(f[1]).unwrap().default_state();
        assert!(crate::block::entity::supports(state.block()));
        let mut entity = create(POS, state).unwrap();
        assert!(entity.has_server_ticker(Some(true)).unwrap());
        match f[0] {
            "sculk" => {
                entity.load_with_components(&parse_compound(f[2]).unwrap());
                let saved = entity.save_with_full_metadata().unwrap();
                assert_eq!(saved, parse_compound(f[3]).unwrap(), "{row}");
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(f[4]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    f[5] == "true",
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    DataComponentMap::from_nbt(&Tag::Compound(parse_compound(f[6]).unwrap()))
                        .unwrap(),
                    "{row}"
                );
                assert_eq!(flags(&entity), f[7], "{row}");
                let again = load_static(POS, state, &saved).unwrap().unwrap();
                assert_eq!(
                    again.save_with_full_metadata().unwrap(),
                    parse_compound(f[8]).unwrap(),
                    "{row}"
                );
                assert_eq!(flags(&again), f[9], "{row}");
                checked[0] += 1;
            }
            "sculk-components" => {
                entity
                    .apply_components_from_item_stack(
                        &ItemStack::from_nbt(&Tag::Compound(parse_compound(f[2]).unwrap()))
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    DataComponentMap::from_nbt(&Tag::Compound(parse_compound(f[4]).unwrap()))
                        .unwrap(),
                    "{row}"
                );
                assert_eq!(flags(&entity), f[5], "{row}");
                checked[1] += 1;
            }
            "sculk-worldgen" => {
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[2]).unwrap(),
                    "{row}"
                );
                checked[2] += 1;
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(checked, [75, 4, 4]);
}

#[test]
fn calibrated_sensor_base_projection_and_cached_vibration_identity_are_distinct() {
    let mut entity = create(POS, Block::CALIBRATED_SCULK_SENSOR.default_state()).unwrap();
    let sensor = SculkSensorBlockEntity::view_mut(&mut entity).unwrap();
    sensor.last_vibration_frequency = 13;
    sensor.vibration_data.current_vibration = Some(VibrationInfo {
        game_event: ferraloom_data::registries::game_event::Id::STEP,
        distance: 2.0,
        pos: ferraloom_util::Vec3::ZERO,
        uuid: Some(ferraloom_util::Uuid::new(1, 2)),
        projectile_owner_uuid: None,
        entity: Some(crate::entity::EntityRef::fixture(
            crate::level::RegionId::FIXTURE,
            5,
        )),
    });
    let saved = entity.save_with_full_metadata().unwrap();
    let loaded = load_static(POS, Block::CALIBRATED_SCULK_SENSOR.default_state(), &saved)
        .unwrap()
        .unwrap();
    assert!(CalibratedSculkSensorBlockEntity::view(&loaded).is_some());
    let info = SculkSensorBlockEntity::view(&loaded)
        .unwrap()
        .vibration_data
        .current_vibration
        .as_ref()
        .unwrap();
    assert_eq!(info.uuid, Some(ferraloom_util::Uuid::new(1, 2)));
    assert_eq!(info.entity, None);
    assert_eq!(
        SculkSensorBlockEntity::view(&loaded)
            .unwrap()
            .last_vibration_frequency,
        13
    );
}
