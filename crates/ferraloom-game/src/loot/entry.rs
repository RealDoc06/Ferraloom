use super::table::Output;
use super::{
    EvalContext, Holder, HolderSet, Identifier, LootData, LootItemCondition, LootItemFunction,
    LootTable, Result, codec,
};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::HolderSet as ItemSet,
    component::Persistent,
    component::{DataComponentPatch, ItemStack},
};
use ferraloom_util::mth;
use serde_json::Value;

/// Port of LootPoolEntryContainer and its uniform/composite subclasses.
#[derive(Clone, Debug, PartialEq)]
pub struct LootPoolEntryContainer {
    pub condition: Option<Holder<LootItemCondition>>,
    pub modifier: Option<Holder<LootItemFunction>>,
    pub weight: i32,
    pub quality: i32,
    pub kind: EntryKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum EntryKind {
    Empty,
    Item(Item),
    Tag {
        items: ItemSet,
        expand: bool,
    },
    Table {
        tables: HolderSet<LootTable>,
        expand: bool,
    },
    Dynamic(Identifier),
    Alternatives(Vec<LootPoolEntryContainer>),
    Sequence(Vec<LootPoolEntryContainer>),
    Group(Vec<LootPoolEntryContainer>),
}
#[derive(Clone, Debug)]
pub(crate) enum ExpandedKind {
    Empty,
    Items(Vec<Item>),
    Tables(Vec<Holder<LootTable>>),
    Dynamic(Identifier),
}
pub(crate) struct ExpandedEntry {
    pub kind: ExpandedKind,
    pub weight: i32,
    pub modifiers: Vec<Holder<LootItemFunction>>,
}
impl LootPoolEntryContainer {
    pub fn decode(v: &Value) -> Result<Self> {
        let kind = codec::kind(v)?;
        let expand = || -> Result<bool> {
            Ok(codec::optional(v, "expand", codec::boolean)?.unwrap_or(false))
        };
        let children = || -> Result<Vec<Self>> {
            Ok(
                codec::optional(v, "children", |v| codec::list(v, Self::decode))?
                    .unwrap_or_default(),
            )
        };
        let entry = match kind.as_str() {
            "minecraft:empty" => EntryKind::Empty,
            "minecraft:item" => {
                let id = codec::id(codec::field(v, "name")?)?;
                EntryKind::Item(
                    Item::from_name(id.as_str())
                        .ok_or_else(|| codec::error(format!("unknown loot item {id}")))?,
                )
            }
            "minecraft:tag" => EntryKind::Tag {
                items: ItemSet::from_json(codec::field(v, "items")?)?,
                expand: expand()?,
            },
            "minecraft:loot_table" => EntryKind::Table {
                tables: HolderSet::decode(codec::field(v, "value")?, LootTable::decode)?,
                expand: expand()?,
            },
            "minecraft:dynamic" => EntryKind::Dynamic(codec::id(codec::field(v, "name")?)?),
            "minecraft:alternatives" => EntryKind::Alternatives(children()?),
            "minecraft:sequence" => EntryKind::Sequence(children()?),
            "minecraft:group" => EntryKind::Group(children()?),
            "minecraft:slots" => return Err(super::LootError::Unsupported("slot_source".into())),
            _ => return Err(codec::error(format!("unknown entry type {kind}"))),
        };
        let composite = matches!(
            entry,
            EntryKind::Alternatives(_) | EntryKind::Sequence(_) | EntryKind::Group(_)
        );
        Ok(Self {
            condition: codec::optional(v, "condition", |v| {
                Holder::decode(v, LootItemCondition::decode)
            })?,
            modifier: codec::optional(v, "modifier", |v| {
                Holder::decode(v, LootItemFunction::decode)
            })?,
            weight: if composite {
                1
            } else {
                codec::optional(v, "weight", codec::int)?.unwrap_or(1)
            },
            quality: if composite {
                0
            } else {
                codec::optional(v, "quality", codec::int)?.unwrap_or(0)
            },
            kind: entry,
        })
    }
    pub(crate) fn expand(
        &self,
        data: &LootData,
        c: &mut EvalContext<'_>,
        output: &mut Vec<ExpandedEntry>,
    ) -> Result<bool> {
        if let Some(condition) = &self.condition {
            if !condition.test(data, c)? {
                return Ok(false);
            }
        }
        let start = output.len();
        let weight = mth::floor(f64::from(
            self.weight as f32 + self.quality as f32 * c.params.luck,
        ))
        .max(0);
        let entry = |kind| ExpandedEntry {
            kind,
            weight,
            modifiers: Vec::new(),
        };
        let success = match &self.kind {
            EntryKind::Empty => {
                output.push(entry(ExpandedKind::Empty));
                true
            }
            EntryKind::Item(item) => {
                output.push(entry(ExpandedKind::Items(vec![*item])));
                true
            }
            EntryKind::Dynamic(id) => {
                output.push(entry(ExpandedKind::Dynamic(id.clone())));
                true
            }
            EntryKind::Tag { items, expand } => {
                let items = match items {
                    ItemSet::Direct(ids) => ids
                        .iter()
                        .map(|id| {
                            Item::from_name(id.as_str())
                                .ok_or_else(|| codec::error(format!("unknown loot item {id}")))
                        })
                        .collect::<Result<Vec<_>>>()?,
                    ItemSet::Named(id) => {
                        let tags = c.facts.item_tags()?;
                        let key = id.to_string().parse().map_err(super::LootError::from)?;
                        let tag = tags.resolve_name(&key).ok_or_else(|| {
                            super::LootError::MissingReference {
                                registry: "item tag",
                                id: id.clone(),
                            }
                        })?;
                        tags.values(tag).to_vec()
                    }
                };
                if *expand {
                    for item in items {
                        output.push(entry(ExpandedKind::Items(vec![item])));
                    }
                } else {
                    output.push(entry(ExpandedKind::Items(items)));
                }
                true
            }
            EntryKind::Table { tables, expand } => {
                let tables = tables.resolve(data)?;
                if *expand {
                    for table in tables {
                        output.push(entry(ExpandedKind::Tables(vec![table])));
                    }
                } else {
                    output.push(entry(ExpandedKind::Tables(tables)));
                }
                true
            }
            EntryKind::Alternatives(children) => {
                let mut success = false;
                for child in children {
                    if child.expand(data, c, output)? {
                        success = true;
                        break;
                    }
                }
                success
            }
            EntryKind::Sequence(children) => {
                let mut success = true;
                for child in children {
                    if !child.expand(data, c, output)? {
                        success = false;
                        break;
                    }
                }
                success
            }
            EntryKind::Group(children) if children.len() == 1 => {
                children[0].expand(data, c, output)?
            }
            EntryKind::Group(children) => {
                for child in children {
                    child.expand(data, c, output)?;
                }
                true
            }
        };
        if let Some(modifier) = &self.modifier {
            for item in &mut output[start..] {
                item.modifiers.push(modifier.clone());
            }
        }
        Ok(success)
    }
}
impl ExpandedEntry {
    pub(crate) fn create(
        &self,
        data: &LootData,
        c: &mut EvalContext<'_>,
        output: &mut Output<'_>,
    ) -> Result<()> {
        let mut decorated = |mut stack: ItemStack, c: &mut EvalContext<'_>| {
            for modifier in &self.modifiers {
                stack = modifier.apply(stack, data, c)?;
            }
            output(stack, c)
        };
        match &self.kind {
            ExpandedKind::Empty => {}
            ExpandedKind::Items(items) => {
                for item in items {
                    decorated(ItemStack::new(*item, 1, DataComponentPatch::default()), c)?;
                }
            }
            ExpandedKind::Tables(tables) => {
                for table in tables {
                    table.raw(data, c, &mut decorated)?;
                }
            }
            ExpandedKind::Dynamic(id) => {
                for stack in c.facts.dynamic_drops(id)? {
                    decorated(stack, c)?;
                }
            }
        }
        Ok(())
    }
}
