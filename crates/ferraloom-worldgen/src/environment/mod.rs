//! Shared immutable environment attribute decoding. Live clocks and biome access
//! remain with the concrete Level; both live/worldgen readers use these entries.
mod definitions;
mod system;
mod timeline;
mod value;
use crate::{Error, Identifier};
pub use definitions::{DimensionEnvironment, EnvironmentDefinitions};
use ferraloom_data::registries::{
    attribute_type,
    environment_attribute::{self, Id},
};
use serde_json::Value;
use std::marker::PhantomData;
pub use system::{EnvironmentAttributeSystem, SpatialAttributeInterpolator, WeatherSample};
pub use timeline::{AttributeTrack, ClockSnapshot, SampleError, TimeMarker, Timeline};
pub use value::{AttributeEntry, AttributeValue, BooleanModifier, ScalarValue, TriState};

/// A typed registry key, resolved once rather than by strings at each sample.
#[derive(Clone, Copy, Debug)]
pub struct AttributeKey<T> {
    id: Id,
    value: PhantomData<fn() -> T>,
}
impl<T> AttributeKey<T> {
    pub const fn new(id: Id) -> Self {
        Self {
            id,
            value: PhantomData,
        }
    }
    pub const fn id(&self) -> Id {
        self.id
    }
}
pub struct EnvironmentAttributes;
impl EnvironmentAttributes {
    pub const SKY_LIGHT_LEVEL: AttributeKey<f32> = AttributeKey::new(Id::GAMEPLAY_SKY_LIGHT_LEVEL);
    pub const SUN_ANGLE: AttributeKey<f32> = AttributeKey::new(Id::VISUAL_SUN_ANGLE);
    pub const WATER_EVAPORATES: AttributeKey<bool> =
        AttributeKey::new(Id::GAMEPLAY_WATER_EVAPORATES);
    pub const FAST_LAVA: AttributeKey<bool> = AttributeKey::new(Id::GAMEPLAY_FAST_LAVA);
    pub const BEES_STAY_IN_HIVE: AttributeKey<bool> =
        AttributeKey::new(Id::GAMEPLAY_BEES_STAY_IN_HIVE);
}
/// Required-value failure, kept distinct from malformed datapack JSON. An
/// unported value family is never replaced with its default during sampling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedAttribute(pub Id);

#[derive(Clone, Debug)]
pub struct AttributeMap {
    entries: Vec<Option<AttributeEntry>>,
}
impl Default for AttributeMap {
    fn default() -> Self {
        Self {
            entries: vec![None; environment_attribute::ALL.len()],
        }
    }
}
impl AttributeMap {
    pub fn decode(value: &Value, positional_only: bool) -> Result<Self, Error> {
        let object = value
            .as_object()
            .ok_or_else(|| Error("expected environment attribute map".into()))?;
        let mut result = Self::default();
        for (name, value) in object {
            let name: Identifier = name.parse()?;
            let id = Id::from_name(&name.to_string())
                .ok_or_else(|| Error(format!("unknown environment attribute {name}")))?;
            if positional_only && !id.data().positional {
                return Err(Error(format!("attribute {name} cannot be positional")));
            }
            let slot = &mut result.entries[id.protocol_id() as usize];
            if slot.is_some() {
                return Err(Error(format!("duplicate environment attribute {name}")));
            }
            *slot = Some(AttributeEntry::decode(id, value)?);
        }
        Ok(result)
    }
    pub fn get(&self, id: Id) -> Option<&AttributeEntry> {
        self.entries[id.protocol_id() as usize].as_ref()
    }
    pub fn attributes(&self) -> impl Iterator<Item = Id> + '_ {
        environment_attribute::ALL
            .iter()
            .copied()
            .filter(|id| self.get(*id).is_some())
    }
    pub fn apply(&self, id: Id, base: ScalarValue) -> Result<ScalarValue, UnsupportedAttribute> {
        match self.get(id) {
            Some(entry) => entry.apply(id, base),
            None => Ok(base),
        }
    }
    /// Cold-path constant-map evaluation. Live samplers decode their complete
    /// native default table once when constructing the dimension runtime.
    pub fn value<T: AttributeValue>(
        &self,
        key: AttributeKey<T>,
    ) -> Result<T, UnsupportedAttribute> {
        let id = key.id;
        let default = ScalarValue::native_default(id)?;
        let value = self.apply(id, default)?.sanitize(id);
        T::project(value).ok_or(UnsupportedAttribute(id))
    }
}
pub(crate) fn supports_scalar(id: Id) -> bool {
    matches!(
        id.data().attribute_type,
        attribute_type::Id::BOOLEAN
            | attribute_type::Id::FLOAT
            | attribute_type::Id::ANGLE_DEGREES
            | attribute_type::Id::TRI_STATE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_modifiers_and_sanitization_match_native_attribute_system() {
        let mut count = 0;
        for line in include_str!("../../tests/java/environment/modifiers.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let id = Id::from_name(fields[0]).unwrap();
            let base = match id.data().attribute_type {
                attribute_type::Id::BOOLEAN => ScalarValue::Boolean(fields[1].parse().unwrap()),
                _ => ScalarValue::Float(fields[1].parse().unwrap()),
            };
            let json: Value = serde_json::from_str(fields[2]).unwrap();
            let actual = AttributeEntry::decode(id, &json)
                .map(|entry| entry.apply(id, base).unwrap().sanitize(id));
            if fields[3] == "error" {
                assert!(actual.is_err(), "{line}");
            } else {
                let actual = match actual.unwrap() {
                    ScalarValue::Boolean(value) => value.to_string(),
                    ScalarValue::Float(value) => value.to_bits().to_string(),
                    ScalarValue::TriState(_) => unreachable!(),
                };
                assert_eq!(actual, fields[3], "{line}");
            }
            count += 1;
        }
        assert_eq!(count, 99);
    }
    #[test]
    fn normalized_keys_and_required_unported_values_do_not_use_defaults() {
        let value =
            serde_json::json!({"gameplay/water_evaporates":{"modifier":"xor","argument":true}});
        let map = AttributeMap::decode(&value, true).unwrap();
        assert_eq!(map.value(EnvironmentAttributes::WATER_EVAPORATES), Ok(true));
        let duplicate = serde_json::json!({"gameplay/water_evaporates":true,"minecraft:gameplay/water_evaporates":false});
        assert!(AttributeMap::decode(&duplicate, false).is_err());
        assert!(
            AttributeMap::decode(&serde_json::json!({"gameplay/fast_lava":true}), true).is_err()
        );
        let map =
            AttributeMap::decode(&serde_json::json!({"audio/background_music":{}}), false).unwrap();
        let id = Id::from_name("minecraft:audio/background_music").unwrap();
        assert!(matches!(map.get(id), Some(AttributeEntry::Unported(_))));
        assert_eq!(
            map.apply(id, ScalarValue::Boolean(false)),
            Err(UnsupportedAttribute(id))
        );
    }
}
