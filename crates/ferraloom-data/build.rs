//! Turns pinned vanilla reports and extracted facts into allocation-free tables.
#[path = "build/block_drop_tables.rs"]
mod block_drop_tables;
#[path = "build/block_methods.rs"]
mod block_methods;
#[path = "build/building_properties.rs"]
mod building_properties;
#[path = "build/center_support.rs"]
mod center_support;
#[path = "build/command_block_properties.rs"]
mod command_block_properties;
#[path = "build/copper_chest_properties.rs"]
mod copper_chest_properties;
#[path = "build/environment_attributes.rs"]
mod environment_attributes;
#[path = "build/feature_requirements.rs"]
mod feature_requirements;
#[path = "build/game_events.rs"]
mod game_events;
#[path = "build/item_methods.rs"]
mod item_methods;

#[path = "build/dispenser_properties.rs"]
mod dispenser_properties;
#[path = "build/note_properties.rs"]
mod note_properties;
#[path = "build/redstone_properties.rs"]
mod redstone_properties;

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

fn load(root: &Path, name: &str) -> Value {
    let path = root.join(format!("{name}.json"));
    println!("cargo:rerun-if-changed={}", path.display());
    serde_json::from_slice(&std::fs::read(path).expect("committed generated data")).unwrap()
}

fn string(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn number(v: &Value) -> u64 {
    v.as_u64().unwrap()
}

fn ident(name: &str) -> String {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let mut out: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "value_");
    }
    if ["type", "match", "ref", "self", "mod", "move", "loop", "use"].contains(&out.as_str()) {
        out.push('_');
    }
    out
}

fn pascal(name: &str) -> String {
    let value: String = ident(name)
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| format!("{}{}", s[..1].to_ascii_uppercase(), &s[1..]))
        .collect();
    if value == "Self" {
        "SelfValue".into()
    } else {
        value
    }
}

fn constant(name: &str) -> String {
    ident(name).to_ascii_uppercase()
}

fn float(v: &Value) -> String {
    let n = v.as_f64().unwrap();
    assert!(n.is_finite());
    format!("{n:?}")
}

fn ordered_entries(v: &Value) -> Vec<(&str, u64)> {
    let mut entries: Vec<_> = v
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, entry)| (name.as_str(), number(&entry["protocol_id"])))
        .collect();
    entries.sort_by_key(|(_, id)| *id);
    for (i, (_, id)) in entries.iter().enumerate() {
        assert_eq!(*id as usize, i, "non-contiguous protocol IDs");
    }
    entries
}

fn registries(out: &mut String, reports: &Value) {
    writeln!(out, "pub mod registries {{").unwrap();
    for (registry, report) in reports.as_object().unwrap() {
        let entries = ordered_entries(&report["entries"]);
        assert!(entries.len() <= u16::MAX as usize);
        writeln!(out, "pub mod {} {{", ident(registry)).unwrap();
        writeln!(
            out,
            "pub const REGISTRY_PROTOCOL_ID: u16 = {};",
            report["protocol_id"]
        )
        .unwrap();
        writeln!(out, "#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)] pub struct Id(pub(crate) u16);").unwrap();
        writeln!(out, "impl Id {{").unwrap();
        for (name, id) in &entries {
            writeln!(out, "pub const {}: Self = Self({id});", constant(name)).unwrap();
        }
        out.push_str("pub const fn protocol_id(self) -> u16 { self.0 }\n");
        out.push_str("pub const fn from_protocol_id(id: u16) -> Option<Self> { if (id as usize) < NAMES.len() { Some(Self(id)) } else { None } }\n");
        out.push_str("pub const fn name(self) -> &'static str { NAMES[self.0 as usize] }\n");
        out.push_str("/// Cold-path identifier parsing; hot paths use protocol IDs.\npub fn from_name(name: &str) -> Option<Self> { NAMES.iter().position(|&n| n == name).map(|i| Self(i as u16)) }\n}\n");
        writeln!(
            out,
            "pub const NAMES: &[&str] = &{:?};",
            entries.iter().map(|(name, _)| *name).collect::<Vec<_>>()
        )
        .unwrap();
        out.push_str("pub const ALL: &[Id] = &[");
        for (_, id) in &entries {
            write!(out, "Id({id}),").unwrap();
        }
        out.push_str("];\npub const fn name(id: Id) -> &'static str { id.name() }\npub fn from_name(name: &str) -> Option<Id> { Id::from_name(name) }\n}\n");
    }
    out.push_str("}\npub use registries::{block::Id as Block, item::Id as Item, entity_type::Id as EntityType};\n");
    out.push_str("/// Static registry entries in numeric protocol order.\npub fn registry_entries(name: &str) -> Option<&'static [&'static str]> { match name {\n");
    for registry in reports.as_object().unwrap().keys() {
        writeln!(
            out,
            "{registry:?} => Some(registries::{}::NAMES),",
            ident(registry)
        )
        .unwrap();
    }
    out.push_str("_ => None, } }\n");
    out.push_str("pub const REGISTRIES: &[RegistryInfo] = &[\n");
    for (name, report) in reports.as_object().unwrap() {
        writeln!(
            out,
            "RegistryInfo {{ name: {name:?}, protocol_id: {}, count: {} }},",
            report["protocol_id"],
            report["entries"].as_object().unwrap().len()
        )
        .unwrap();
    }
    out.push_str("];\n");
}

