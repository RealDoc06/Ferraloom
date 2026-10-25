use crate::registries::sound_event::Id;

impl Id {
    pub const fn fixed_range(self) -> Option<f32> {
        crate::SOUND_FIXED_RANGES[self.protocol_id() as usize]
    }
    /// Port of SoundEvent#getRange, retaining native f32 arithmetic and NaN
    /// comparison semantics. Facts come from registered SoundEvent records.
    pub fn range(self, volume: f32) -> f32 {
        self.fixed_range()
            .unwrap_or(if volume > 1.0 { 16.0 * volume } else { 16.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sound_ranges_match_registry_facts_and_native_float_boundaries() {
        // Independent capture of BuiltInRegistries.SOUND_EVENT, rather than
        // reading the same generated table that builds fixed_range().
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/registry_parity/sounds.json"
        ))
        .unwrap();
        let entries = native.as_array().unwrap();
        assert_eq!(crate::registries::sound_event::ALL.len(), entries.len());
        for (ordinal, (id, entry)) in crate::registries::sound_event::ALL
            .iter()
            .zip(entries)
            .enumerate()
        {
            assert_eq!(usize::from(id.protocol_id()), ordinal);
            assert_eq!(u64::from(id.protocol_id()), entry["id"].as_u64().unwrap());
            assert_eq!(id.name(), entry["name"].as_str().unwrap());
            assert_eq!(
                id.fixed_range().map(f32::to_bits).map(u64::from),
                entry["fixed_range_bits"].as_u64(),
                "{} fixed range",
                id.name()
            );
        }
        let sound = Id::ENTITY_GENERIC_EXTINGUISH_FIRE;
        for (volume, expected) in [
            (f32::NEG_INFINITY, 16.0),
            (-1.0, 16.0),
            (0.0, 16.0),
            (1.0, 16.0),
            (1.5, 24.0),
            (f32::INFINITY, f32::INFINITY),
            (f32::NAN, 16.0),
        ] {
            assert_eq!(sound.range(volume).to_bits(), expected.to_bits());
        }
    }
    #[test]
    fn jukebox_song_entries_match_vanilla_registry_codec_dump() {
        use crate::datapack::{Pack, ResourceManager};
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/registry_parity/jukebox.json"
        ))
        .unwrap();
        let entries = native.as_array().unwrap();
        let actual = resources.registry("jukebox_song");
        assert_eq!(actual.len(), entries.len());
        for (ordinal, ((name, resource), expected)) in actual.iter().zip(entries).enumerate() {
            assert_eq!(ordinal as u64, expected["id"].as_u64().unwrap());
            assert_eq!(name.to_string(), expected["name"].as_str().unwrap());
            assert_eq!(resource.json().unwrap(), expected["entry"], "{name}");
        }
    }
}
