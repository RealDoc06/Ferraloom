use crate::loot::{Identifier, LevelBasedValue, LootError, Result, codec};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_protocol::component::{
    AttributeModifier, AttributeOperation, EquipmentSlot, JavaF64, Persistent,
};
use serde_json::Value;

/// Port of EnchantmentAttributeEffect's immutable codec and slot-qualified
/// modifiers. Location activation and deactivation use the canonical AttributeMap.
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentAttributeEffect {
    pub id: Identifier,
    pub attribute: Attribute,
    pub amount: LevelBasedValue,
    pub operation: AttributeOperation,
}
impl EnchantmentAttributeEffect {
    pub fn decode(value: &Value) -> Result<Self> {
        let attribute = codec::id(codec::field(value, "attribute")?)?;
        Ok(Self {
            id: codec::id(codec::field(value, "id")?)?,
            attribute: Attribute::from_name(attribute.as_str()).ok_or(
                LootError::MissingReference {
                    registry: "attribute",
                    id: attribute,
                },
            )?,
            amount: LevelBasedValue::decode(codec::field(value, "amount")?)?,
            operation: AttributeOperation::from_json(codec::field(value, "operation")?)?,
        })
    }

    /// Port of EnchantmentAttributeEffect#getModifier / idForSlot. The amount
    /// is calculated as a float before widening to the native double modifier.
    pub fn modifier(&self, level: i32, slot: EquipmentSlot) -> Result<AttributeModifier> {
        let slot = match slot {
            EquipmentSlot::Mainhand => "mainhand",
            EquipmentSlot::Offhand => "offhand",
            EquipmentSlot::Feet => "feet",
            EquipmentSlot::Legs => "legs",
            EquipmentSlot::Chest => "chest",
            EquipmentSlot::Head => "head",
            EquipmentSlot::Body => "body",
            EquipmentSlot::Saddle => "saddle",
        };
        Ok(AttributeModifier {
            id: format!("{}/{slot}", self.id).parse()?,
            amount: JavaF64(f64::from(self.amount.calculate(level)?)),
            operation: self.operation,
        })
    }
}

#[cfg(test)]
mod tests;