fn packets(out: &mut String, reports: &Value) {
    out.push_str("pub mod packets {\n");
    for (state, directions) in reports.as_object().unwrap() {
        writeln!(out, "pub mod {} {{", ident(state)).unwrap();
        for (direction, packets) in directions.as_object().unwrap() {
            writeln!(out, "pub mod {} {{", ident(direction)).unwrap();
            let entries = ordered_entries(packets);
            out.push_str("#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]\n#[repr(u16)]\npub enum Packet {\n");
            for (name, id) in &entries {
                writeln!(out, "{} = {id},", pascal(name)).unwrap();
            }
            out.push_str("}\nimpl Packet { pub const fn protocol_id(self) -> u16 { self as u16 }\npub const fn from_protocol_id(id: u16) -> Option<Self> { match id {\n");
            for (name, id) in &entries {
                writeln!(out, "{id} => Some(Self::{}),", pascal(name)).unwrap();
            }
            out.push_str(
                "_ => None, } }\npub const fn name(self) -> &'static str { match self {\n",
            );
            for (name, _) in &entries {
                writeln!(out, "Self::{} => {name:?},", pascal(name)).unwrap();
            }
            out.push_str("} } }\npub const ALL: &[Packet] = &[\n");
            for (name, _) in &entries {
                writeln!(out, "Packet::{},", pascal(name)).unwrap();
            }
            out.push_str("];\n}\n");
        }
        out.push_str("}\n");
    }
    out.push_str("}\n");
}

fn sound_events(out: &mut String, entries: &Value, report: &Value) {
    let entries = entries.as_array().expect("sound event facts");
    let registry = ordered_entries(&report["entries"]);
    assert_eq!(entries.len(), registry.len());
    writeln!(
        out,
        "static SOUND_FIXED_RANGES: [Option<f32>; {}] = [",
        entries.len()
    )
    .unwrap();
    for (entry, (name, id)) in entries.iter().zip(registry) {
        assert_eq!(entry["id"].as_u64(), Some(id));
        assert_eq!(entry["name"].as_str(), Some(name));
        if entry["fixed_range"].is_null() {
            out.push_str("None,");
        } else {
            write!(out, "Some({}),", float(&entry["fixed_range"])).unwrap();
        }
    }
    out.push_str("];\n");
}

struct Property {
    id: usize,
    values: Vec<String>,
    rust_type: String,
}

impl Property {
    fn index(&self, value: &str) -> u8 {
        match self.rust_type.as_str() {
            "bool" => u8::from(value == "true"),
            "u8" => value.parse().unwrap(),
            _ => self
                .values
                .iter()
                .position(|v| v == value)
                .unwrap()
                .try_into()
                .unwrap(),
        }
    }
}

fn properties(out: &mut String, reports: &Value) -> BTreeMap<String, Property> {
    let mut values: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for report in reports.as_object().unwrap().values() {
        if let Some(props) = report["properties"].as_object() {
            for (name, allowed) in props {
                values.entry(name.clone()).or_default().extend(
                    allowed
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| string(v).to_owned()),
                );
            }
        }
    }
    assert!(values.len() <= u8::MAX as usize);
    out.push_str("pub mod properties {\nuse super::{Property, PropertyValue, PhantomData};\n");
    let mut result = BTreeMap::new();
    for (id, (name, allowed)) in values.into_iter().enumerate() {
        let allowed: Vec<_> = allowed.into_iter().collect();
        let ty = if allowed.iter().all(|s| s == "true" || s == "false") {
            "bool".to_owned()
        } else if allowed.iter().all(|s| s.parse::<u8>().is_ok()) {
            "u8".to_owned()
        } else {
            pascal(&name)
        };
        if ty != "bool" && ty != "u8" {
            out.push_str("#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n#[repr(u8)]\n");
            writeln!(out, "pub enum {ty} {{").unwrap();
            for (i, value) in allowed.iter().enumerate() {
                writeln!(out, "{} = {i},", pascal(value)).unwrap();
            }
            writeln!(out, "}}\nimpl PropertyValue for {ty} {{ fn index(self) -> u8 {{ self as u8 }} fn from_index(index: u8) -> Option<Self> {{ match index {{").unwrap();
            for (i, value) in allowed.iter().enumerate() {
                writeln!(out, "{i} => Some(Self::{}),", pascal(value)).unwrap();
            }
            out.push_str("_ => None, } } }\n");
            writeln!(
                out,
                "impl {ty} {{ pub const fn as_str(self) -> &'static str {{ match self {{"
            )
            .unwrap();
            for value in &allowed {
                writeln!(out, "Self::{} => {value:?},", pascal(value)).unwrap();
            }
            out.push_str("} } }\n");
        }
        writeln!(
            out,
            "pub const {}: Property<{ty}> = Property {{ id: {id}, marker: PhantomData }};",
            constant(&name)
        )
        .unwrap();
        result.insert(
            name,
            Property {
                id,
                values: allowed,
                rust_type: ty,
            },
        );
    }
    out.push_str("}\n");
    result
}

