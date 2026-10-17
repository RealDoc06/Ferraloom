use super::*;
use crate::{Identifier, Result};
use ferraloom_data::Item;
use serde_json::Value;

/// Port of DataComponentMap. Item prototypes come from ferraloom-data's extracted defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataComponentMap {
    pub entries: Vec<DataComponent>,
}
impl DataComponentMap {
    pub fn from_item_defaults(item: Item) -> Result<Self> {
        let value: Value = serde_json::from_str(item.data().default_components_json)
            .map_err(|e| crate::Error::invalid("item defaults", e.to_string()))?;
        Self::from_json(&value)
    }
    pub fn from_json(value: &Value) -> Result<Self> {
        let values = value.as_object().ok_or_else(|| invalid("component map"))?;
        Ok(Self {
            entries: values
                .iter()
                .map(|(key, value)| DataComponent::from_json(&Identifier::parse(key)?, value))
                .collect::<Result<_>>()?,
        })
    }
    pub fn get(&self, component_type: ComponentTypeId) -> Option<&DataComponent> {
        self.entries
            .iter()
            .find(|v| v.component_type() == component_type)
    }
    pub fn apply(&mut self, patch: &DataComponentPatch) {
        self.entries
            .retain(|v| !patch.removed.contains(&v.component_type()));
        for value in &patch.added {
            if let Some(old) = self
                .entries
                .iter_mut()
                .find(|v| v.component_type() == value.component_type())
            {
                *old = value.clone();
            } else {
                self.entries.push(value.clone());
            }
        }
    }
    pub fn to_json(&self) -> Result<Value> {
        Ok(Value::Object(
            self.entries
                .iter()
                .map(|v| Ok((v.component_type().name().into(), v.to_json()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
