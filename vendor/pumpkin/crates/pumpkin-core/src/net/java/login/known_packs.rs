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
