use super::*;
use crate::structure::processor::tests::generator;
use serde_json::Value;
use std::io::Read;
fn golden(bytes: &[u8]) -> Value {
    let mut s = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}
#[test]
fn jigsaw_structure_compounds_round_trip_through_storage_chunk_model() {
    let (resources, generator) = generator();
    let registry = StructureRegistry::load(&resources, &generator.data).unwrap();
    let pools = crate::structure::pool::StructurePools::load(&resources, &mut |_| {
        Err(Error("no inline feature".into()))
    })
    .unwrap();
    let templates = StructureTemplateManager::load(&resources, pools.templates()).unwrap();
    let reference_cases = golden(include_bytes!("testdata/reference_sets.json.gz"));
    let refs = reference_cases.as_array().unwrap().last().unwrap();
    let input_refs = refs["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect::<Vec<_>>();
    let expected_refs = refs["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect::<Vec<_>>();
    for case in golden(include_bytes!("testdata/jigsaw.json.gz"))
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.get("start_nbt").is_some())
    {
        let id = case["structure"].as_str().unwrap();
        let start = ferraloom_nbt::parse_compound(case["start_nbt"].as_str().unwrap()).unwrap();
        let starts: CompoundTag = [(id, Tag::Compound(start))].into_iter().collect();
        let references: CompoundTag = [
            (id, Tag::LongArray(input_refs.clone())),
            ("unknown:discarded", Tag::LongArray(input_refs.clone())),
            ("minecraft:stronghold", Tag::LongArray(Vec::new())),
        ]
        .into_iter()
        .collect();
        let data: CompoundTag = [
            ("starts", Tag::Compound(starts)),
            ("References", Tag::Compound(references)),
        ]
        .into_iter()
        .collect();
        let chunk = ChunkStructures::from_nbt(
            [0, 0],
            &data,
            &registry,
            &templates,
            &resources,
            &mut |_| Err(Error("no inline feature".into())),
        )
        .unwrap();
        let packed = chunk.to_nbt().unwrap();
        let mut expected =
            ferraloom_nbt::parse_compound(case["loaded_nbt"].as_str().unwrap()).unwrap();
        expected.insert("ChunkX", Tag::Int(0));
        expected.insert("ChunkZ", Tag::Int(0));
        assert_eq!(
            packed
                .get("starts")
                .unwrap()
                .as_compound()
                .unwrap()
                .get(id)
                .unwrap(),
            &Tag::Compound(expected)
        );
        assert_eq!(
            packed
                .get("References")
                .unwrap()
                .as_compound()
                .unwrap()
                .get(id)
                .unwrap(),
            &Tag::LongArray(expected_refs.clone())
        );
        assert_eq!(
            packed
                .get("References")
                .unwrap()
                .as_compound()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(chunk.known_valid_starts()[&id.parse().unwrap()], 3);
        let storage = ferraloom_storage::schema::StructureData::from_nbt(packed.clone()).unwrap();
        assert_eq!(storage.to_nbt(), packed);
        let root: CompoundTag = [
            ("DataVersion", Tag::Int(5023)),
            (
                "Status",
                Tag::String("minecraft:structure_references".into()),
            ),
            ("xPos", Tag::Int(0)),
            ("zPos", Tag::Int(0)),
            ("structures", Tag::Compound(packed)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            ferraloom_storage::schema::SerializableChunkData::parse(root.clone())
                .unwrap()
                .write()
                .unwrap(),
            root
        );
    }
}
