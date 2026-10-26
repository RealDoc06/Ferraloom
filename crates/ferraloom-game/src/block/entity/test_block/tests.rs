use super::*;
use crate::block::entity::{BlockEntityDataView, create, load_static, new_block_entity, supports};
use ferraloom_data::BlockState;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::BlockPos;
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn prime(entity: &mut BlockEntityState) {
    let entity = TestBlockEntity::view_mut(entity).unwrap();
    entity.mode = TestBlockMode::Log;
    entity.message = "old".into();
    entity.powered = true;
    entity.triggered = true;
}
fn scalar(entity: &BlockEntityState) -> String {
    let entity = TestBlockEntity::view(entity).unwrap();
    format!(
        "{},{},{},{}",
        entity.mode_name(),
        entity.message.to_string_lossy(),
        entity.powered,
        entity.triggered
    )
}

#[test]
fn native_test_block_factory_load_save_update_and_components_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/test-block-factory.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let context = crate::level::LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut checked = [0; 2];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        assert!(supports(state.block()), "{row}");
        assert!(super::super::supports_creation(state.block()), "{row}");
        let mut entity = new_block_entity(POS, state).unwrap().unwrap();
        assert_eq!(
            entity.save_custom_only().unwrap(),
            create(POS, state).unwrap().save_custom_only().unwrap()
        );
        match f[0] {
            "test-block-data" => {
                if f[2] != "@new" {
                    prime(&mut entity);
                    entity.load_with_components(&parse_compound(f[2]).unwrap(), registries);
                }
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "{row}"
                );
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
                    super::super::components::load(
                        Some(&parse_compound(f[6]).unwrap()),
                        registries
                    ),
                    "{row}"
                );
                assert_eq!(scalar(&entity), f[7], "{row}");
                let reloaded = load_static(
                    POS,
                    state,
                    &entity.save_with_full_metadata().unwrap(),
                    registries,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    reloaded.save_with_full_metadata().unwrap(),
                    parse_compound(f[8]).unwrap(),
                    "{row}"
                );
                assert!(!TestBlockEntity::view(&reloaded).unwrap().triggered);
                checked[0] += 1;
            }
            "test-block-components" => {
                prime(&mut entity);
                let stack = ItemStack::from_nbt(&ferraloom_nbt::Tag::Compound(
                    parse_compound(f[2]).unwrap(),
                ))
                .unwrap();
                entity.apply_components_from_item_stack(&stack).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    super::super::components::load(
                        Some(&parse_compound(f[4]).unwrap()),
                        registries
                    ),
                    "{row}"
                );
                assert_eq!(scalar(&entity), f[5], "{row}");
                checked[1] += 1;
            }
            _ => panic!("unknown native case: {row}"),
        }
        assert_eq!(entity.has_server_ticker(None), Ok(false), "{row}");
    }
    assert_eq!(checked, [40, 4]);
}
