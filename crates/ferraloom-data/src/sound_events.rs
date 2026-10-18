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
        let native: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/generated/sound_events.json"))
                .unwrap();
        for (id, entry) in crate::registries::sound_event::ALL
            .iter()
            .zip(native.as_array().unwrap())
        {
            assert_eq!(id.name(), entry["name"].as_str().unwrap());
            assert_eq!(
                id.fixed_range(),
                entry["fixed_range"].as_f64().map(|n| n as f32)
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
}
