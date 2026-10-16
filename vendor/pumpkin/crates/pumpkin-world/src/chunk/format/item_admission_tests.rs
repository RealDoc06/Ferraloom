//! Source-authored preservation regressions, not paired Java acceptance.

use super::*;
use crate::chunk::io::{ChunkSerializer, FileIO, LoadedData, file_manager::ChunkFileManager};
use pumpkin_config::chunk::AnvilChunkConfig;
use pumpkin_data::{data_component::DataComponent, item_stack::ItemStack};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

const POSITION: BlockPos = BlockPos::new(1, 64, 2);
const CONTAINERS: [&str; 3] = [
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:barrel",
];

fn saved_item(id: &str, component: &str, value: NbtTag) -> NbtCompound {
    let mut components = NbtCompound::new();
    components.put(component, value);
    let mut item = NbtCompound::new();
    item.put_string("id", id.into());
    item.put_int("count", 1);
    item.put_compound("components", components);
    item
}

fn sing_horn() -> NbtCompound {
    saved_item(
        "minecraft:goat_horn",
        "minecraft:instrument",
        NbtTag::String("minecraft:sing_goat_horn".into()),
    )
}

fn saved_container(id: &str, mut item: NbtCompound) -> NbtCompound {
    item.put_byte("Slot", 6);
    let mut container = NbtCompound::new();
    container.put_string("id", id.into());
    container.put_int("x", POSITION.0.x);
    container.put_int("y", POSITION.0.y);
    container.put_int("z", POSITION.0.z);
    container.put_list("Items", vec![NbtTag::Compound(item)]);
    container
}

fn saved_chunk(container: NbtCompound) -> Arc<ChunkData> {
    let chunk = ChunkData::empty_sync(0, 0);
    chunk.set_block_absolute_y(1, 64, 2, Block::CHEST.default_state.id);
    chunk
        .pending_block_entities
        .lock()
        .unwrap()
        .insert(POSITION, container);
    chunk
}

fn assert_refused(error: &ChunkReadingError, id: &str) {
    assert!(!error.to_string().contains("private-"));
    assert!(matches!(error, ChunkReadingError::ParsingError(
        ChunkParsingError::UnsupportedContainerItem {
            block_entity, position, slot: Some(6), ..
        }
    ) if *block_entity == id && *position == POSITION));
}

#[derive(Deserialize)]
struct Prototype {
    component: String,
    item: String,
    value: serde_json::Value,
}

// These are exact JSON component payloads from official 26.3 prototype reports,
// with per-report provenance in the fixture. JSON does not carry NBT widths;
// booleans become bytes, integers ints and fractional fields floats here. This
// tests refusal of real component shapes, not an observed Java saved-chest run.
fn prototype_nbt(value: serde_json::Value) -> NbtTag {
    match value {
        serde_json::Value::Bool(value) => NbtTag::Byte(i8::from(value)),
        serde_json::Value::Number(value) => value.as_i64().map_or_else(
            || NbtTag::Float(value.as_f64().unwrap() as f32),
            |value| NbtTag::Int(i32::try_from(value).unwrap()),
        ),
        serde_json::Value::String(value) => NbtTag::String(value.into()),
        serde_json::Value::Array(values) => {
            NbtTag::List(values.into_iter().map(prototype_nbt).collect())
        }
        serde_json::Value::Object(values) => {
            let mut compound = NbtCompound::new();
            for (key, value) in values {
                compound.put(&key, prototype_nbt(value));
            }
            NbtTag::Compound(compound)
        }
        serde_json::Value::Null => panic!("component prototype must have an NBT value"),
    }
}

#[test]
fn saved_containers_refuse_all_23_placeholder_component_inputs() {
    let prototypes: Vec<Prototype> =
        serde_json::from_str(include_str!("item_component_prototypes_26_3.json")).unwrap();
    assert_eq!(prototypes.len(), 19);
    let mut inputs: Vec<_> = prototypes
        .into_iter()
        .map(|prototype| {
            (
                prototype.component,
                prototype.item,
                prototype_nbt(prototype.value),
            )
        })
        .collect();
    // Cached 26.3 DataComponents lambda$static$120 uses Unit.CODEC for waxed.
    // AdditionalTradeCost and CreativeSlotLock have network codecs only, so
    // saved inputs are adversarial. SulfurCubeContent has a persistent codec,
    // but no cached payload proof: its marker is an unsupported extension test.
    let mut sulfur = NbtCompound::new();
    sulfur.put_string("unverified_shape", "private-sulfur-payload".into());
    for (name, value) in [
        ("waxed", NbtTag::Compound(NbtCompound::new())),
        ("additional_trade_cost", NbtTag::Int(7)),
        ("creative_slot_lock", NbtTag::Compound(NbtCompound::new())),
        ("sulfur_cube_content", NbtTag::Compound(sulfur)),
    ] {
        inputs.push((format!("minecraft:{name}"), "minecraft:stone".into(), value));
    }
    assert_eq!(inputs.len(), 23);
    for (name, id, value) in inputs {
        let item = saved_item(&id, &name, value);
        let component = DataComponent::try_from_name(&name).unwrap();
        let stack = ItemStack::read_item_stack(&item).unwrap();
        let mut reduced = NbtCompound::new();
        stack.write_item_stack(&mut reduced);
        if component.is_persistent() {
            assert_eq!(
                reduced.get_compound("components").unwrap().get(&name),
                Some(&NbtTag::End),
                "{name}"
            );
        } else {
            assert!(reduced.get("components").is_none(), "{name}");
        }
        assert_eq!(
            validate_item_stack(&item),
            Err(ItemStackAdmissionError::Lossy)
        );
        for id in CONTAINERS {
            let container = saved_container(id, item.clone());
            let source = saved_chunk(container.clone());
            // Encode the original, valid tree, never the reduced ItemStack.
            let bytes = source.to_bytes().unwrap();
            let Err(error) = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)) else {
                panic!("saved placeholder must refuse the chunk");
            };
            assert_refused(&error, id);
            assert_eq!(
                source.pending_block_entities.lock().unwrap()[&POSITION],
                container
            );
            let mut deferred = container;
            deferred.put_string("LootTable", "minecraft:chests/simple_dungeon".into());
            let source = saved_chunk(deferred.clone());
            let admitted =
                ChunkData::from_bytes(&source.to_bytes().unwrap(), Vector2::new(0, 0)).unwrap();
            assert_eq!(
                admitted.pending_block_entities.lock().unwrap()[&POSITION],
                deferred
            );
        }
    }
}

