//! Tests the actual configuration snapshot and datapack loader. Source-only; UNRUN.

use super::*;
use crate::{data::datapack::DatapackManager, server::recipe::RecipeManager};
use pumpkin_data::{data_component::DataComponent, data_component_impl::JukeboxPlayableImpl};
use pumpkin_protocol::{ClientPacket, codec::data_component, ser::NetworkReadExt};

#[test]
fn accepted_snapshot_binds_all_song_wire_ids_and_properties_to_serialized_registry() {
    let snapshot = registry_snapshot(&DatapackManager::new()).unwrap();
    let registry = snapshot
        .iter()
        .find(|registry| registry.registry_id == "minecraft:jukebox_song")
        .unwrap();
    let mut packet = Vec::new();
    CRegistryData::new(&registry.registry_id, &registry.registry_entries)
        .write_packet_data(&mut packet, &CURRENT_MC_VERSION)
        .unwrap();
    let mut reader = packet.as_slice();
    assert_eq!(reader.get_str().unwrap().as_ref(), "minecraft:jukebox_song");
    assert_eq!(reader.get_var_int().unwrap().0, 22);
    let expected_names = [
        "11",
        "13",
        "5",
        "blocks",
        "bounce",
        "cat",
        "chirp",
        "creator",
        "creator_music_box",
        "far",
        "lava_chicken",
        "mall",
        "mellohi",
        "otherside",
        "pigstep",
        "precipice",
        "relic",
        "stal",
        "strad",
        "tears",
        "wait",
        "ward",
    ];
    for (index, name) in expected_names.into_iter().enumerate() {
        let key = reader.get_str().unwrap();
        assert_eq!(key.as_ref(), format!("minecraft:{name}"));
        let song = JukeboxSong::from_name(&key).unwrap();
        assert!(reader.get_bool().unwrap());
        let value = reader
            .get_nbt_with_version(&CURRENT_MC_VERSION)
            .unwrap()
            .unwrap();
        let value = value.extract_compound().unwrap();
        assert_eq!(
            value.get_double("length_in_seconds"),
            Some(f64::from(song.length_in_seconds()))
        );
        assert_eq!(
            value.get_int("comparator_output"),
            Some(i32::from(song.comparator_output()))
        );
        assert_eq!(
            value.get_string("sound_event"),
            Some(format!("minecraft:music_disc.{name}").as_str())
        );
        assert_eq!(
            value
                .get_compound("description")
                .unwrap()
                .get_string("translate"),
            Some(format!("jukebox_song.minecraft.{name}").as_str())
        );
        let mut wire = Vec::new();
        data_component::serialize(
            DataComponent::JukeboxPlayable,
            &JukeboxPlayableImpl { song },
            &mut wire,
        )
        .unwrap();
        assert_eq!(wire, [u8::try_from(index + 1).unwrap()]);
        assert_eq!(song.get_id() as usize, index);
    }
    assert!(reader.is_empty());
}

#[test]
fn jukebox_profile_refuses_missing_reordered_or_changed_property_bytes() {
    let mut missing = Registry::get_synced(CURRENT_MC_VERSION);
    missing.retain(|registry| registry.registry_id != "minecraft:jukebox_song");
    assert_eq!(
        validate_jukebox_profile(&missing),
        Err(RegistryProfileError::MissingJukeboxRegistry)
    );
    let mut reordered = Registry::get_synced(CURRENT_MC_VERSION);
    reordered
        .iter_mut()
        .find(|registry| registry.registry_id == "minecraft:jukebox_song")
        .unwrap()
        .registry_entries
        .swap(0, 1);
    assert_eq!(
        validate_jukebox_profile(&reordered),
        Err(RegistryProfileError::UnsupportedJukeboxEntries)
    );
    let mut changed = Registry::get_synced(CURRENT_MC_VERSION);
    changed
        .iter_mut()
        .find(|registry| registry.registry_id == "minecraft:jukebox_song")
        .unwrap()
        .registry_entries[0]
        .data = None;
    assert_eq!(
        validate_jukebox_profile(&changed),
        Err(RegistryProfileError::UnsupportedJukeboxEntries)
    );
}

#[test]
fn actual_datapack_song_definitions_cannot_bypass_configuration_profile() {
    for (namespace, name, definition) in [
        (
            "minecraft",
            "11",
            r#"{"sound_event":"minecraft:music_disc.11","description":{"translate":"jukebox_song.minecraft.11"},"length_in_seconds":72.0,"comparator_output":11}"#,
        ),
        (
            "custom",
            "nested/song",
            r#"{"sound_event":"minecraft:music_disc.11","description":"Custom","length_in_seconds":71.0,"comparator_output":11}"#,
        ),
        ("minecraft", "11", "{malformed"),
    ] {
        let world = tempfile::tempdir().unwrap();
        let pack = world.path().join("datapacks/song-test");
        let definition_path = pack.join(format!("data/{namespace}/jukebox_song/{name}.json"));
        std::fs::create_dir_all(definition_path.parent().unwrap()).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"jukebox profile test","pack_format":94}}"#,
        )
        .unwrap();
        std::fs::write(definition_path, definition).unwrap();
        let datapacks = DatapackManager::new();
        datapacks.load_all(
            world.path(),
            &["file/song-test".into()],
            &RecipeManager::new(),
        );
        assert_eq!(
            datapacks
                .get_custom_registry_entries("minecraft:jukebox_song")
                .len(),
            1
        );
        assert!(matches!(
            registry_snapshot(&datapacks),
            Err(RegistryProfileError::UnsupportedJukeboxEntries)
        ));
    }
    // The existing horn same-snapshot gate remains active after song support.
    let world = tempfile::tempdir().unwrap();
    let pack = world.path().join("datapacks/horn-test");
    let definitions = pack.join("data/minecraft/instrument");
    std::fs::create_dir_all(&definitions).unwrap();
    std::fs::write(
        pack.join("pack.mcmeta"),
        r#"{"pack":{"description":"horn profile control","pack_format":94}}"#,
    )
    .unwrap();
    std::fs::write(definitions.join("ponder_goat_horn.json"), r#"{"sound_event":"minecraft:item.goat_horn.sound.0","description":"Changed","use_duration":7.0,"range":256.0}"#).unwrap();
    let datapacks = DatapackManager::new();
    datapacks.load_all(
        world.path(),
        &["file/horn-test".into()],
        &RecipeManager::new(),
    );
    assert!(matches!(
        registry_snapshot(&datapacks),
        Err(RegistryProfileError::UnsupportedEntries)
    ));
}
