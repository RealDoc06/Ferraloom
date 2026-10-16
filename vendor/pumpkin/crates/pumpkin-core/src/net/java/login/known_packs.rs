#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_data::{
    instrument::Instrument,
    registry::{REGISTRY_V_26_3, Registry},
};

#[derive(Debug, PartialEq, Eq)]
enum InstrumentProfileError {
    MissingRegistry,
    UnsupportedEntries,
}

/// The context-free instrument component codec and gameplay table support the
/// pinned builtin profile only. Check keys, order and full property bytes on
/// the exact merged snapshot that will be sent, before any registry packets.
fn validate_instrument_profile(snapshot: &[Registry]) -> Result<(), InstrumentProfileError> {
    let instrument = snapshot
        .iter()
        .find(|registry| registry.registry_id == "minecraft:instrument")
        .ok_or(InstrumentProfileError::MissingRegistry)?;
    let builtin = REGISTRY_V_26_3
        .iter()
        .find(|registry| registry.registry_id == "instrument")
        .ok_or(InstrumentProfileError::MissingRegistry)?;
    if instrument.registry_entries.len() != Instrument::all().len()
        || builtin.entries.len() != Instrument::all().len()
        || !instrument
            .registry_entries
            .iter()
            .zip(builtin.entries)
            .zip(Instrument::all())
            .enumerate()
            .all(|(id, ((entry, expected), value))| {
                value.id() as usize == id
                    && expected.name == value.to_name()
                    && entry.entry_id == value.asset_id()
                    && entry.data.as_deref() == Some(expected.data)
            })
    {
        return Err(InstrumentProfileError::UnsupportedEntries);
    }
    Ok(())
}

fn registry_snapshot(
    datapacks: &crate::data::datapack::DatapackManager,
) -> Result<Vec<Registry>, InstrumentProfileError> {
    let mut snapshot = Registry::get_synced(CURRENT_MC_VERSION);
    for registry in &mut snapshot {
        registry.registry_entries = if registry.registry_id == "minecraft:damage_type" {
            datapacks.merge_damage_type_entries(&registry.registry_entries)
        } else {
            datapacks.merge_registry_entries(&registry.registry_id, &registry.registry_entries)
        };
    }
    validate_instrument_profile(&snapshot)?;
    Ok(snapshot)
}

impl PendingConnection {
    pub async fn handle_known_packs(&mut self, server: &Server) {
        let version = CURRENT_MC_VERSION;
        let registry = match registry_snapshot(&server.datapack_manager) {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "Unsupported instrument registry profile; refusing configuration"
                );
                self.kick(TextComponent::text("Unsupported instrument registry profile: only the builtin Minecraft 26.3 instruments are supported"))
                    .await;
                return;
            }
        };
        for reg in &registry {
            self.send_packet_now(&CRegistryData::new(&reg.registry_id, &reg.registry_entries))
                .await;
        }
        let mut tags = Vec::new();
        for &key in pumpkin_data::tag::RegistryKey::NETWORK_KEYS {
            if pumpkin_data::tag::get_registry_key_tags(version, key)
                .is_some_and(|map| !map.is_empty())
            {
                tags.push(key);
            }
        }
        self.send_packet_now(&CUpdateTags::new(&tags)).await;
        self.send_packet_now(&CFinishConfig).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::datapack::{
        DatapackManager, dynamic_registry_loader::merge_dynamic_registry_entries,
    };
    use pumpkin_data::{
        data_component_impl::InstrumentImpl, registry::RegistryEntryData, sound::Sound,
    };
    use pumpkin_protocol::{
        ClientPacket, codec::data_component::DataComponentCodec, ser::NetworkReadExt,
    };
    use std::collections::HashMap;

    #[test]
    fn accepted_snapshot_binds_horn_wire_ids_and_properties_to_sent_entries() {
        let snapshot = registry_snapshot(&DatapackManager::new()).unwrap();
        let registry = snapshot
            .iter()
            .find(|registry| registry.registry_id == "minecraft:instrument")
            .unwrap();
        let mut packet = Vec::new();
        CRegistryData::new(&registry.registry_id, &registry.registry_entries)
            .write_packet_data(&mut packet, &CURRENT_MC_VERSION)
            .unwrap();
        let mut reader = packet.as_slice();
        assert_eq!(reader.get_str().unwrap().as_ref(), "minecraft:instrument");
        assert_eq!(reader.get_var_int().unwrap().0, 8);
        for index in 0..8 {
            let key = reader.get_str().unwrap();
            let instrument = Instrument::from_name(&key).unwrap();
            assert!(reader.get_bool().unwrap());
            let value = reader
                .get_nbt_with_version(&CURRENT_MC_VERSION)
                .unwrap()
                .unwrap();
            let value = value.extract_compound().unwrap();
            assert_eq!(value.get_double("use_duration"), Some(7.0));
            assert_eq!(value.get_double("range"), Some(256.0));
            assert_eq!(value.get_int("durability_damage").unwrap_or(0), 0);
            assert_eq!(
                Sound::from_name(
                    value
                        .get_string("sound_event")
                        .unwrap()
                        .strip_prefix("minecraft:")
                        .unwrap()
                ),
                Some(instrument.sound())
            );
            assert_eq!(
                value
                    .get_compound("description")
                    .unwrap()
                    .get_string("translate"),
                Some(instrument.translation_key())
            );
            let mut wire = Vec::new();
            InstrumentImpl { instrument }.serialize(&mut wire).unwrap();
            assert_eq!(wire, vec![index + 1]);
        }
        assert!(reader.is_empty());
    }

    #[test]
    fn instrument_profile_refuses_missing_reordered_custom_and_overridden_entries() {
        let mut missing = Registry::get_synced(CURRENT_MC_VERSION);
        missing.retain(|registry| registry.registry_id != "minecraft:instrument");
        assert_eq!(
            validate_instrument_profile(&missing),
            Err(InstrumentProfileError::MissingRegistry)
        );
        let mut reordered = Registry::get_synced(CURRENT_MC_VERSION);
        reordered
            .iter_mut()
            .find(|registry| registry.registry_id == "minecraft:instrument")
            .unwrap()
            .registry_entries
            .swap(0, 1);
        assert_eq!(
            validate_instrument_profile(&reordered),
            Err(InstrumentProfileError::UnsupportedEntries)
        );

        for key in ["custom:horn", "minecraft:sing_goat_horn"] {
            let mut snapshot = Registry::get_synced(CURRENT_MC_VERSION);
            let instrument = snapshot
                .iter_mut()
                .find(|registry| registry.registry_id == "minecraft:instrument")
                .unwrap();
            // Real merge path: extra custom keys append; builtin overrides keep
            // their position but replace properties. Both must refuse.
            let custom = HashMap::from([(
                key.to_string(),
                RegistryEntryData {
                    entry_id: key.into(),
                    data: instrument.registry_entries[0].data.clone(),
                },
            )]);
            instrument.registry_entries =
                merge_dynamic_registry_entries(&instrument.registry_entries, &custom);
            assert_eq!(
                validate_instrument_profile(&snapshot),
                Err(InstrumentProfileError::UnsupportedEntries)
            );
        }
    }
}