fn blocks(
    out: &mut String,
    reports: &Value,
    extracted: &Value,
    props: &BTreeMap<String, Property>,
    render_shapes: &Value,
) {
    let render_shapes = render_shapes.as_array().expect("per-state render shapes");
    let blocks = extracted["blocks"].as_array().unwrap();
    let states = extracted["states"].as_array().unwrap();
    assert_eq!(states.len(), render_shapes.len());
    assert!(states.len() <= u16::MAX as usize + 1);
    assert_eq!(blocks.len(), reports.as_object().unwrap().len());
    out.push_str("pub const BLOCKS: &[BlockData] = &[\n");
    for (id, block) in blocks.iter().enumerate() {
        assert_eq!(id as u64, number(&block["id"]));
        let report = &reports[string(&block["name"])];
        let states = report["states"].as_array().unwrap();
        let first = number(&states[0]["id"]);
        let default = states.iter().find(|s| s["default"] == true).unwrap();
        assert_eq!(default["id"], block["default_state"]);
        write!(
            out,
            "BlockData {{ default_state: BlockState({}), first_state: {first}, state_count: {},",
            block["default_state"],
            states.len()
        )
        .unwrap();
        for field in [
            "explosion_resistance",
            "friction",
            "speed_factor",
            "jump_factor",
            "fall_distance_reduction",
            "bounce_restitution",
        ] {
            write!(out, "{field}: {},", float(&block[field])).unwrap();
        }
        out.push_str("properties: &[\n");
        let mut layouts = Vec::new();
        let mut stride = 1;
        if let Some(properties) = report["properties"].as_object() {
            // StateDefinition sorts keys; the last property changes fastest.
            for (name, values) in properties.iter().rev() {
                let property = &props[name];
                let values = values.as_array().unwrap();
                let indexes: Vec<_> = values.iter().map(|v| property.index(string(v))).collect();
                let integer_values = if property.rust_type == "u8" {
                    format!(
                        "Some(&{:?})",
                        values
                            .iter()
                            .map(|v| string(v).parse::<i32>().unwrap())
                            .collect::<Vec<_>>()
                    )
                } else {
                    "None".to_owned()
                };
                writeln!(
                    out,
                    "PropertyLayout {{ id: {}, name: {name:?}, names: &{:?}, stride: {stride}, values: &{indexes:?}, integer_values: {integer_values} }},",
                    property.id, values.iter().map(string).collect::<Vec<_>>()
                )
                .unwrap();
                layouts.push((name, values, stride));
                stride *= values.len();
            }
        }
        assert_eq!(stride, states.len());
        // Fail generation if any report stops following the ported arithmetic.
        for (offset, state) in states.iter().enumerate() {
            assert_eq!(number(&state["id"]), first + offset as u64);
            for (name, values, stride) in &layouts {
                assert_eq!(
                    state["properties"][*name],
                    values[offset / stride % values.len()]
                );
            }
        }
        out.push_str("] },\n");
    }
    out.push_str("];\n");
    let mut sounds = BTreeMap::new();
    for state in states {
        let key = serde_json::to_string(&state["sound"]).unwrap();
        let next = sounds.len();
        sounds.entry(key).or_insert(next);
    }
    let mut ordered_sounds: Vec<_> = sounds.iter().collect();
    ordered_sounds.sort_by_key(|(_, id)| **id);
    out.push_str("pub const SOUND_TYPES: &[SoundType] = &[\n");
    for (json, _) in ordered_sounds {
        let sound: Value = serde_json::from_str(json).unwrap();
        write!(
            out,
            "SoundType {{ name: {:?}, volume: {}, pitch: {},",
            string(&sound["name"]),
            float(&sound["volume"]),
            float(&sound["pitch"])
        )
        .unwrap();
        for field in ["break", "step", "place", "hit", "fall"] {
            write!(
                out,
                "{field}_sound: registries::sound_event::Id({}),",
                sound[field]
            )
            .unwrap();
        }
        out.push_str("},\n");
    }
    out.push_str("];\npub const BLOCK_STATES: &[BlockStateData] = &[\n");
    for (id, state) in states.iter().enumerate() {
        assert_eq!(number(&state["id"]), id as u64);
        write!(
            out,
            "BlockStateData {{ block: Block({}), destroy_time: {},",
            state["block"],
            float(&state["destroy_time"])
        )
        .unwrap();
        for field in [
            "light_emission",
            "context_dependent_shape",
            "solid_render",
            "opacity",
            "propagates_skylight",
            "use_shape_for_light_occlusion",
            "map_color",
            "requires_correct_tool",
            "random_ticking",
            "replaceable",
        ] {
            write!(out, "{field}: {},", state[field]).unwrap();
        }
        let render = string(&render_shapes[id]);
        assert!(matches!(render, "invisible" | "model"));
        write!(out, "render_shape: RenderShape::{},", pascal(render)).unwrap();
        for field in ["collision", "outline", "occlusion"] {
            write!(out, "{field}: ShapeId({}),", state[field]).unwrap();
        }
        write!(
            out,
            "push_reaction: PushReaction::{}, sound: &SOUND_TYPES[{}],",
            pascal(string(&state["push_reaction"])),
            sounds[&serde_json::to_string(&state["sound"]).unwrap()]
        )
        .unwrap();
        let fluid = &state["fluid"];
        write!(out, "fluid: FluidState {{ fluid_type: registries::fluid::Id({}), amount: {}, source: {}, empty: {}, falling: {} }}", fluid["type"], fluid["amount"], fluid["source"], fluid["empty"], fluid["state"]["properties"]["falling"] == "true").unwrap();
        out.push_str("},\n");
    }
    out.push_str("];\n");
}

