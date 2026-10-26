use super::LevelContext;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    level::{Level, LevelError, Region},
};
use ferraloom_data::{
    Block,
    datapack::{Pack, ResourceManager, TagSet, Tags},
};
use ferraloom_worldgen::{settings::DimensionType, structure::processor::BlockSet};
use std::{collections::BTreeMap, path::Path, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;

fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla resources");
    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()])
}

fn dimension(name: &str) -> DimensionType {
    DimensionType::decode(
        &resources()
            .require(
                &format!("minecraft:dimension_type/{name}.json")
                    .parse()
                    .unwrap(),
            )
            .unwrap()
            .json()
            .unwrap(),
    )
    .unwrap()
}

fn level(dimension: DimensionType) -> Level {
    let context = LevelContext::fixture(0, 63);
    Level::new(
        Region::new(&BEHAVIORS),
        Arc::new(context.as_ref().clone().with_dimension_type(dimension)),
    )
}

fn assert_native_membership(level: &Level, members: &str) {
    let members: Vec<_> = members.split(',').filter(|name| !name.is_empty()).collect();
    for &block in ferraloom_data::registries::block::ALL {
        assert_eq!(
            level.is_infiniburn(block.default_state()).unwrap(),
            members.contains(&block.name()),
            "{}",
            block.name(),
        );
    }
}

fn assert_dimension(name: &str) {
    let row = include_str!("../../tests/java/level/infiniburn.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .find(|row| row[1] == name)
        .unwrap();
    let mut level = level(dimension(name));
    let BlockSet::Tag(tag) = &level.context().dimension_type().unwrap().infiniburn else {
        panic!("native built-in infiniburn uses a named holder set");
    };
    assert_eq!(format!("\"#{tag}\""), row[2]);
    level.set_block_tags(Arc::new(TagSet::load(&resources()).unwrap()));
    assert_native_membership(&level, row[3]);
}

#[test]
fn overworld_infiniburn_matches_native_dimension_holder_set() {
    assert_dimension("overworld");
}

#[test]
fn nether_infiniburn_matches_native_dimension_holder_set() {
    assert_dimension("the_nether");
}

#[test]
fn end_infiniburn_matches_native_dimension_holder_set() {
    assert_dimension("the_end");
}

#[test]
fn direct_infiniburn_uses_the_decoded_holder_set_without_tags() {
    let resources = resources();
    let base = resources
        .require(&"minecraft:dimension_type/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    let mut count = 0;
    for row in include_str!("../../tests/java/level/infiniburn.tsv").lines() {
        let row: Vec<_> = row.split('\t').collect();
        if row[1] != "direct" {
            continue;
        }
        let mut encoded = base.clone();
        encoded["infiniburn"] = serde_json::from_str(row[2]).unwrap();
        let level = level(DimensionType::decode(&encoded).unwrap());
        assert_native_membership(&level, row[3]);
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn infiniburn_requires_dimension_and_tag_services_under_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level(dimension("the_end"));
        level.set_unported_behavior(policy);
        let state = Block::BEDROCK.default_state();
        assert_eq!(
            level.is_infiniburn(state),
            Err(LevelError::MissingBlockTags)
        );
        let tags = |values| {
            Arc::new(
                TagSet::from_tags(
                    Tags {
                        values,
                        diagnostics: Vec::new(),
                    },
                    ferraloom_data::registries::block::ALL.len(),
                )
                .unwrap(),
            )
        };
        level.set_block_tags(tags(BTreeMap::new()));
        assert_eq!(level.is_infiniburn(state), Err(LevelError::MissingBlockTag));
        level.set_block_tags(tags(BTreeMap::from([(
            "minecraft:infiniburn_end".parse().unwrap(),
            Vec::new(),
        )])));
        assert!(!level.is_infiniburn(state).unwrap());

        let mut context = level.context().as_ref().clone();
        context.dimension_type = None;
        let mut missing = Level::new(Region::new(&BEHAVIORS), Arc::new(context));
        missing.set_unported_behavior(policy);
        assert_eq!(
            missing.is_infiniburn(state),
            Err(LevelError::MissingDimensionType)
        );
    }
}
