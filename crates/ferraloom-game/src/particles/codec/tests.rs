use super::*;
use ferraloom_nbt::Tag;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    format: String,
    input: String,
    wire: Option<String>,
    error: Option<String>,
}

#[test]
fn native_particle_types_codec_json_and_nbt_match_values_wire_and_errors() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("../codec-vectors.json")).unwrap();
    assert_eq!(cases.len(), 388);
    let mut counts = (0, 0);
    let registries = crate::commands::context::CommandBuildContext::default();
    for case in cases {
        let result = match case.format.as_str() {
            "nbt" => {
                counts.0 += 1;
                let input: Tag = ferraloom_nbt::parse(&case.input).unwrap();
                decode(&input)
            }
            "json" => {
                counts.1 += 1;
                let input: serde_json::Value = serde_json::from_str(&case.input).unwrap();
                decode(&input)
            }
            _ => panic!("unknown input format"),
        };
        match result {
            Ok(particle) => {
                assert_eq!(case.error, None, "{} {}", case.format, case.input);
                let mut bytes = Writer::with_context(&registries);
                particle.encode(&mut bytes).unwrap();
                let wire: String = bytes
                    .into_inner()
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect();
                assert_eq!(Some(wire), case.wire, "{} {}", case.format, case.input);
            }
            Err(error) => {
                assert_eq!(case.wire, None, "{} {}: {error}", case.format, case.input);
                assert_eq!(
                    Some(error.to_string()),
                    case.error,
                    "{} {}",
                    case.format,
                    case.input
                );
            }
        }
    }
    assert_eq!(counts, (191, 197));
}