fn shapes(out: &mut String, shapes: &Value) {
    let shapes = shapes.as_array().unwrap();
    assert!(shapes.len() <= u16::MAX as usize);
    out.push_str("pub const SHAPES: &[&[Aabb]] = &[\n");
    for shape in shapes {
        out.push_str("&[");
        for aabb in shape.as_array().unwrap() {
            let c: Vec<_> = aabb.as_array().unwrap().iter().map(float).collect();
            assert_eq!(c.len(), 6);
            write!(
                out,
                "Aabb {{ min: [{},{},{}], max: [{},{},{}] }},",
                c[0], c[1], c[2], c[3], c[4], c[5]
            )
            .unwrap();
        }
        out.push_str("],\n");
    }
    out.push_str("];\n");
}

fn voxel_shapes(data: &Value, block_properties: &Value) -> String {
    let snapshots = data["snapshots"].as_array().unwrap();
    let instances = data["shapes"].as_array().unwrap();
    let states = data["states"].as_array().unwrap();
    let contexts = data["context_dependent"].as_array().unwrap();
    let properties = block_properties["states"].as_array().unwrap();
    assert!(snapshots.len() <= u16::MAX as usize && instances.len() <= u16::MAX as usize);
    assert_eq!(states.len(), properties.len());
    assert_eq!(contexts.len(), states.len());
    let mut out = String::from(
        "// Generated native vanilla 26.3 geometry.\nstatic SNAPSHOTS: &[NativeSnapshot] = &[\n",
    );
    for snapshot in snapshots {
        writeln!(out, "NativeSnapshot {{ kind: VoxelShapeKind::{}, singleton: Singleton::{}, size: {}, bounds: {}, coordinates: [",
            pascal(string(&snapshot["kind"])), pascal(string(&snapshot["singleton"])), snapshot["size"], snapshot["bounds"]).unwrap();
        for coords in snapshot["coordinates"].as_array().unwrap() {
            if let Some(size) = coords["cube"].as_u64() {
                write!(out, "NativeCoordinates::Cube({size}),").unwrap();
            } else {
                out.push_str("NativeCoordinates::Array(&[");
                for bits in coords.as_array().unwrap() {
                    let bits = u64::from_str_radix(string(bits), 16).unwrap();
                    write!(out, "0x{bits:x},").unwrap();
                }
                out.push_str("]),");
            }
        }
        out.push_str("], bits: &[");
        for word in snapshot["bits"].as_array().unwrap() {
            let word = u64::from_str_radix(string(word), 16).unwrap();
            write!(out, "0x{word:x},").unwrap();
        }
        out.push_str("] },\n");
    }
    out.push_str("];\nstatic SHAPE_SNAPSHOTS: &[u16] = &[");
    for instance in instances {
        assert!((number(instance) as usize) < snapshots.len());
        write!(out, "{instance},").unwrap();
    }
    writeln!(out, "];\nstatic NATIVE_SHAPES: [OnceLock<VoxelShape>; {}] = [const {{ OnceLock::new() }}; {}];\nstatic STATE_SHAPES: &[BlockStateShapes] = &[", instances.len(), instances.len()).unwrap();
    for (id, row) in states.iter().enumerate() {
        let indexes = row.as_array().unwrap();
        assert_eq!(indexes.len(), 9);
        assert!(
            indexes
                .iter()
                .all(|v| (number(v) as usize) < instances.len())
        );
        assert_eq!(contexts[id], properties[id]["context_dependent_shape"]);
        write!(out, "BlockStateShapes {{ collision: NativeShapeId({}), outline: NativeShapeId({}), occlusion: NativeShapeId({}), face_occlusion: [",indexes[0],indexes[1],indexes[2]).unwrap();
        for face in &indexes[3..] {
            write!(out, "NativeShapeId({face}),").unwrap();
        }
        writeln!(out, "], context_dependent: {} }},", contexts[id]).unwrap();
    }
    out.push_str("];\n");
    out
}

