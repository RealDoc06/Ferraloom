use crate::{block::entity, level::LevelContext};
use ferraloom_data::BlockState;
use ferraloom_nbt::parse_compound;
use ferraloom_util::BlockPos;

#[test]
fn native_container_and_beacon_lock_map_boundaries_match() {
    let context = LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut checked = 0;
    for row in include_str!("../../../tests/java/blocks/entities/lock-storage.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields[0], "lock-storage");
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let mut entity = entity::create(BlockPos::new(8, 64, 8), state).unwrap();
        entity.load_with_components(&parse_compound(fields[2]).unwrap(), registries);
        assert_eq!(
            entity.save_with_full_metadata().unwrap(),
            parse_compound(fields[3]).unwrap(),
            "save {row}"
        );
        assert_eq!(
            entity.collect_components(),
            entity::components::load(Some(&parse_compound(fields[4]).unwrap()), registries),
            "components {row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 100);
}
