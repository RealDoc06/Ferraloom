use super::{Error, Result};
use ferraloom_data::{
    Item,
    datapack::{Identifier, TagSet},
};
use ferraloom_protocol::{
    codec::HolderSet,
    component::{ItemSet, ItemStack},
    play::SlotDisplay,
};
use serde_json::Value;

/// Port of Ingredient: a nonempty direct holder list or a named (possibly empty) tag.
/// Components and counts never constrain ingredient membership.
#[derive(Clone, Debug, PartialEq)]
pub struct Ingredient {
    tag: Option<Identifier>,
    items: Vec<Item>,
}
impl Ingredient {
    pub fn of(items: Vec<Item>) -> Result<Self> {
        if items.is_empty() || items.contains(&Item::AIR) {
            return Err(Error::Invalid(
                "ingredient cannot be empty or contain air".into(),
            ));
        }
        Ok(Self { tag: None, items })
    }
    pub fn decode(value: &Value, tags: &TagSet<Item>) -> Result<Self> {
        if let Some(name) = value.as_str().and_then(|name| name.strip_prefix('#')) {
            let name: Identifier = name.parse()?;
            let tag = tags
                .resolve_name(&name)
                .ok_or_else(|| Error::Invalid(format!("unknown item tag #{name}")))?;
            return Ok(Self {
                tag: Some(name),
                items: tags.values(tag).to_vec(),
            });
        }
        let item = |v: &Value| -> Result<Item> {
            let name: Identifier = v
                .as_str()
                .ok_or_else(|| {
                    Error::Invalid("ingredient must be an item, tag or item list".into())
                })?
                .parse()?;
            Item::from_name(&name.to_string())
                .ok_or_else(|| Error::Invalid(format!("unknown item {name}")))
        };
        Self::of(if let Some(values) = value.as_array() {
            values.iter().map(item).collect::<Result<_>>()?
        } else {
            vec![item(value)?]
        })
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn accepts_item(&self, item: Item) -> bool {
        self.items.contains(&item)
    }
    pub fn test(&self, stack: &ItemStack) -> bool {
        stack
            .0
            .as_ref()
            .is_some_and(|stack| self.accepts_item(stack.item))
    }
    pub fn item_set(&self) -> ItemSet {
        ItemSet(match &self.tag {
            Some(tag) => HolderSet::Named(
                ferraloom_protocol::Identifier::parse(&tag.to_string())
                    .expect("validated identifier"),
            ),
            None => HolderSet::Direct(
                self.items
                    .iter()
                    .map(|item| {
                        ferraloom_protocol::Identifier::parse(item.name()).expect("registry name")
                    })
                    .collect(),
            ),
        })
    }
    pub fn display(&self) -> SlotDisplay {
        SlotDisplay::Tag {
            tag: self.item_set(),
        }
    }
    pub fn test_optional(ingredient: Option<&Self>, stack: &ItemStack) -> bool {
        ingredient.map_or_else(|| stack.is_empty(), |ingredient| ingredient.test(stack))
    }
}