fn items(out: &mut String, items: &Value, crafting: &Value) {
    out.push_str("pub const ITEMS: &[ItemData] = &[\n");
    for (id, item) in items.as_array().unwrap().iter().enumerate() {
        assert_eq!(number(&item["id"]), id as u64);
        let block = item["block"]
            .as_u64()
            .map_or_else(|| "None".into(), |id| format!("Some(Block({id}))"));
        let facts = &crafting[id];
        let remainder = facts["remainder"].as_object().map_or_else(|| "None".to_owned(), |r| {
            let item_id = items.as_array().unwrap().iter().position(|item| item["name"] == r["id"]).expect("crafting remainder item");
            let count = r.get("count").and_then(Value::as_i64).unwrap_or(1);
            let components = r.get("components").cloned().unwrap_or_else(|| serde_json::json!({}));
            format!("Some(CraftingRemainder {{ item: Item({item_id}), count: {count}, components_json: {:?} }})", serde_json::to_string(&components).unwrap())
        });
        let banner_color = facts["banner_color"]
            .as_str()
            .map_or_else(|| "None".to_owned(), |name| format!("Some({name:?})"));
        writeln!(out, "ItemData {{ crafting_remainder: {remainder}, banner_color: {banner_color}, max_stack_size: {}, max_damage: {}, rarity: Rarity::{}, block: {block}, default_components_json: {:?} }},", item["max_stack_size"], item["max_damage"], pascal(string(&item["rarity"])), serde_json::to_string(&item["components"]).unwrap()).unwrap();
    }
    out.push_str("];\n");
}

fn item_names(out: &mut String, facts: &Value, items: &Value, registries: &Value) {
    let rows = facts["items"].as_array().expect("item description facts");
    assert_eq!(rows.len(), items.as_array().unwrap().len());
    let descriptions: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(id, row)| {
            assert_eq!(number(&row["id"]) as usize, id);
            assert_eq!(row["name"], items[id]["name"]);
            string(&row["description_id"])
        })
        .collect();
    writeln!(
        out,
        "static ITEM_DESCRIPTION_IDS: [&str; {}] = {:?};",
        descriptions.len(),
        descriptions
    )
    .unwrap();
    let rows = facts["potions"].as_array().expect("potion name facts");
    let entries = ordered_entries(&registries["minecraft:potion"]["entries"]);
    assert_eq!(rows.len(), entries.len());
    let names: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(id, row)| {
            assert_eq!(number(&row["id"]) as usize, id);
            assert_eq!(string(&row["name"]), entries[id].0);
            string(&row["effect_name"])
        })
        .collect();
    writeln!(
        out,
        "const POTION_EFFECT_NAMES: [&str; {}] = {:?};",
        names.len(),
        names
    )
    .unwrap();
}

fn attribute_properties(out: &mut String, attributes: &Value, registries: &Value) {
    let attributes = attributes.as_array().unwrap();
    assert_eq!(
        attributes.len(),
        registries["minecraft:attribute"]["entries"]
            .as_object()
            .unwrap()
            .len()
    );
    writeln!(
        out,
        "const ATTRIBUTE_DATA: [AttributeData; {}] = [",
        attributes.len()
    )
    .unwrap();
    for (id, attribute) in attributes.iter().enumerate() {
        assert_eq!(number(&attribute["id"]), id as u64);
        assert_eq!(
            number(
                &registries["minecraft:attribute"]["entries"][string(&attribute["name"])]["protocol_id"]
            ),
            id as u64
        );
        write!(
            out,
            "AttributeData {{ default_value: {}, syncable: {}, range: ",
            float(&attribute["default_value"]),
            attribute["syncable"]
        )
        .unwrap();
        match (&attribute["min_value"], &attribute["max_value"]) {
            (Value::Null, Value::Null) => out.push_str("None"),
            (min, max) => {
                assert!(min.as_f64().unwrap() <= max.as_f64().unwrap());
                write!(
                    out,
                    "Some(AttributeRange {{ min: {}, max: {} }})",
                    float(min),
                    float(max)
                )
                .unwrap();
            }
        }
        out.push_str(" },\n");
    }
    out.push_str("];\n");
}

fn entities(out: &mut String, entities: &Value, registries: &Value) {
    out.push_str("pub const ENTITY_TYPES: &[EntityTypeData] = &[\n");
    for (id, entity) in entities.as_array().unwrap().iter().enumerate() {
        assert_eq!(number(&entity["id"]), id as u64);
        out.push_str("EntityTypeData {");
        for field in ["width", "height", "eye_height"] {
            write!(out, "{field}: {},", float(&entity[field])).unwrap();
        }
        for field in [
            "fixed_dimensions",
            "tracking_range",
            "update_interval",
            "track_deltas",
            "fire_immune",
            "summonable",
        ] {
            write!(out, "{field}: {},", entity[field]).unwrap();
        }
        write!(
            out,
            "category: MobCategory::{}, attributes: &[",
            pascal(string(&entity["category"]))
        )
        .unwrap();
        for (name, value) in entity["attributes"].as_object().unwrap() {
            let id = number(&registries["minecraft:attribute"]["entries"][name]["protocol_id"]);
            write!(out, "(registries::attribute::Id({id}), {}),", float(value)).unwrap();
        }
        out.push_str("], attachments: EntityAttachments { points: [");
        for attachment in ["passenger", "vehicle", "name_tag", "warden_chest"] {
            let points = entity["attachments"][attachment].as_array().unwrap();
            assert!(
                !points.is_empty(),
                "entity attachment has no fallback points"
            );
            out.push_str("&[");
            for point in points {
                let values = point.as_array().unwrap();
                assert_eq!(values.len(), 3);
                write!(
                    out,
                    "ferraloom_util::Vec3 {{ x: {}, y: {}, z: {} }},",
                    float(&values[0]),
                    float(&values[1]),
                    float(&values[2])
                )
                .unwrap();
            }
            out.push_str("],");
        }
        out.push_str("] } },\n");
    }
    out.push_str("];\n");
}

