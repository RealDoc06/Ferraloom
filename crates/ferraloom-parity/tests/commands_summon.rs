//! Summon UUID normalization is limited to fields proven variable in two JVMs.
use ferraloom_parity::{Normalization, ObservationLog, Result};
use serde_json::Value;

#[test]
fn implicit_summon_uuid_aliases_match_two_independent_native_jvms() -> Result<()> {
    let captures: [Vec<Value>; 2] = serde_json::from_str(include_str!(
        "../../ferraloom-game/tests/java/commands/summon/uuid-proof.json"
    ))?;
    assert_eq!(captures[0].len(), 5);
    assert_eq!(captures[1].len(), 5);
    for (first, second) in captures[0].iter().zip(&captures[1]) {
        assert_eq!(first["command"], second["command"]);
        assert_ne!(first["uuid"], second["uuid"]);
        for sample in [first, second] {
            let words: [i32; 4] =
                serde_json::from_value(sample["display_name"]["hover_event"]["uuid"].clone())?;
            let uuid = ferraloom_util::Uuid::from_int_array(words).to_string();
            assert_eq!(sample["uuid"], uuid);
            assert_eq!(sample["display_name"]["insertion"], uuid);
        }
    }
    let normalize = |capture: &[Value]| -> Result<ObservationLog> {
        let mut log = ObservationLog::default();
        log.record("summons", capture)?;
        // Real implicit-NBT summons differ in exactly these UUID fields between
        // JVMs. Explicit-NBT fixtures never use these normalization rules.
        let rules = (0..capture.len())
            .flat_map(|index| {
                [
                    "uuid",
                    "display_name/hover_event/uuid",
                    "display_name/insertion",
                ]
                .map(|field| Normalization::RandomUuid(format!("/summons/{index}/{field}")))
            })
            .collect::<Vec<_>>();
        log.normalized(&rules)
    };
    normalize(&captures[0])?.compare(&normalize(&captures[1])?)
}
