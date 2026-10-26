use super::*;
use crate::{block::entity, level::LevelContext};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::BlockPos;
use std::{collections::BTreeSet, io::Read};

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn prime(state: &mut BlockEntityState) {
    let BlockEntityState::Jigsaw(entity) = state else {
        unreachable!()
    };
    entity.name = Identifier::parse("test:old_name").unwrap();
    entity.target = Identifier::parse("test:old_target").unwrap();
    entity.pool = Identifier::parse("test:old_pool").unwrap();
    entity.joint = JointType::Aligned;
    entity.final_state = "old:state".into();
    entity.placement_priority = 7;
    entity.selection_priority = 9;
}
fn scalar(state: &BlockEntityState) -> String {
    let BlockEntityState::Jigsaw(entity) = state else {
        unreachable!()
    };
    format!(
        "{},{},{},{},{},{},{}",
        entity.name,
        entity.target,
        entity.pool,
        entity.joint.as_str(),
        entity.final_state,
        entity.placement_priority,
        entity.selection_priority
    )
}
fn units(value: &NbtString) -> String {
    value
        .as_utf16()
        .iter()
        .map(|unit| format!("{unit:04x}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn native_jigsaw_factory_load_save_components_and_orientation_defaults_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/jigsaw-factory.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let context = LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut checked = [0; 3];
    let mut orientations = BTreeSet::new();
    for row in vectors.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        assert_eq!(state.block(), Block::JIGSAW);
        assert!(entity::supports(state.block()));
        assert!(entity::supports_creation(state.block()));
        let mut entity = entity::new_block_entity(POS, state).unwrap().unwrap();
        assert!(!entity.has_server_ticker(Some(true)).unwrap());
        match fields[0] {
            "jigsaw-data" => {
                if fields[2] != "@new" {
                    prime(&mut entity);
                    entity.load_with_components(&parse_compound(fields[2]).unwrap(), registries);
                }
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(fields[4]).unwrap(),
                    "update {row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    fields[5] == "true",
                    "packet {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    entity::components::load(Some(&parse_compound(fields[6]).unwrap()), registries),
                    "components {row}"
                );
                assert_eq!(scalar(&entity), fields[7], "fields {row}");
                let round = entity::load_static(
                    POS,
                    state,
                    &entity.save_with_full_metadata().unwrap(),
                    registries,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    round.save_with_full_metadata().unwrap(),
                    parse_compound(fields[8]).unwrap(),
                    "round trip {row}"
                );
                checked[0] += 1;
            }
            "jigsaw-components" => {
                prime(&mut entity);
                let stack = ItemStack::from_nbt(&Tag::Compound(parse_compound(fields[2]).unwrap()))
                    .unwrap();
                entity.apply_components_from_item_stack(&stack).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(fields[4]).unwrap(),
                    "update {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    entity::components::load(Some(&parse_compound(fields[5]).unwrap()), registries),
                    "components {row}"
                );
                assert_eq!(scalar(&entity), fields[6], "fields {row}");
                checked[1] += 1;
            }
            "jigsaw-utf16" => {
                let mut tag = CompoundTag::new();
                tag.insert(
                    "final_state",
                    Tag::String(NbtString::from_utf16(vec![0x61, 0xd800, 0x62, 0xdfff])),
                );
                entity.load_with_components(&tag, registries);
                let saved = entity.save_with_full_metadata().unwrap();
                assert_eq!(units(saved.get_string("final_state").unwrap()), fields[2]);
                let round = entity::load_static(POS, state, &saved, registries)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    units(
                        round
                            .get_update_tag()
                            .unwrap()
                            .get_string("final_state")
                            .unwrap()
                    ),
                    fields[3]
                );
                checked[2] += 1;
            }
            _ => panic!("native jigsaw vector kind"),
        }
        orientations.insert(state.id());
    }
    assert_eq!(checked, [144, 12, 12]);
    assert_eq!(orientations.len(), 12);
}