/// Pure registry transforms from each block's vanilla virtual methods.
fn state_transforms(out: &mut String, tables: &Value, state_count: usize) {
    for name in [
        "clockwise_90",
        "clockwise_180",
        "counterclockwise_90",
        "left_right",
        "front_back",
    ] {
        let values = tables[name].as_array().expect("state transform array");
        assert_eq!(values.len(), state_count, "state transform table length");
        write!(
            out,
            "static STATE_TRANSFORM_{}: [u16; {}] = [",
            constant(name),
            state_count
        )
        .unwrap();
        for value in values {
            let value = number(value);
            assert!((value as usize) < state_count && value <= u16::MAX as u64);
            write!(out, "{value},").unwrap();
        }
        out.push_str("];\n");
    }
}

// Java class/interface identity is generated separately from state properties.
fn block_classes(out: &mut String, data: &Value, block_count: usize) {
    let classes = data["classes"].as_array().unwrap();
    let blocks = data["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), block_count);
    let names = classes
        .iter()
        .map(|c| string(&c["name"]))
        .collect::<Vec<_>>();
    let variants = names
        .iter()
        .map(|name| pascal(name.rsplit('.').next().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        variants.iter().collect::<BTreeSet<_>>().len(),
        variants.len(),
        "class name collision"
    );
    out.push_str("/// Vanilla concrete block classes, superclasses and implemented interfaces.\n#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\n#[repr(u16)]\npub enum BlockClass {\n");
    for variant in &variants {
        writeln!(out, "{variant},").unwrap();
    }
    out.push_str("}\nimpl BlockClass {\npub const ALL: &[Self] = &[");
    for variant in &variants {
        write!(out, "Self::{variant},").unwrap();
    }
    out.push_str("];\npub const fn java_name(self) -> &'static str { BLOCK_CLASS_NAMES[self as usize] }\n}\n");
    writeln!(
        out,
        "const BLOCK_CLASS_NAMES: [&str; {}] = {names:?};",
        names.len()
    )
    .unwrap();
    write!(out, "const BLOCK_CLASSES: [BlockClass; {block_count}] = [").unwrap();
    for class in blocks {
        write!(out, "BlockClass::{},", variants[number(class) as usize]).unwrap();
    }
    out.push_str("];\n");
    let words = classes.len().div_ceil(64);
    writeln!(
        out,
        "const BLOCK_CLASS_INSTANCES: [[u64; {words}]; {}] = [",
        classes.len()
    )
    .unwrap();
    for class in classes {
        let mut bits = vec![0u64; words];
        for parent in class["instances"].as_array().unwrap() {
            let index = number(parent) as usize;
            assert!(index < classes.len());
            bits[index / 64] |= 1u64 << (index % 64);
        }
        writeln!(out, "{bits:?},").unwrap();
    }
    out.push_str("];\n");
}

/// Cached face occlusion is independent of the level and block position.
fn face_occlusion(out: &mut String, values: &Value, state_count: usize) {
    let values = values.as_array().expect("face occlusion array");
    assert_eq!(values.len(), state_count, "face occlusion table length");
    write!(out, "static FACE_OCCLUSION: [u8; {state_count}] = [").unwrap();
    for value in values {
        let mask = number(value);
        assert!(mask < 64);
        write!(out, "{mask},").unwrap();
    }
    out.push_str("];\n");
}

fn block_state_facts(out: &mut String, values: &Value, state_count: usize) {
    let values = values.as_array().expect("block state facts array");
    assert_eq!(values.len(), state_count, "block state facts table length");
    write!(out, "static BLOCK_STATE_FACTS: [u16; {state_count}] = [").unwrap();
    for value in values {
        let bits = number(value);
        assert!(bits < 32768);
        write!(out, "{bits},").unwrap();
    }
    out.push_str("];\n");
}

/// Tag names and registry identities are extracted separately from membership.
fn tag_keys(root: &Path, registries: &Value) {
    let groups = load(root, "tag_keys");
    let mut out =
        String::from("// Generated vanilla tag keys. Membership belongs to the loaded datapack.\n");
    for (registry, group) in groups.as_object().unwrap() {
        let class = string(&group["class"]);
        let registry_path = registry.strip_prefix("minecraft:").unwrap();
        let entry = if registries.get(registry).is_some() {
            let entry = format!("crate::registries::{}::Id", ident(registry));
            writeln!(out, "impl TagElement for {entry} {{ const VANILLA_TAG_KEYS: &'static [TagKey<Self>] = {class}::ALL; fn tag_index(self) -> usize {{ self.protocol_id() as usize }} }}").unwrap();
            writeln!(out, "impl StaticTagElement for {entry} {{ const REGISTRY: &'static str = {registry_path:?}; const REGISTRY_SIZE: usize = crate::registries::{}::NAMES.len(); fn from_tag_name(name: &Identifier) -> Option<Self> {{ Self::from_name(&name.to_string()) }} }}", ident(registry)).unwrap();
            entry
        } else {
            let entry = format!("{}Entry", class.trim_end_matches("Tags"));
            writeln!(out, "/// Runtime id in the loaded {registry} registry.\n#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)] pub struct {entry}(pub u32);").unwrap();
            writeln!(
                out,
                "impl TagElement for {entry} {{ const VANILLA_TAG_KEYS: &'static [TagKey<Self>] = {class}::ALL; fn tag_index(self) -> usize {{ self.0 as usize }} }}"
            )
            .unwrap();
            entry
        };
        writeln!(out, "pub struct {class};\nimpl {class} {{").unwrap();
        for (ordinal, (key, name)) in group["keys"].as_object().unwrap().iter().enumerate() {
            writeln!(
                out,
                "pub const {key}: TagKey<{entry}> = TagKey::vanilla({name}, {ordinal});"
            )
            .unwrap();
        }
        writeln!(out, "pub const ALL: &[TagKey<{entry}>] = &[").unwrap();
        for key in group["keys"].as_object().unwrap().keys() {
            writeln!(out, "Self::{key},").unwrap();
        }
        out.push_str("];\n}\n");
    }
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("tag_keys.rs"),
        out,
    )
    .unwrap();
}

fn block_entity_types(out: &mut String, report: &Value, block_count: usize) {
    let rows = report.as_array().expect("block entity type array");
    let mut by_block = vec![None; block_count];
    out.push_str("pub static BLOCK_ENTITY_TYPES: &[BlockEntityTypeData] = &[\n");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(number(&row["id"]) as usize, index);
        write!(
            out,
            "BlockEntityTypeData {{ only_op_can_set_nbt: {}, is_nameable: {}, valid_blocks: &[",
            row["only_op_can_set_nbt"],
            row["is_nameable"].as_bool().expect("Nameable fact")
        )
        .unwrap();
        for block in row["valid_blocks"].as_array().unwrap() {
            let id = number(block) as usize;
            assert!(
                by_block[id].replace(index).is_none(),
                "block in two block entity types"
            );
            write!(out, "registries::block::Id({id}),").unwrap();
        }
        out.push_str("] },\n");
    }
    out.push_str("];\npub static BLOCK_ENTITY_TYPE_BY_BLOCK: &[Option<registries::block_entity_type::Id>] = &[\n");
    for kind in by_block {
        match kind {
            Some(id) => writeln!(out, "Some(registries::block_entity_type::Id({id})),").unwrap(),
            None => out.push_str("None,\n"),
        }
    }
    out.push_str("];\n");
}

