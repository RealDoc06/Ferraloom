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

#[path = "commands_summon/live.rs"]
mod live;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the canonical summon construction release and both Strict disposable servers"]
async fn parity_summon_construction_pairing_passengers_and_uuid_failures() -> Result<()> {
    live::run().await
}

#[test]
fn implicit_cow_pairing_relaxes_only_metadata_proven_variable_in_two_jvms() -> Result<()> {
    use ferraloom_parity::protocol::{
        Identifier, Reader, Wire, Writer, codec::RegistryContext, entity_data::EntityDataValue,
        play::clientbound as cb,
    };
    // Native CowSoundVariants.bootstrap registers classic then moody. Comparing
    // each decoded field with the independently named Java row checks this view.
    struct CowVariants;
    impl RegistryContext for CowVariants {
        fn entry_id(&self, _: &Identifier, _: &Identifier) -> Option<i32> {
            None
        }
        fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
            if registry.as_str() != "minecraft:cow_sound_variant" {
                return None;
            }
            Identifier::parse(match id {
                0 => "minecraft:classic",
                1 => "minecraft:moody",
                _ => return None,
            })
            .ok()
        }
        fn has_tag(&self, _: &Identifier, _: &Identifier) -> bool {
            false
        }
    }
    let captures: [Vec<Value>; 2] = serde_json::from_str(include_str!(
        "../../ferraloom-game/tests/java/commands/summon/cow-pairing-proof.json"
    ))?;
    let mut stable = None;
    for capture in &captures {
        assert_eq!(capture.len(), 128);
        let mut hands = std::collections::BTreeSet::new();
        let mut sounds = std::collections::BTreeSet::new();
        for sample in capture {
            let handed = sample["left_handed"].as_bool().unwrap();
            hands.insert(handed);
            sounds.insert(sample["sound_variant"].as_str().unwrap());
            let amount = sample["follow_range_amount"].as_f64().unwrap();
            assert!(
                amount.is_finite()
                    && (-0.11485000000000001..=0.11485000000000001).contains(&amount)
            );
            assert_eq!(sample["follow_range_syncable"], false);
            let mut packets = sample["packets"].as_array().unwrap().clone();
            assert_eq!(
                packets
                    .iter()
                    .map(|p| p["kind"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                [
                    "ClientboundAddEntityPacket",
                    "ClientboundSetEntityDataPacket"
                ]
            );
            for entry in &mut packets {
                if entry["kind"] != "ClientboundSetEntityDataPacket" {
                    continue;
                }
                let encoded = entry["wire"].as_str().unwrap();
                let bytes = (0..encoded.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let mut reader = Reader::with_context(&bytes, &CowVariants);
                let packet = cb::SetEntityData::decode(&mut reader)?;
                reader.finish()?;
                let flags = packet.packed_items.0.iter().find(|entry| entry.id == 15);
                assert_eq!(flags.is_some(), handed);
                let sound = packet
                    .packed_items
                    .0
                    .iter()
                    .find_map(|entry| match (&entry.value, entry.id) {
                        (EntityDataValue::CowSoundVariant(sound), 19) => Some(sound.0.as_str()),
                        _ => None,
                    })
                    .unwrap_or("minecraft:classic");
                assert_eq!(sound, sample["sound_variant"].as_str().unwrap());
                let normalized = live::cow_metadata(&packet)?;
                let mut writer = Writer::new();
                normalized.encode(&mut writer)?;
                let actual = writer
                    .as_slice()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                assert_eq!(entry["stable_wire"], actual);
                entry["wire"] = Value::String(actual);
            }
            if let Some(expected) = &stable {
                assert_eq!(&packets, expected);
            } else {
                stable = Some(packets);
            }
        }
        assert_eq!(hands, std::collections::BTreeSet::from([false, true]));
        assert_eq!(
            sounds,
            std::collections::BTreeSet::from(["minecraft:classic", "minecraft:moody"])
        );
    }
    assert!(
        captures[0]
            .iter()
            .zip(&captures[1])
            .any(|(a, b)| a["left_handed"] != b["left_handed"])
    );
    assert!(
        captures[0]
            .iter()
            .zip(&captures[1])
            .any(|(a, b)| a["sound_variant"] != b["sound_variant"])
    );
    Ok(())
}
