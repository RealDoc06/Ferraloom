use super::*;
use std::io::Write;

fn pack(name: &str, files: &[(&str, &str)]) -> Pack {
    let mut pack = Pack::new(name);
    for (id, data) in files {
        pack.insert(id.parse().unwrap(), data.as_bytes());
    }
    pack
}

#[test]
fn pack_order_and_tag_merge_replace_optional() {
    let manager = ResourceManager::new(vec![
        pack(
            "base",
            &[
                ("worldgen/noise/a.json", "1"),
                (
                    "tags/block/a.json",
                    r##"{"values":["stone","#b","stone",{"id":"missing","required":false}]}"##,
                ),
                ("tags/block/b.json", r#"{"values":["dirt"]}"#),
                ("tags/block/replaced.json", r#"{"values":["missing"]}"#),
                ("tags/block/broken.json", r#"{"values":["missing"]}"#),
            ],
        ),
        pack(
            "overlay",
            &[
                ("worldgen/noise/a.json", "2"),
                ("tags/block/a.json", r#"{"values":["sand"]}"#),
                (
                    "tags/block/replaced.json",
                    r#"{"replace":true,"values":["stone"]}"#,
                ),
            ],
        ),
    ]);
    assert_eq!(
        manager
            .require(&"worldgen/noise/a.json".parse().unwrap())
            .unwrap()
            .json()
            .unwrap(),
        2
    );
    assert_eq!(manager.registry("worldgen/noise").len(), 1);
    let tags = load_tags(&manager, "block", |id, _| {
        ["stone", "dirt", "sand"]
            .contains(&id.path())
            .then(|| id.path().to_owned())
    });
    assert_eq!(
        tags.values[&"a".parse().unwrap()],
        ["stone", "dirt", "sand"]
    );
    assert_eq!(tags.values[&"replaced".parse().unwrap()], ["stone"]);
    assert!(!tags.values.contains_key(&"broken".parse().unwrap()));
    assert_eq!(tags.diagnostics.len(), 1);
}

#[test]
fn resolved_tags_preserve_order_and_test_membership_across_words() {
    use crate::{Block, registries::block, tags::BlockTags};
    let selected = [0, 63, 64, block::ALL.len() - 1].map(|i| block::ALL[i]);
    let mut pack = Pack::new("custom");
    pack.insert(
        "test:tags/block/custom.json".parse().unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "values": selected.map(Block::name),
        }))
        .unwrap(),
    );
    let tags = TagSet::<Block>::load(&ResourceManager::new(vec![pack])).unwrap();
    assert!(tags.diagnostics.is_empty());
    let tag = tags.resolve_name(&"test:custom".parse().unwrap()).unwrap();
    assert_eq!(tags.values(tag), selected);
    assert_eq!(tags.name(tag).to_string(), "test:custom");
    assert_eq!(tags.resolve(TagKey::new("test:custom")), Some(tag));
    for &block in block::ALL {
        assert_eq!(tags.contains(tag, block), selected.contains(&block));
    }
    assert!(tags.resolve(BlockTags::SUPPORTS_VEGETATION).is_none());
    assert!(!tags.contains_key(BlockTags::SUPPORTS_VEGETATION, Block::DIRT));
}

#[test]
fn generated_tag_keys_cover_vanilla_and_static_registries_resolve() {
    use crate::{Block, EntityType, Item, registries::fluid, tags::*};
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&path)).unwrap()]);
    let blocks = TagSet::<Block>::load(&resources).unwrap();
    let fluids = TagSet::<fluid::Id>::load(&resources).unwrap();
    let items = TagSet::<Item>::load(&resources).unwrap();
    let entities = TagSet::<EntityType>::load(&resources).unwrap();
    assert!(blocks.diagnostics.is_empty(), "{:?}", blocks.diagnostics);
    assert!(fluids.diagnostics.is_empty(), "{:?}", fluids.diagnostics);
    assert!(items.diagnostics.is_empty(), "{:?}", items.diagnostics);
    assert!(
        entities.diagnostics.is_empty(),
        "{:?}",
        entities.diagnostics
    );
    let water = fluids.resolve(FluidTags::WATER).unwrap();
    assert!(fluids.contains(water, fluid::Id::WATER));
    assert!(fluids.contains_key(FluidTags::WATER, fluid::Id::WATER));
    assert!(fluids.contains(water, fluid::Id::FLOWING_WATER));
    assert!(!fluids.contains(water, fluid::Id::LAVA));
    assert!(blocks.contains(blocks.resolve(BlockTags::DIRT).unwrap(), Block::DIRT));
    for (ordinal, &key) in BlockTags::ALL.iter().enumerate() {
        assert_eq!(key.ordinal(), Some(ordinal));
        let tag = blocks.resolve_name(&key.name().parse().unwrap());
        assert_eq!(blocks.resolve(key), tag);
        for &block in crate::registries::block::ALL {
            assert_eq!(
                blocks.contains_key(key, block),
                tag.is_some_and(|tag| blocks.contains(tag, block))
            );
        }
    }
    assert!(items.contains(items.resolve(ItemTags::LOGS).unwrap(), Item::OAK_LOG));
    assert!(entities.contains(
        entities.resolve(EntityTypeTags::ARTHROPOD).unwrap(),
        EntityType::SPIDER,
    ));
    for (registry, keys) in [
        (
            "block",
            BlockTags::ALL
                .iter()
                .map(|key| key.name())
                .collect::<Vec<_>>(),
        ),
        (
            "fluid",
            FluidTags::ALL.iter().map(|key| key.name()).collect(),
        ),
        ("item", ItemTags::ALL.iter().map(|key| key.name()).collect()),
        (
            "entity_type",
            EntityTypeTags::ALL.iter().map(|key| key.name()).collect(),
        ),
    ] {
        for name in resources.registry(&format!("tags/{registry}")).keys() {
            assert!(
                keys.contains(&name.to_string().as_str()),
                "missing key {registry}/{name}"
            );
        }
    }
}