fn main() {
    let root = Path::new("../../data/generated");
    let registries_report = load(root, "registries");
    tag_keys(root, &registries_report);
    let block_report = load(root, "blocks");
    let block_properties = load(root, "block_properties");
    block_methods::generate(root, &block_properties);
    redstone_properties::generate(root);
    dispenser_properties::generate(root);
    building_properties::generate(root);
    note_properties::generate(root);
    let item_report = load(root, "items");
    item_methods::generate(root, &item_report);
    let entity_report = load(root, "entity_types");
    let attribute_report = load(root, "attributes");
    for (name, count) in [
        (
            "block",
            block_properties["blocks"].as_array().unwrap().len(),
        ),
        ("item", item_report.as_array().unwrap().len()),
        ("entity_type", entity_report.as_array().unwrap().len()),
    ] {
        assert_eq!(
            registries_report[format!("minecraft:{name}")]["entries"]
                .as_object()
                .unwrap()
                .len(),
            count
        );
    }
    let mut out = String::from("// Generated from the pinned vanilla 26.3 server. Do not edit.\n");
    let mut language = load(root, "en_us");
    let deprecated = load(root, "deprecated_translations");
    let entries = language.as_object_mut().unwrap();
    for key in deprecated["removed"].as_array().unwrap() {
        entries.remove(key.as_str().unwrap());
    }
    for (from, to) in deprecated["renamed"].as_object().unwrap() {
        let value = entries.remove(from);
        let to = to.as_str().unwrap();
        if let Some(value) = value {
            entries.insert(to.into(), value);
        } else {
            entries.remove(to);
        }
    }
    writeln!(
        out,
        "pub const DEFAULT_LANGUAGE_JSON: &str = {:?};",
        language.to_string()
    )
    .unwrap();
    registries(&mut out, &registries_report);
    command_block_properties::generate(
        &mut out,
        &load(root, "command_block_properties"),
        block_report.as_object().unwrap().len(),
    );
    let mut copper = String::new();
    copper_chest_properties::generate(
        &mut copper,
        &load(root, "copper_chest_properties"),
        block_report.as_object().unwrap().len(),
    );
    std::fs::write(
        Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("copper_chest_properties.rs"),
        copper,
    )
    .unwrap();
    let fluid_random_ticking = load(root, "fluid_random_ticking");
    let fluid_random_ticking = fluid_random_ticking
        .as_array()
        .expect("fluid random-tick facts");
    let fluid_count = registries_report["minecraft:fluid"]["entries"]
        .as_object()
        .unwrap()
        .len();
    assert_eq!(fluid_random_ticking.len(), fluid_count);
    writeln!(
        out,
        "static FLUID_RANDOM_TICKING: [bool; {fluid_count}] = {:?};",
        fluid_random_ticking
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect::<Vec<_>>()
    )
    .unwrap();
    block_classes(
        &mut out,
        &load(root, "block_classes"),
        block_properties["blocks"].as_array().unwrap().len(),
    );
    packets(&mut out, &load(root, "packets"));
    let props = properties(&mut out, &block_report);
    blocks(
        &mut out,
        &block_report,
        &block_properties,
        &props,
        &load(root, "block_render_shapes"),
    );
    shapes(&mut out, &load(root, "shapes"));
    state_transforms(
        &mut out,
        &load(root, "state_transforms"),
        block_properties["states"].as_array().unwrap().len(),
    );
    let state_count = block_properties["states"].as_array().unwrap().len();
    let mut is_air = vec![false; state_count];
    for id in load(root, "air_states").as_array().unwrap() {
        is_air[number(id) as usize] = true;
    }
    writeln!(
        out,
        "static STATE_IS_AIR: [bool; {state_count}] = {is_air:?};"
    )
    .unwrap();
    face_occlusion(
        &mut out,
        &load(root, "face_occlusion"),
        block_properties["states"].as_array().unwrap().len(),
    );
    block_state_facts(
        &mut out,
        &load(root, "block_state_facts"),
        block_properties["states"].as_array().unwrap().len(),
    );
    center_support::generate(
        &mut out,
        &load(root, "block_center_support"),
        block_properties["states"].as_array().unwrap().len(),
    );
    let collision_flags = load(root, "block_collision_flags");
    let collision_flags = collision_flags.as_array().expect("block collision flags");
    let block_count = block_properties["blocks"].as_array().unwrap().len();
    assert_eq!(collision_flags.len(), block_count);
    let block_items = load(root, "block_items");
    let block_items = block_items.as_array().expect("block items");
    assert_eq!(block_items.len(), block_count);
    write!(out, "static BLOCK_ITEMS: [Item; {block_count}] = [").unwrap();
    for item in block_items {
        let item = number(item);
        assert!((item as usize) < item_report.as_array().unwrap().len());
        write!(out, "Item({item}),").unwrap();
    }
    out.push_str("];\n");
    writeln!(
        out,
        "static BLOCK_HAS_COLLISION: [bool; {block_count}] = {:?};",
        collision_flags
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect::<Vec<_>>()
    )
    .unwrap();
    let redstone = load(root, "redstone_state_facts");
    let redstone = redstone.as_array().expect("redstone state facts array");
    assert_eq!(redstone.len(), state_count);
    writeln!(
        out,
        "static REDSTONE_STATE_FACTS: [u8; {state_count}] = {:?};",
        redstone.iter().map(|v| number(v) as u8).collect::<Vec<_>>()
    )
    .unwrap();
    items(&mut out, &item_report, &load(root, "item_crafting"));
    item_names(
        &mut out,
        &load(root, "item_names"),
        &item_report,
        &registries_report,
    );
    block_drop_tables::generate(&mut out, &load(root, "block_drop_tables"), block_count);
    feature_requirements::generate(
        &mut out,
        &load(root, "feature_requirements"),
        block_count,
        item_report.as_array().expect("item report").len(),
    );
    game_events::generate(
        &mut out,
        &load(root, "game_events"),
        &registries_report["minecraft:game_event"],
    );
    sound_events(
        &mut out,
        &load(root, "sound_events"),
        &registries_report["minecraft:sound_event"],
    );
    block_entity_types(
        &mut out,
        &load(root, "block_entity_types"),
        registries_report["minecraft:block"]["entries"]
            .as_object()
            .unwrap()
            .len(),
    );
    environment_attributes::generate(
        &mut out,
        &load(root, "environment_attributes"),
        &registries_report["minecraft:environment_attribute"],
    );
    entities(&mut out, &entity_report, &registries_report);
    attribute_properties(&mut out, &attribute_report, &registries_report);
    let path = Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("data.rs");
    std::fs::write(path, out).unwrap();
    let native = voxel_shapes(&load(root, "voxel_shapes"), &block_properties);
    let path = Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("voxel_shapes.rs");
    std::fs::write(path, native).unwrap();
}
