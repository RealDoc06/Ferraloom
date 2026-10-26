use crate::block::entity::{create, load_static};
use ferraloom_data::Block;
use ferraloom_nbt::parse_compound;
use ferraloom_util::BlockPos;

#[test]
fn native_chest_and_furnace_typed_list_count_partials_preserve_other_slots_and_components() {
    let pos = BlockPos::new(8, 64, 8);
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/blocks/entities/container-storage.tsv").lines()
    {
        let fields: Vec<_> = row.split('\t').collect();
        let state = Block::from_name(fields[1]).unwrap().default_state();
        let mut receiver = create(pos, state).unwrap();
        receiver.load_with_components(
            &parse_compound(fields[2]).unwrap(),
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap(),
        );
        let saved = receiver.save_with_full_metadata().unwrap();
        assert_eq!(saved, parse_compound(fields[3]).unwrap(), "{row}");
        let again = load_static(
            pos,
            state,
            &saved,
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            again.save_with_full_metadata().unwrap(),
            parse_compound(fields[4]).unwrap(),
            "{row}",
        );
        checked += 1;
    }
    assert_eq!(checked, 16);
}
