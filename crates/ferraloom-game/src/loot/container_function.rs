use super::function::{max_stack_size, stack_count};
use super::{
    EvalContext, Holder, LootData, LootItemFunction, LootPoolEntryContainer, Result, codec,
};
use crate::loot::codec::DecodeInput;
use ferraloom_protocol::component::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContainerComponent {
    Container,
    BundleContents,
    ChargedProjectiles,
}
impl ContainerComponent {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(match codec::id(v)?.as_str() {
            "minecraft:container" => Self::Container,
            "minecraft:bundle_contents" => Self::BundleContents,
            "minecraft:charged_projectiles" => Self::ChargedProjectiles,
            _ => return Err(codec::error("no items in component")),
        })
    }
    pub(crate) fn id(self) -> ComponentTypeId {
        match self {
            Self::Container => ComponentTypeId::CONTAINER,
            Self::BundleContents => ComponentTypeId::BUNDLE_CONTENTS,
            Self::ChargedProjectiles => ComponentTypeId::CHARGED_PROJECTILES,
        }
    }
}
/// Port of SetContainerContents and ModifyContainerContents. Generation uses
/// streaming expansion: emitted modifiers run before the next child expands.
#[derive(Clone, Debug, PartialEq)]
pub enum ContainerFunction {
    Set {
        component: ContainerComponent,
        entries: Vec<LootPoolEntryContainer>,
    },
    Modify {
        component: ContainerComponent,
        modifier: Holder<LootItemFunction>,
    },
}
impl ContainerFunction {
    pub(crate) fn decode<I: DecodeInput>(kind: &str, v: &I) -> Result<Option<Self>> {
        Ok(Some(match kind {
            "minecraft:set_contents" => Self::Set {
                component: ContainerComponent::decode(codec::field(v, "component")?)?,
                entries: codec::list(codec::field(v, "entries")?, LootPoolEntryContainer::decode)?,
            },
            "minecraft:modify_contents" => Self::Modify {
                component: ContainerComponent::decode(codec::field(v, "component")?)?,
                modifier: Holder::decode(codec::field(v, "modifier")?, LootItemFunction::decode)?,
            },
            _ => return Ok(None),
        }))
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        if stack.is_empty() {
            return Ok(stack);
        }
        let (component, items) = match self {
            Self::Set { component, entries } => {
                let mut items = Vec::new();
                for entry in entries {
                    entry.expand_with(data, c, &mut |entry, c| {
                        entry.create(data, c, &mut |stack, c| {
                            if c.facts().item_enabled(&stack)? {
                                super::table::split_stack(stack, &mut items)?;
                            }
                            Ok(())
                        })
                    })?;
                }
                (*component, items)
            }
            Self::Modify {
                component,
                modifier,
            } => {
                let map = stack.components()?;
                let Some(value) = map.get(component.id()) else {
                    return Ok(stack);
                };
                let mut items = match value {
                    DataComponent::Container(value) => value
                        .0
                        .iter()
                        .map(|s| s.as_ref().map_or_else(ItemStack::default, template_stack))
                        .collect::<Vec<_>>(),
                    DataComponent::BundleContents(value) => {
                        value.iter().map(template_stack).collect()
                    }
                    DataComponent::ChargedProjectiles(value) => {
                        value.0.iter().map(template_stack).collect()
                    }
                    _ => unreachable!("container component identity"),
                };
                for item in &mut items {
                    if !item.is_empty() {
                        *item = modifier.apply(std::mem::take(item), data, c)?;
                        let max = max_stack_size(item)?;
                        if let Some(s) = &mut item.0 {
                            s.count = s.count.min(max);
                        }
                    }
                }
                (*component, items)
            }
        };
        let value = match component {
            ContainerComponent::Container => {
                let mut values = items.iter().map(template).collect::<Vec<_>>();
                while values.last().is_some_and(Option::is_none) {
                    values.pop();
                }
                if values.len() > 256 {
                    return Err(codec::error("container exceeds 256 slots"));
                }
                DataComponent::Container(BoundedList(values))
            }
            ContainerComponent::ChargedProjectiles => DataComponent::ChargedProjectiles(
                BoundedList(items.iter().filter_map(template).collect()),
            ),
            ContainerComponent::BundleContents => {
                DataComponent::BundleContents(c.facts().bundle_contents(items)?)
            }
        };
        stack.0.as_mut().expect("nonempty").components.set(value);
        Ok(stack)
    }
}
fn template(stack: &ItemStack) -> Option<ItemStackTemplate> {
    if stack.is_empty() {
        None
    } else {
        let s = stack.0.as_ref().expect("nonempty");
        Some(ItemStackTemplate {
            item: s.item,
            count: stack_count(stack),
            components: s.components.clone(),
        })
    }
}
fn template_stack(value: &ItemStackTemplate) -> ItemStack {
    ItemStack::new(value.item, value.count, value.components.clone())
}
