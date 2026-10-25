use super::*;
use crate::block::entity::{BlockEntityDataView, create, load_static, new_block_entity, supports};
use ferraloom_data::{BlockState, Item};
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::Persistent;
use ferraloom_util::BlockPos;
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);

#[test]
fn native_chiseled_bookshelf_factory_load_save_and_components_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/chiseled-bookshelf.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = [0; 3];
    for row in vectors.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        assert!(supports(state.block()));
        let mut entity = new_block_entity(POS, state).unwrap().unwrap();
        match fields[0] {
            "shelf-default" => {
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[2]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    ChiseledBookShelfBlockEntity::view(&entity)
                        .unwrap()
                        .items
                        .len(),
                    fields[3].parse::<usize>().unwrap(),
                    "{row}"
                );
                assert_eq!(fields[4], "1");
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
                assert_eq!(
                    entity.collect_components(),
                    crate::block::entity::components::load(Some(
                        &parse_compound(fields[7]).unwrap()
                    )),
                    "{row}"
                );
                assert_eq!(
                    entity.has_server_ticker(None).unwrap(),
                    fields[8] == "true",
                    "{row}"
                );
                checked[0] += 1;
            }
            "shelf-load" => {
                ChiseledBookShelfBlockEntity::view_mut(&mut entity)
                    .unwrap()
                    .items[5] = ItemStack::new(Item::DIAMOND, 3, Default::default());
                entity.load_with_components(
                    &parse_compound(fields[2]).unwrap(),
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                );
                let saved = entity.save_with_full_metadata().unwrap();
                assert_eq!(saved, parse_compound(fields[3]).unwrap(), "{row}");
                assert_eq!(
                    ChiseledBookShelfBlockEntity::view(&entity)
                        .unwrap()
                        .last_interacted_slot(),
                    fields[4].parse::<i32>().unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    crate::block::entity::components::load(Some(
                        &parse_compound(fields[5]).unwrap()
                    )),
                    "{row}"
                );
                assert_eq!(
                    load_static(
                        POS,
                        state,
                        &saved,
                        crate::level::LevelContext::fixture(0, 63)
                            .registry_context()
                            .unwrap()
                    )
                    .unwrap()
                    .unwrap()
                    .save_with_full_metadata()
                    .unwrap(),
                    parse_compound(fields[6]).unwrap(),
                    "{row}"
                );
                checked[1] += 1;
            }
            "shelf-components" => {
                entity.load_with_components(
                    &parse_compound(
                        "{last_interacted_slot:3,Items:[{Slot:1b,id:'diamond',count:4}]}",
                    )
                    .unwrap(),
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                );
                entity
                    .apply_components_from_item_stack(
                        &ItemStack::from_nbt(&Tag::Compound(parse_compound(fields[2]).unwrap()))
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[3]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    crate::block::entity::components::load(Some(
                        &parse_compound(fields[4]).unwrap()
                    )),
                    "{row}"
                );
                assert_eq!(
                    entity.data().block_state.id(),
                    fields[5].parse::<u16>().unwrap(),
                    "loading must not update occupied properties: {row}"
                );
                let mut stripped = entity.save_custom_only().unwrap();
                entity.remove_components_from_tag(&mut stripped);
                assert_eq!(stripped, parse_compound(fields[6]).unwrap(), "{row}");
                checked[2] += 1;
            }
            _ => panic!("unknown native bookshelf vector: {row}"),
        }
    }
    assert_eq!(checked, [256, 80, 12]);
    let defaults = create(
        POS,
        ferraloom_data::Block::CHISELED_BOOKSHELF.default_state(),
    )
    .unwrap();
    assert_eq!(
        ChiseledBookShelfBlockEntity::view(&defaults)
            .unwrap()
            .last_interacted_slot(),
        -1
    );
}