#[test]
fn zip_and_directory_have_identical_resources() {
    let root = std::env::temp_dir().join(format!("ferraloom-pack-{}", std::process::id()));
    fs::create_dir_all(root.join("data/test/registry")).unwrap();
    fs::write(root.join("data/test/registry/a.json"), b"{}").unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "data/test/registry/a.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.write_all(b"{}").unwrap();
    writer
        .start_file(
            "assets/test/ignored.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.write_all(b"{}").unwrap();
    let zipped = Pack::from_zip("zip", writer.finish().unwrap()).unwrap();
    let directory = Pack::from_directory("dir", &root).unwrap();
    assert_eq!(zipped.resources, directory.resources);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn real_vanilla_pack_loads_from_bundle_or_extracted_jar() {
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let pack = Pack::vanilla_jar(Path::new(&path)).unwrap();
    let manager = ResourceManager::new(vec![pack]);
    assert!(
        manager
            .registry("worldgen/noise_settings")
            .contains_key(&"overworld".parse().unwrap())
    );
    let tags = load_tags(&manager, "block", |id, _| {
        crate::Block::from_name(&id.to_string())
    });
    assert!(tags.diagnostics.is_empty(), "{:?}", tags.diagnostics);
    assert!(!tags.values[&"minecraft:logs".parse().unwrap()].is_empty());
}

#[test]
fn vanilla_json_duplicate_keys_and_lenient_block_properties() {
    let manager = ResourceManager::new(vec![pack(
        "invalid",
        &[
            ("tag.json", r#"{"replace":false,"repl\u0061ce":true}"#),
            ("valid.json", r#"{"a":"{}:\"", "nested":{"a":1}}"#),
        ],
    )]);
    assert_eq!(
        manager
            .require(&"tag.json".parse().unwrap())
            .unwrap()
            .json()
            .unwrap()["replace"],
        true
    );
    assert!(
        manager
            .require(&"valid.json".parse().unwrap())
            .unwrap()
            .json()
            .is_ok()
    );
    let state=decode_block_state(&serde_json::json!({"id":"oak_stairs","properties":{"facing":"west","waterlogged":"true","half":"invalid","invented":"value"}})).unwrap();
    assert_eq!(
        state.get(crate::properties::FACING),
        Some(crate::properties::Facing::West)
    );
    assert_eq!(state.get(crate::properties::WATERLOGGED), Some(true));
    assert_eq!(
        state.get(crate::properties::HALF),
        crate::Block::OAK_STAIRS
            .default_state()
            .get(crate::properties::HALF)
    );
}

#[test]
fn identifiers_match_vanilla_empty_paths_namespace_guard_and_path_first_order() {
    for (input, namespace, path) in [
        ("", "minecraft", ""),
        (":stone", "minecraft", "stone"),
        ("test:", "test", ""),
        ("../path", "minecraft", "../path"),
    ] {
        let id: Identifier = input.parse().unwrap();
        assert_eq!((id.namespace(), id.path()), (namespace, path));
    }
    for invalid in ["..:stone", "Upper:stone", "minecraft:has space"] {
        assert!(invalid.parse::<Identifier>().is_err());
    }
    assert!("z:a".parse::<Identifier>().unwrap() < "a:z".parse().unwrap());
}
