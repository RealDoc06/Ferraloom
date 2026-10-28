use super::*;
use ferraloom_data::datapack::Pack;
use ferraloom_nbt::{Compression, NbtIo};
use std::sync::atomic::{AtomicU64, Ordering};

struct World(PathBuf);
impl World {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "ferraloom-template-cache-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        Self(directory)
    }
    fn write(&self, name: &str, data: &[u8]) {
        let file = self
            .0
            .join("generated/fixture/structure")
            .join(format!("{name}.nbt"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, data).unwrap();
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn bytes(size: i32) -> Vec<u8> {
    let tag = ferraloom_nbt::parse_compound(&format!(
        "{{DataVersion:5023,size:[{size},1,1],palette:[],blocks:[],entities:[]}}"
    ))
    .unwrap();
    let mut bytes = Vec::new();
    NbtIo::write_compound(&mut bytes, Compression::Gzip, &tag).unwrap();
    bytes
}
fn resources(size: i32) -> ResourceManager {
    let mut pack = Pack::new("template-source-fixture");
    for (name, size) in [("shared", 1), ("fallback", 8), ("resource", size)] {
        pack.insert(
            format!("fixture:structure/{name}.nbt").parse().unwrap(),
            bytes(size),
        );
    }
    ResourceManager::new(vec![pack])
}
#[test]
fn generated_sources_cache_hits_misses_and_listing_match_native() {
    let world = World::new();
    world.write("shared", &bytes(2));
    let mut manager = StructureTemplateManager::load_all(&resources(7))
        .unwrap()
        .with_world_source(&world.0);
    let id = |name: &str| format!("fixture:{name}").parse::<Identifier>().unwrap();
    let mut rows = Vec::new();
    let mut record = |key: &str, value: i32| rows.push(format!("{key}\t{value}"));
    let first = manager.get(&id("shared")).unwrap();
    record("world_wins", first.size[0]);
    world.write("shared", &bytes(3));
    let cached = manager.get(&id("shared")).unwrap();
    record("cached_hit", cached.size[0]);
    record("cached_identity", i32::from(Arc::ptr_eq(&first, &cached)));
    manager.remove(&id("shared"));
    record("after_remove", manager.get(&id("shared")).unwrap().size[0]);
    record(
        "missing",
        if manager.get(&id("future")).is_err() {
            -1
        } else {
            0
        },
    );
    world.write("future", &bytes(4));
    record(
        "cached_miss",
        if manager.get(&id("future")).is_err() {
            -1
        } else {
            0
        },
    );
    record(
        "listed_despite_miss",
        i32::from(manager.list_templates().contains(&id("future"))),
    );
    manager.remove(&id("future"));
    record(
        "miss_after_remove",
        manager.get(&id("future")).unwrap().size[0],
    );
    let created = manager.get_or_create(&id("created"));
    record("created_empty", created.size[0]);
    record(
        "created_identity",
        i32::from(Arc::ptr_eq(
            &created,
            &manager.get_or_create(&id("created")),
        )),
    );
    record(
        "created_not_listed",
        i32::from(!manager.list_templates().contains(&id("created"))),
    );
    world.write("created", &bytes(5));
    record(
        "created_cached",
        manager.get(&id("created")).unwrap().size[0],
    );
    manager.remove(&id("created"));
    record(
        "created_after_remove",
        manager.get(&id("created")).unwrap().size[0],
    );
    world.write("fallback", b"invalid gzip");
    record(
        "malformed_world_falls_through",
        manager.get(&id("fallback")).unwrap().size[0],
    );
    record("resource", manager.get(&id("resource")).unwrap().size[0]);
    assert!(manager.get(&id("reload_future")).is_err());
    world.write("reload_future", &bytes(6));
    manager.on_resource_manager_reload(resources(10));
    record(
        "resource_after_reload",
        manager.get(&id("resource")).unwrap().size[0],
    );
    record(
        "world_after_reload",
        manager.get(&id("shared")).unwrap().size[0],
    );
    record(
        "miss_after_reload",
        manager.get(&id("reload_future")).unwrap().size[0],
    );
    let listing = manager.list_templates();
    let unique = listing.iter().collect::<BTreeSet<_>>();
    record("listing_distinct", i32::from(listing.len() == unique.len()));
    let world_count = listing
        .iter()
        .take_while(|name| **name != id("resource"))
        .count();
    record("generated_before_resources", i32::from(world_count == 5));
    assert_eq!(
        rows.join("\n").trim(),
        include_str!("../../testdata/template_manager.tsv").trim()
    );
}

#[test]
fn concurrent_asset_lookups_share_identity_and_allow_reload_of_held_arcs() {
    let world = World::new();
    world.write("shared", &bytes(2));
    let manager = Arc::new(
        StructureTemplateManager::load_all(&resources(7))
            .unwrap()
            .with_world_source(&world.0),
    );
    let id: Identifier = "fixture:shared".parse().unwrap();
    let readers = (0..8)
        .map(|_| {
            let manager = manager.clone();
            let id = id.clone();
            std::thread::spawn(move || manager.get(&id).unwrap())
        })
        .collect::<Vec<_>>();
    let values = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .collect::<Vec<_>>();
    assert!(values.iter().all(|value| Arc::ptr_eq(value, &values[0])));
    let mut manager = Arc::try_unwrap(manager).unwrap();
    manager.on_resource_manager_reload(resources(11));
    assert_eq!(values[0].size[0], 2);
    assert_eq!(
        manager
            .get(&"fixture:resource".parse().unwrap())
            .unwrap()
            .size[0],
        11
    );
}
