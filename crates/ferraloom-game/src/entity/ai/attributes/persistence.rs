use super::{Attribute, AttributeMap, AttributeModifier, AttributeOperation};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::Identifier;
use std::{borrow::Cow, sync::Arc};

/// Port of AttributeInstance.Packed. Only permanent modifiers are persisted.
#[derive(Clone, Debug)]
pub struct PackedAttribute {
    pub attribute: Attribute,
    pub base_value: f64,
    pub modifiers: Vec<Arc<AttributeModifier>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributeLoadError {
    NotAList,
    UnknownAttribute,
    InvalidBase,
    InvalidModifiers,
    InvalidModifier,
}

impl PackedAttribute {
    pub fn to_nbt(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.insert("id", Tag::String(self.attribute.name().into()));
        tag.insert("base", Tag::Double(self.base_value));
        if !self.modifiers.is_empty() {
            let modifiers = self
                .modifiers
                .iter()
                .map(|modifier| {
                    let mut tag = CompoundTag::new();
                    tag.insert("id", Tag::String(modifier.id.as_str().into()));
                    tag.insert("amount", Tag::Double(modifier.amount.0));
                    tag.insert(
                        "operation",
                        Tag::String(
                            match modifier.operation {
                                AttributeOperation::AddValue => "add_value",
                                AttributeOperation::AddMultipliedBase => "add_multiplied_base",
                                AttributeOperation::AddMultipliedTotal => "add_multiplied_total",
                            }
                            .into(),
                        ),
                    );
                    Tag::Compound(tag)
                })
                .collect();
            tag.insert("modifiers", Tag::List(ListTag(modifiers)));
        }
        Tag::Compound(tag)
    }

    /// Codec errors can still carry a partial Packed value. TagValueInput#read
    /// reports those errors and applies the partial result, including valid
    /// modifiers from an otherwise malformed list.
    fn decode(tag: &Tag, errors: &mut Vec<AttributeLoadError>) -> Option<Self> {
        let Some(tag) = tag.as_compound() else {
            errors.push(AttributeLoadError::UnknownAttribute);
            return None;
        };
        let attribute = tag
            .get_string("id")
            .and_then(|name| Identifier::parse(&name.to_string_lossy()).ok())
            .and_then(|name| Attribute::from_name(name.as_str()));
        let Some(attribute) = attribute else {
            errors.push(AttributeLoadError::UnknownAttribute);
            return None;
        };
        let base_value = match tag.get("base") {
            None => 0.0,
            Some(tag) => tag.as_double().unwrap_or_else(|| {
                errors.push(AttributeLoadError::InvalidBase);
                0.0
            }),
        };
        let mut modifiers = Vec::new();
        if let Some(tag) = tag.get("modifiers") {
            if let Some(entries) = collection(tag) {
                for entry in entries.iter() {
                    if let Some(modifier) = decode_modifier(entry) {
                        modifiers.push(Arc::new(modifier));
                    } else {
                        errors.push(AttributeLoadError::InvalidModifier);
                    }
                }
            } else {
                errors.push(AttributeLoadError::InvalidModifiers);
            }
        }
        Some(Self {
            attribute,
            base_value,
            modifiers,
        })
    }
}
fn decode_modifier(tag: &Tag) -> Option<AttributeModifier> {
    let tag = tag.as_compound()?;
    let id = Identifier::parse(&tag.get_string("id")?.to_string_lossy()).ok()?;
    let amount = tag.get_double("amount")?;
    let operation = match tag.get_string("operation")?.to_string_lossy().as_str() {
        "add_value" => AttributeOperation::AddValue,
        "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
        _ => return None,
    };
    Some(AttributeModifier {
        id,
        amount: amount.into(),
        operation,
    })
}
fn collection(tag: &Tag) -> Option<Cow<'_, [Tag]>> {
    Some(match tag {
        Tag::List(list) => Cow::Borrowed(&list.0),
        Tag::ByteArray(values) => {
            Cow::Owned(values.iter().map(|value| Tag::Byte(*value)).collect())
        }
        Tag::IntArray(values) => Cow::Owned(values.iter().map(|value| Tag::Int(*value)).collect()),
        Tag::LongArray(values) => {
            Cow::Owned(values.iter().map(|value| Tag::Long(*value)).collect())
        }
        _ => return None,
    })
}
impl AttributeMap {
    pub fn save_nbt(&self) -> Tag {
        Tag::List(ListTag(
            self.pack().iter().map(PackedAttribute::to_nbt).collect(),
        ))
    }
    pub fn load_nbt(&mut self, tag: &Tag) -> Vec<AttributeLoadError> {
        let Some(entries) = collection(tag) else {
            return vec![AttributeLoadError::NotAList];
        };
        let mut errors = Vec::new();
        let packed: Vec<_> = entries
            .iter()
            .filter_map(|tag| PackedAttribute::decode(tag, &mut errors))
            .collect();
        self.apply(&packed);
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_codec_retains_native_partial_values_and_permanent_modifiers() {
        let inputs = [
            "{id:'minecraft:max_health'}",
            "{id:'minecraft:max_health',base:'bad'}",
            "{id:'minecraft:max_health',base:30.0,modifiers:[{id:'minecraft:yes',amount:2.0,operation:'add_value'},{id:'minecraft:no',operation:'add_value'}]}",
            "{id:'minecraft:max_health',modifiers:'bad'}",
            "{id:'minecraft:unknown',base:30.0}",
        ];
        let mut rows = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let tag = ferraloom_nbt::parse(input).unwrap();
            let mut errors = Vec::new();
            let packed = PackedAttribute::decode(&tag, &mut errors);
            rows.push(format!(
                "attribute-nbt\t{index}\t{}\t{}",
                errors.is_empty(),
                packed
                    .map(|packed| format!(
                        "{:x}:{}",
                        packed.base_value.to_bits(),
                        packed
                            .modifiers
                            .iter()
                            .map(|modifier| modifier.id.path())
                            .collect::<Vec<_>>()
                            .join(",")
                    ))
                    .unwrap_or_else(|| "none".into())
            ));
        }
        let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("attribute-nbt\t"))
            .collect();
        assert_eq!(rows, expected);
        let mut source = AttributeMap::new(ferraloom_data::EntityType::PLAYER);
        let mut health = source.get_instance(Attribute::MAX_HEALTH).unwrap();
        health.set_base_value(30.0);
        health
            .add_permanent_modifier(Arc::new(AttributeModifier {
                id: Identifier::parse("permanent").unwrap(),
                amount: 2.0.into(),
                operation: AttributeOperation::AddValue,
            }))
            .unwrap();
        health
            .add_transient_modifier(Arc::new(AttributeModifier {
                id: Identifier::parse("temporary").unwrap(),
                amount: 4.0.into(),
                operation: AttributeOperation::AddValue,
            }))
            .unwrap();
        let saved = source.save_nbt();
        let mut loaded = AttributeMap::new(ferraloom_data::EntityType::PLAYER);
        assert!(loaded.load_nbt(&saved).is_empty());
        assert_eq!(loaded.value(Attribute::MAX_HEALTH).unwrap(), 32.0);
        assert_eq!(loaded.save_nbt(), saved);
    }
}
