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
