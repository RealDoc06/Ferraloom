//! Immutable EnvironmentAttribute registry metadata. Sampling belongs to game;
//! JSON values are decoded once into the shared typed runtime definitions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentAttributeData {
    pub attribute_type: crate::registries::attribute_type::Id,
    pub field: &'static str,
    pub default_json: &'static str,
    pub positional: bool,
    pub spatially_interpolated: bool,
    pub full_resolution_biomes: bool,
    pub syncable: bool,
    pub modifiers: &'static [&'static str],
    pub float_min: Option<f32>,
    pub float_max: Option<f32>,
}
impl crate::registries::environment_attribute::Id {
    pub const fn data(self) -> &'static EnvironmentAttributeData {
        &crate::ENVIRONMENT_ATTRIBUTES[self.protocol_id() as usize]
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn native_metadata_distinguishes_positional_booleans_and_dimension_brightness() {
        use crate::registries::{attribute_type::Id as Type, environment_attribute::Id};
        let sky = Id::from_name("minecraft:gameplay/sky_light_level")
            .unwrap()
            .data();
        assert_eq!(sky.attribute_type, Type::FLOAT);
        assert_eq!((sky.float_min, sky.float_max), (Some(0.0), Some(15.0)));
        assert!(!sky.positional);
        assert_eq!(sky.default_json, "15.0");
        let water = Id::from_name("minecraft:gameplay/water_evaporates")
            .unwrap()
            .data();
        assert!(water.positional);
        assert_eq!(water.attribute_type, Type::BOOLEAN);
        assert_eq!(water.default_json, "false");
        assert_eq!(water.modifiers.len(), 7);
        assert!(water.modifiers.contains(&"xor"));
        let sun = Id::from_name("minecraft:visual/sun_angle").unwrap().data();
        assert_eq!(sun.attribute_type, Type::ANGLE_DEGREES);
        assert!(sun.spatially_interpolated);
        assert!(sun.float_min.is_none() && sun.float_max.is_none());
    }
}