#[test]
fn saved_container_guard_checks_nested_items_but_keeps_opaque_data_and_removals() {
    for name in [
        "minecraft:container",
        "minecraft:bundle_contents",
        "minecraft:charged_projectiles",
    ] {
        let nested = if name == "minecraft:container" {
            let mut entry = NbtCompound::new();
            entry.put_int("slot", 0);
            entry.put_compound("item", sing_horn());
            entry
        } else {
            sing_horn()
        };
        let item = saved_item(
            "minecraft:crossbow",
            name,
            NbtTag::List(vec![NbtTag::Compound(nested)]),
        );
        for id in CONTAINERS {
            assert!(
                validate_saved_container_items(&saved_container(id, item.clone()), POSITION)
                    .is_err()
            );
        }
    }
    // Authored custom_data is not a nested item boundary. In particular, its
    // count/components fields must not be normalized by the admission check.
    let mut opaque = sing_horn();
    opaque.put_int("count", 0);
    opaque.put_string("private-field", "private-value".into());
    let mut plain = NbtCompound::new();
    plain.put_string("id", "minecraft:goat_horn".into());
    for item in [
        plain,
        saved_item(
            "minecraft:stone",
            "minecraft:custom_data",
            NbtTag::Compound(opaque),
        ),
        saved_item(
            "minecraft:goat_horn",
            "!minecraft:instrument",
            NbtTag::Compound(NbtCompound::new()),
        ),
    ] {
        assert_eq!(validate_item_stack(&item), Ok(()));
        for id in CONTAINERS {
            let container = saved_container(id, item.clone());
            let source = saved_chunk(container.clone());
            let admitted =
                ChunkData::from_bytes(&source.to_bytes().unwrap(), Vector2::new(0, 0)).unwrap();
            assert_eq!(
                admitted.pending_block_entities.lock().unwrap()[&POSITION],
                container
            );
        }
    }
}

async fn fetch(
    reader: &ChunkFileManager<anvil::AnvilChunkFile<ChunkData>>,
    folder: &LevelFolder,
    position: Vector2<i32>,
) -> LoadedData<Arc<ChunkData>, ChunkReadingError> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (send, mut receive) = tokio::sync::mpsc::channel(1);
        reader.fetch_chunks(folder, &[position], send).await;
        receive.recv().await.expect("chunk read must reply")
    })
    .await
    .expect("chunk IO timed out")
}

#[tokio::test]
async fn refused_horn_and_unknown_chests_keep_mca_bytes_while_healthy_regions_write() {
    for item in [
        sing_horn(),
        saved_item(
            "minecraft:stone",
            "unknown:private-component",
            NbtTag::String("private-value".into()),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let folder = LevelFolder {
            root_folder: root.clone(),
            dim_folder: root.clone(),
            region_folder: root.join("region"),
            entities_folder: root.join("entities"),
            poi_folder: root.join("poi"),
        };
        std::fs::create_dir_all(&folder.region_folder).unwrap();
        let container = saved_container("minecraft:chest", item);
        let source = saved_chunk(container.clone());
        let path = folder.region_folder.join("r.0.0.mca");
        let mut region = anvil::AnvilChunkFile::<ChunkData>::default();
        region
            .update_chunk(source.clone(), &AnvilChunkConfig::default())
            .await
            .unwrap();
        region.write(&path).await.unwrap();
        drop(region);
        let before = std::fs::read(&path).unwrap();
        let reader =
            ChunkFileManager::<anvil::AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
        let LoadedData::Error((position, error)) =
            fetch(&reader, &folder, Vector2::new(0, 0)).await
        else {
            panic!("an unsupported saved chest must never be published as loaded or missing");
        };
        assert_eq!(position, Vector2::new(0, 0));
        assert_refused(&error, "minecraft:chest");

        let healthy = ChunkData::empty_sync(32, 0);
        healthy.set_block_absolute_y(3, 64, 4, Block::DIAMOND_BLOCK.default_state.id);
        healthy.mark_dirty(true);
        tokio::time::timeout(
            Duration::from_secs(10),
            reader.save_chunks(&folder, vec![(Vector2::new(32, 0), healthy)]),
        )
        .await
        .unwrap()
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), reader.flush())
            .await
            .unwrap()
            .unwrap();
        drop(reader);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            source.pending_block_entities.lock().unwrap()[&POSITION],
            container
        );

        let fresh =
            ChunkFileManager::<anvil::AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
        let LoadedData::Loaded(healthy) = fetch(&fresh, &folder, Vector2::new(32, 0)).await else {
            panic!("independent healthy region must remain readable");
        };
        assert_eq!(
            healthy.section.get_block_absolute_y(3, 64, 4),
            Some(Block::DIAMOND_BLOCK.default_state.id)
        );
        let LoadedData::Error((_, error)) = fetch(&fresh, &folder, Vector2::new(0, 0)).await else {
            panic!("fresh reader must still refuse the original chest");
        };
        assert_refused(&error, "minecraft:chest");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
