use super::*;
use crate::commands::exceptions::BrigadierExceptions;
use serde::Deserialize;
#[derive(Deserialize)]
struct Vector {
    parser: String,
    properties: serde_json::Value,
    input: String,
    ok: Option<bool>,
    cursor: usize,
    error_cursor: Option<i32>,
    message: Option<String>,
    value: Option<serde_json::Value>,
    world_coordinates: Option<serde_json::Value>,
    local_coordinates: Option<[f64; 3]>,
}
#[test]
fn argument_parser_vectors_match_java_errors_cursors_and_values() {
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../vanilla/parser-vectors.json")).unwrap();
    let implemented = [
        "brigadier:bool",
        "brigadier:float",
        "brigadier:double",
        "brigadier:integer",
        "brigadier:long",
        "brigadier:string",
        "minecraft:entity",
        "minecraft:game_profile",
        "minecraft:block_pos",
        "minecraft:column_pos",
        "minecraft:vec3",
        "minecraft:vec2",
        "minecraft:rotation",
        "minecraft:block_state",
        "minecraft:team_color",
        "minecraft:hex_color",
        "minecraft:message",
        "minecraft:nbt_compound_tag",
        "minecraft:nbt_tag",
        "minecraft:nbt_path",
        "minecraft:objective",
        "minecraft:objective_criteria",
        "minecraft:scoreboard_slot",
        "minecraft:resource_selector",
        "minecraft:team",
        "minecraft:operation",
        "minecraft:angle",
        "minecraft:score_holder",
        "minecraft:swizzle",
        "minecraft:item_slot",
        "minecraft:item_slots",
        "minecraft:resource_location",
        "minecraft:function",
        "minecraft:entity_anchor",
        "minecraft:int_range",
        "minecraft:float_range",
        "minecraft:dimension",
        "minecraft:gamemode",
        "minecraft:time",
        "minecraft:resource",
        "minecraft:resource_key",
        "minecraft:resource_or_tag_key",
        "minecraft:template_mirror",
        "minecraft:template_rotation",
        "minecraft:heightmap",
        "minecraft:swing_animation",
        "minecraft:uuid",
    ];
    let mut failures = Vec::new();
    for vector in vectors
        .iter()
        .filter(|v| implemented.contains(&v.parser.as_str()))
    {
        if let Err(failure) = std::panic::catch_unwind(|| check_vector(vector)) {
            failures.push(format!(
                "{} {:?}: {:?}",
                vector.parser,
                vector.input,
                failure
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| failure.downcast_ref::<&str>().copied())
                    .unwrap_or("panic")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} vector failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
fn check_vector(vector: &Vector) {
    let mut argument =
        MinecraftArgument::new(wire::from_report(&vector.parser, &vector.properties));
    argument.context = Arc::new(crate::commands::context::CommandBuildContext {
        registries: serde_json::from_str(include_str!("../vanilla/registry-elements.json"))
            .unwrap(),
        tags: Default::default(),
    });
    let mut reader = StringReader::with_provider(&vector.input, Arc::new(BrigadierExceptions));
    let parsed = argument.parse(&mut reader);
    let label = format!("{} {:?}", vector.parser, vector.input);
    if vector.ok == Some(true) {
        let value = parsed.unwrap_or_else(|e| panic!("{label}: {e}"));
        if let Some(expected) = &vector.value {
            let actual = match &value {
                ArgumentValue::Boolean(v) => serde_json::json!(v),
                ArgumentValue::Integer(v) => serde_json::json!(v),
                ArgumentValue::Long(v) => serde_json::json!(v),
                ArgumentValue::Float(v) => serde_json::json!(v),
                ArgumentValue::Double(v) => serde_json::json!(v),
                ArgumentValue::String(v) | ArgumentValue::Identifier(v) => serde_json::json!(v),
                ArgumentValue::HexColor(v) => serde_json::json!(*v as i32),
                _ => expected.clone(),
            };
            assert_eq!(actual, *expected, "{label}");
        }
        if let Some(expected) = &vector.world_coordinates {
            let ArgumentValue::Coordinates(Coordinates::World(coordinates)) = value else {
                panic!("{label}: expected world coordinates");
            };
            let actual = serde_json::json!(
                coordinates.map(|c| serde_json::json!({"relative":c.relative,"value":c.value}))
            );
            assert_eq!(actual, *expected, "{label}");
        }
        if let Some(expected) = vector.local_coordinates {
            let ArgumentValue::Coordinates(Coordinates::Local { left, up, forwards }) = value
            else {
                panic!("{label}: expected local coordinates");
            };
            assert_eq!([left, up, forwards], expected, "{label}");
        }
    } else if vector.ok == Some(false) {
        let error = parsed.expect_err(&label);
        assert_eq!(
            error.cursor.map(|c| c as i32).unwrap_or(-1),
            vector.error_cursor.unwrap(),
            "{label}: {error}"
        );
        assert_eq!(
            error.raw_message.text(),
            vector.message.as_deref().unwrap(),
            "{label}"
        );
    } else {
        return;
    }
    assert_eq!(reader.cursor(), vector.cursor, "{label}: reader cursor");
}
#[test]
fn contextual_selectors_are_rejected_before_parsing_but_messages_remain_literal() {
    let argument = MinecraftArgument::new(CommandArgument::Entity {
        flags: ferraloom_protocol::play::EntityArgumentFlags {
            single: false,
            players_only: false,
        },
    });
    let source = CommandSourceStack::server(0, ferraloom_util::Vec3::ZERO);
    assert_eq!(
        argument
            .parse_with_source(&mut StringReader::new("@a"), &source)
            .unwrap_err()
            .cursor,
        Some(0)
    );
    let message = MinecraftArgument::new(CommandArgument::Message {})
        .parse_with_source(&mut StringReader::new("hello @e[bad]"), &source)
        .unwrap();
    let ArgumentValue::Message(message) = message else {
        panic!("message");
    };
    assert!(message.parts.is_empty());
    assert_eq!(message.text, "hello @e[bad]");
}
