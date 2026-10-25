use super::PathComputationType;
use crate::{
    Behaviors,
    block::LevelReaderRef,
    level::{Level, LevelContext, Region},
};
use ferraloom_data::{
    BlockState,
    datapack::{Identifier, Pack, ResourceManager, TagSet, Tags, load_tags},
    registries::fluid::Id,
};
use std::{collections::BTreeMap, io::Read, sync::Arc};

const TYPES: [PathComputationType; 3] = [
    PathComputationType::Land,
    PathComputationType::Water,
    PathComputationType::Air,
];
static EMPTY: Behaviors = Behaviors::EMPTY;

fn level(tags: TagSet<Id>) -> Level {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.set_fluid_tags(Arc::new(tags));
    level
}

fn check(level: &Level, state: BlockState, expected: u8) {
    let reader = LevelReaderRef::Server(level);
    for (bit, computation) in TYPES.into_iter().enumerate() {
        assert_eq!(
            reader.is_pathfindable(state, computation).unwrap(),
            expected & (1 << bit) != 0,
            "state {state:?}, computation {computation:?}"
        );
    }
}

#[test]
fn native_pathfindability_for_every_state_class_and_computation_type() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let tags = load_tags(&resources, "fluid", |id, _| Id::from_name(&id.to_string()));
    assert!(tags.diagnostics.is_empty());
    let level =
        level(TagSet::from_tags(tags, ferraloom_data::registries::fluid::ALL.len()).unwrap());
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/pathfinder/pathfindability.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[0].parse().unwrap()).unwrap();
        assert_eq!(state.block().class().java_name(), fields[2]);
        check(&level, state, fields[1].parse().unwrap());
        count += 1;
    }
    assert_eq!(count, 35_723);
}

#[test]
fn native_liquid_uses_source_fluid_when_tags_distinguish_flowing_states() {
    let tags = |source| {
        let (water, lava) = if source {
            (Id::WATER, Id::LAVA)
        } else {
            (Id::FLOWING_WATER, Id::FLOWING_LAVA)
        };
        TagSet::from_tags(
            Tags {
                values: BTreeMap::from([
                    (Identifier::new("minecraft", "water").unwrap(), vec![water]),
                    (Identifier::new("minecraft", "lava").unwrap(), vec![lava]),
                ]),
                diagnostics: Vec::new(),
            },
            ferraloom_data::registries::fluid::ALL.len(),
        )
        .unwrap()
    };
    let flowing = level(tags(false));
    let source = level(tags(true));
    let mut count = 0;
    for line in include_str!("../../../../tests/java/pathfinder/pathfindability-tags.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        check(
            if fields[0] == "source" {
                &source
            } else {
                &flowing
            },
            state,
            fields[2].parse().unwrap(),
        );
        count += 1;
    }
    assert_eq!(count, 64);
}
