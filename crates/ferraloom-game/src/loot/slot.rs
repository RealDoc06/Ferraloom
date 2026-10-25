use super::{
    ContainerComponent, ContextParam, EntityTarget, EvalContext, Holder, HolderSet, LootData,
    Result, codec,
};
use crate::loot::codec::DecodeInput;
use crate::predicates::ItemPredicate;
use ferraloom_protocol::component::*;

/// Port of SlotRanges' fixed codec names and ordered integer slot lists.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotRange {
    pub name: String,
    pub slots: Vec<i32>,
}
impl SlotRange {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let name = codec::string(v)?;
        let named = match name {
            "contents" => Some(vec![0]),
            "weapon" | "weapon.mainhand" => Some(vec![98]),
            "weapon.offhand" => Some(vec![99]),
            "weapon.*" => Some(vec![98, 99]),
            "armor.head" => Some(vec![103]),
            "armor.chest" => Some(vec![102]),
            "armor.legs" => Some(vec![101]),
            "armor.feet" => Some(vec![100]),
            "armor.body" => Some(vec![105]),
            "armor.*" => Some(vec![103, 102, 101, 100, 105]),
            "saddle" => Some(vec![106]),
            "horse.chest" | "player.cursor" => Some(vec![499]),
            _ => None,
        };
        if let Some(slots) = named {
            return Ok(Self {
                name: name.into(),
                slots,
            });
        }
        for (prefix, offset, size) in [
            ("container.", 0, 54),
            ("hotbar.", 0, 9),
            ("inventory.", 9, 27),
            ("enderchest.", 200, 27),
            ("mob.inventory.", 300, 8),
            ("horse.", 500, 15),
            ("player.crafting.", 500, 4),
        ] {
            if let Some(suffix) = name.strip_prefix(prefix) {
                let slots = if suffix == "*" {
                    (offset..offset + size).collect()
                } else if let Ok(index) = suffix.parse::<i32>() {
                    if index < 0 || index >= size || index.to_string() != suffix {
                        return Err(codec::error("unknown slot range"));
                    }
                    vec![offset + index]
                } else {
                    return Err(codec::error("unknown slot range"));
                };
                return Ok(Self {
                    name: name.into(),
                    slots,
                });
            }
        }
        Err(codec::error(format!("unknown slot range {name}")))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotTarget {
    Container,
    BlockEntity,
    Entity(EntityTarget),
}
impl SlotTarget {
    fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(match codec::string(v)? {
            "container" => Self::Container,
            "block_entity" => Self::BlockEntity,
            _ => Self::Entity(EntityTarget::decode(v)?),
        })
    }
    pub fn param(self) -> ContextParam {
        match self {
            Self::Container => ContextParam::Container,
            Self::BlockEntity => ContextParam::BlockEntity,
            Self::Entity(target) => target.param(),
        }
    }
}
/// Read-only loot use of SlotSources. Inventory adapters own slot mutation;
/// these providers retain vanilla's grouping, filtering and range order.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotSource {
    Empty,
    Group(HolderSet<Self>),
    Range {
        source: SlotTarget,
        slots: SlotRange,
    },
    Filtered {
        source: Holder<Self>,
        filter: ItemPredicate,
    },
    Limit {
        source: Holder<Self>,
        limit: usize,
    },
    Contents {
        source: Holder<Self>,
        component: ContainerComponent,
    },
}
impl SlotSource {
    /// Decode through the same schema using NbtOps scalar/component readers.
    pub fn decode_nbt(tag: &ferraloom_nbt::Tag) -> Result<Self> {
        Self::decode(&super::NbtOps::new(tag))
    }

    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_array() || v.is_string() {
            Ok(Self::Group(HolderSet::decode(v, Self::decode_typed)?))
        } else {
            Self::decode_typed(v)
        }
    }
    fn decode_typed<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(match codec::kind(v)?.as_str() {
            "minecraft:empty" => Self::Empty,
            "minecraft:group" => Self::Group(HolderSet::decode(
                codec::field(v, "terms")?,
                Self::decode_typed,
            )?),
            "minecraft:slot_range" => Self::Range {
                source: codec::optional(v, "source", SlotTarget::decode)?
                    .unwrap_or(SlotTarget::Container),
                slots: SlotRange::decode(codec::field(v, "slots")?)?,
            },
            "minecraft:filtered" => Self::Filtered {
                source: Holder::decode(codec::field(v, "slot_source")?, Self::decode)?,
                filter: ItemPredicate::decode(codec::field(v, "item_filter")?)?,
            },
            "minecraft:limit_slots" => {
                let limit = codec::int(codec::field(v, "limit")?)?;
                if limit <= 0 {
                    return Err(codec::error("nonpositive slot limit"));
                }
                Self::Limit {
                    source: Holder::decode(codec::field(v, "slot_source")?, Self::decode)?,
                    limit: limit as usize,
                }
            }
            "minecraft:contents" => Self::Contents {
                source: Holder::decode(codec::field(v, "slot_source")?, Self::decode)?,
                component: ContainerComponent::decode(codec::field(v, "component")?)?,
            },
            _ => return Err(codec::error("unknown slot source")),
        })
    }
    pub(crate) fn items(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<Vec<ItemStack>> {
        match self {
            Self::Empty => Ok(Vec::new()),
            Self::Group(sources) => {
                let mut result = Vec::new();
                for source in sources.resolve(data)? {
                    result.extend(source.items(data, c)?);
                }
                Ok(result)
            }
            Self::Range { source, slots } => match source {
                SlotTarget::Container => Ok(c
                    .params
                    .container
                    .as_ref()
                    .map(|items| {
                        slots
                            .slots
                            .iter()
                            .filter_map(|slot| {
                                usize::try_from(*slot).ok().and_then(|i| items.get(i))
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default()),
                SlotTarget::Entity(target) if c.params.entity(*target).is_none() => Ok(Vec::new()),
                SlotTarget::BlockEntity
                    if c.params.block_entity.is_none()
                        && c.params.block_entity_components.is_none() =>
                {
                    Ok(Vec::new())
                }
                _ => c.facts().slot_items(*source, &slots.slots, c.params),
            },
            Self::Filtered { source, filter } => {
                let mut result = Vec::new();
                for item in source.items(data, c)? {
                    if filter.matches(&item, c.facts())? {
                        result.push(item);
                    }
                }
                Ok(result)
            }
            Self::Limit { source, limit } => {
                let mut items = source.items(data, c)?;
                items.truncate(*limit);
                Ok(items)
            }
            Self::Contents { source, component } => {
                let mut result = Vec::new();
                let id = component.id();
                for item in source.items(data, c)? {
                    if let Some(value) = item.components()?.get(id) {
                        match value {
                            DataComponent::Container(value) => {
                                result.extend(value.0.iter().map(|item| {
                                    item.as_ref().map_or_else(ItemStack::default, template)
                                }))
                            }
                            DataComponent::BundleContents(value) => {
                                result.extend(value.iter().map(template))
                            }
                            DataComponent::ChargedProjectiles(value) => {
                                result.extend(value.0.iter().map(template))
                            }
                            _ => unreachable!("container component"),
                        }
                    }
                }
                Ok(result)
            }
        }
    }
}
fn template(value: &ItemStackTemplate) -> ItemStack {
    ItemStack::new(value.item, value.count, value.components.clone())
}
impl Holder<SlotSource> {
    pub(crate) fn items(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<Vec<ItemStack>> {
        if let Self::Reference(id) = self {
            let key = ("slot_source", id.clone());
            if !c.visited.insert(key.clone()) {
                return Err(codec::error("recursive slot source"));
            }
            let result = self.resolve(data)?.items(data, c);
            c.visited.remove(&key);
            result
        } else {
            self.resolve(data)?.items(data, c)
        }
    }
}
