use super::*;
use crate::commands::{CommandSourceStack, Commands};
use serde::Deserialize;

#[derive(Deserialize)]
struct Vector {
    lines: Vec<String>,
    permission: i32,
    entries: Option<Vec<String>>,
    error: Option<String>,
}
#[test]
fn native_plain_function_compilation_lines_permissions_and_errors() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/plain.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 23);
    let commands = Commands::default();
    for vector in vectors {
        let source = CommandSourceStack::server(vector.permission, ferraloom_util::Vec3::ZERO);
        let result = CompiledFunction::from_lines(
            "test:plain".parse().unwrap(),
            &commands,
            source,
            &vector.lines,
        );
        match result {
            Ok(CompiledFunction::Plain(function)) => assert_eq!(
                Some(
                    function
                        .entries
                        .iter()
                        .map(|entry| entry.input.clone())
                        .collect::<Vec<_>>()
                ),
                vector.entries,
                "{:?}",
                vector.lines
            ),
            Ok(CompiledFunction::MacroPending(_)) => panic!("plain vector became a macro"),
            Err(error) => assert_eq!(Some(error.message), vector.error, "{:?}", vector.lines),
        }
    }
}
#[test]
fn native_function_resource_utf8_replacement_and_line_terminators() {
    #[derive(Deserialize)]
    struct ReaderVector {
        bytes: Vec<u8>,
        lines: Vec<String>,
    }
    let vectors: Vec<ReaderVector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/readers.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 13);
    for vector in vectors {
        assert_eq!(
            read_lines(&vector.bytes),
            vector.lines,
            "{:?}",
            vector.bytes
        );
    }
}
