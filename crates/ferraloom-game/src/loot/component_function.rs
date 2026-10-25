use super::{
    ContextParam, EntityTarget, EvalContext, Identifier, LootData, LootError, NbtSource, Result,
    codec,
};
use crate::loot::codec::{DecodeInput, PersistentInput};
use ferraloom_data::Block;
use ferraloom_protocol::{
    chat::Component,
    codec::{Holder, HolderSet},
    component::*,
};
use ferraloom_util::random::RandomSource;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ComponentSource {
    Tool,
    BlockEntity,
    Entity(EntityTarget),
}
impl ComponentSource {
    fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(match codec::string(v)? {
            "tool" => Self::Tool,
            "block_entity" => Self::BlockEntity,
            _ => Self::Entity(EntityTarget::decode(v)?),
        })
    }
    pub(crate) fn param(self) -> ContextParam {
        match self {
            Self::Tool => ContextParam::Tool,
            Self::BlockEntity => ContextParam::BlockEntity,
            Self::Entity(target) => target.param(),
        }
    }
}
/// Ports of component-preserving loot functions. Live resolution is isolated in
/// typed facts; component maps and persistent values use the protocol codecs.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentFunction {
    CopyComponents {
        source: ComponentSource,
        include: Option<Vec<ComponentTypeId>>,
        exclude: Option<Vec<ComponentTypeId>>,
    },
    CopyName(NbtSource),
    CopyState {
        block: Block,
        properties: Vec<String>,
    },
    SetName {
        name: Option<Box<Component>>,
        entity: Option<EntityTarget>,
        item_name: bool,
    },
    SetLootTable {
        table: Identifier,
        seed: i64,
    },
    SetInstrument(HolderSet),
    SetRandomPotion(Option<HolderSet>),
    SetBannerPattern {
        patterns: Vec<BannerLayer>,
        append: bool,
    },
    ToggleTooltips(Vec<(ComponentTypeId, bool)>),
}
impl ComponentFunction {
    pub(crate) fn decode<I: DecodeInput>(kind: &str, v: &I) -> Result<Option<Self>> {
        let types = |v: &I| {
            codec::list(v, |v| {
                let id = codec::id(v)?;
                ComponentTypeId::from_name(id.as_str())
                    .ok_or_else(|| codec::error("unknown data component"))
            })
        };
        Ok(Some(match kind {
            "minecraft:copy_components" => Self::CopyComponents {
                source: ComponentSource::decode(codec::field(v, "source")?)?,
                include: codec::optional(v, "include", types)?,
                exclude: codec::optional(v, "exclude", types)?,
            },
            "minecraft:copy_name" => Self::CopyName(NbtSource::decode(codec::field(v, "source")?)?),
            "minecraft:copy_state" => {
                let id = codec::id(codec::field(v, "block")?)?;
                let block =
                    Block::from_name(id.as_str()).ok_or_else(|| codec::error("unknown block"))?;
                let properties = codec::list(codec::field(v, "properties")?, |v| {
                    Ok(codec::string(v)?.to_owned())
                })?
                .into_iter()
                .filter(|name| block.default_state().properties().any(|(n, _)| n == name))
                .collect();
                Self::CopyState { block, properties }
            }
            "minecraft:set_name" => Self::SetName {
                name: codec::optional(v, "name", |v| {
                    Ok(Box::new(<Component as PersistentInput>::from_input(v)?))
                })?,
                entity: codec::optional(v, "entity", EntityTarget::decode)?,
                item_name: match codec::optional(v, "target", codec::string)?
                    .unwrap_or("custom_name")
                {
                    "custom_name" => false,
                    "item_name" => true,
                    _ => return Err(codec::error("invalid name target")),
                },
            },
            "minecraft:set_loot_table" => Self::SetLootTable {
                table: codec::id(codec::field(v, "loot_table_id")?)?,
                seed: codec::optional(v, "seed", codec::long)?.unwrap_or(0),
            },
            "minecraft:set_instrument" => {
                Self::SetInstrument(HolderSet::from_input(codec::field(v, "options")?)?)
            }
            "minecraft:set_random_potion" => {
                Self::SetRandomPotion(codec::optional(v, "options", |v| {
                    Ok(HolderSet::from_input(v)?)
                })?)
            }
            "minecraft:set_banner_pattern" => Self::SetBannerPattern {
                patterns: Vec::<BannerLayer>::from_input(codec::field(v, "patterns")?)?,
                append: codec::boolean(codec::field(v, "append")?)?,
            },
            "minecraft:toggle_tooltips" => Self::ToggleTooltips(
                codec::map(codec::field(v, "toggles")?)?
                    .entries()?
                    .into_iter()
                    .map(|(key, value)| {
                        let key = Identifier::parse(key)?;
                        Ok((
                            ComponentTypeId::from_name(key.as_str())
                                .ok_or_else(|| codec::error("unknown component"))?,
                            codec::boolean(value)?,
                        ))
                    })
                    .collect::<Result<_>>()?,
            ),
            _ => return Ok(None),
        }))
    }
    pub(crate) fn params(&self) -> Vec<ContextParam> {
        match self {
            Self::CopyComponents { source, .. } => vec![source.param()],
            Self::CopyName(source) => vec![source.param()],
            Self::CopyState { .. } => vec![ContextParam::BlockState],
            Self::SetName {
                entity: Some(target),
                ..
            } => vec![target.param()],
            _ => Vec::new(),
        }
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        _data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        let set = |stack: &mut ItemStack, value: DataComponent| {
            if let Some(s) = &mut stack.0 {
                s.components.set(value);
            }
        };
        match self {
            Self::CopyComponents {
                source,
                include,
                exclude,
            } => {
                let map = match source {
                    ComponentSource::Tool => c
                        .params
                        .tool
                        .as_ref()
                        .map(ItemStack::components)
                        .transpose()?,
                    ComponentSource::BlockEntity => {
                        let map = c.runtime.block_entity_components(c.params)?;
                        if map.is_none() && c.params.block_entity.is_some() {
                            return Err(LootError::Unsupported(
                                "loot.block_entity_components".into(),
                            ));
                        }
                        map
                    }
                    ComponentSource::Entity(target) => c
                        .params
                        .entity(*target)
                        .map(|entity| c.facts().entity_components(entity))
                        .transpose()?,
                };
                if let Some(map) = map {
                    for value in map.entries {
                        let id = value.component_type();
                        if include.as_ref().is_none_or(|keys| keys.contains(&id))
                            && exclude.as_ref().is_none_or(|keys| !keys.contains(&id))
                        {
                            set(&mut stack, value);
                        }
                    }
                }
            }
            Self::CopyName(source) => {
                let (name, present) = match source {
                    NbtSource::Entity(target) => {
                        let entity = c.params.entity(*target);
                        (
                            entity
                                .map(|entity| c.facts().entity_custom_name(entity))
                                .transpose()?
                                .flatten(),
                            entity.is_some(),
                        )
                    }
                    NbtSource::BlockEntity => {
                        if c.params.block_entity.is_none()
                            && c.params.block_entity_components.is_none()
                        {
                            (None, false)
                        } else if let Some(name) = c.runtime.block_entity_display_name(c.params)? {
                            (name.custom_name, true)
                        } else {
                            (None, false)
                        }
                    }
                };
                // A present Nameable with a null custom name removes the target.
                if let Some(name) = name {
                    set(&mut stack, DataComponent::CustomName(Box::new(name)));
                } else if present && let Some(s) = &mut stack.0 {
                    s.components.remove(ComponentTypeId::CUSTOM_NAME);
                }
            }
            Self::CopyState { block, properties } => {
                if let Some(state) = c.params.block_state {
                    let mut values = match stack.components()?.get(ComponentTypeId::BLOCK_STATE) {
                        Some(DataComponent::BlockState(value)) => value.0.clone(),
                        _ => Vec::new(),
                    };
                    for name in properties {
                        if block.default_state().property_values(name)
                            != state.property_values(name)
                        {
                            continue;
                        }
                        if let Some((_, value)) = state.properties().find(|(n, _)| *n == name) {
                            if let Some((_, old)) = values.iter_mut().find(|(n, _)| n == name) {
                                *old = value.into();
                            } else {
                                values.push((name.clone(), value.into()));
                            }
                        }
                    }
                    set(
                        &mut stack,
                        DataComponent::BlockState(BlockItemStateProperties(values)),
                    );
                }
            }
            Self::SetName {
                name,
                entity,
                item_name,
            } => {
                if let Some(name) = name {
                    let name =
                        if let Some(entity) = entity.and_then(|target| c.params.entity(target)) {
                            c.facts().resolve_component(name, entity)?
                        } else {
                            name.as_ref().clone()
                        };
                    set(
                        &mut stack,
                        if *item_name {
                            DataComponent::ItemName(Box::new(name))
                        } else {
                            DataComponent::CustomName(Box::new(name))
                        },
                    );
                }
            }
            Self::SetLootTable { table, seed } => {
                if !stack.is_empty() {
                    set(
                        &mut stack,
                        DataComponent::ContainerLoot(SeededContainerLoot {
                            loot_table: table.clone(),
                            seed: *seed,
                        }),
                    );
                }
            }
            Self::SetInstrument(options) => {
                let ids =
                    resolve_options(options, crate::predicates::PredicateRegistry::Instrument, c)?;
                if !ids.is_empty() {
                    let id = ids[c.random().next_int_bound(ids.len() as i32) as usize].clone();
                    set(
                        &mut stack,
                        DataComponent::Instrument(InstrumentHolder(Holder::Reference(id))),
                    );
                }
            }
            Self::SetRandomPotion(options) => {
                let registry = crate::predicates::PredicateRegistry::Potion;
                let ids = if let Some(options) = options {
                    resolve_options(options, registry, c)?
                } else {
                    c.facts().registry_values(registry)?
                };
                if !ids.is_empty() {
                    let id = ids[c.random().next_int_bound(ids.len() as i32) as usize].clone();
                    let mut value = match stack.components()?.get(ComponentTypeId::POTION_CONTENTS)
                    {
                        Some(DataComponent::PotionContents(value)) => value.clone(),
                        _ => PotionContents {
                            potion: None,
                            custom_color: None,
                            custom_effects: Vec::new(),
                            custom_name: None,
                        },
                    };
                    value.potion = Some(Potion(id));
                    set(&mut stack, DataComponent::PotionContents(value));
                }
            }
            Self::SetBannerPattern { patterns, append } => {
                let mut value = if *append {
                    match stack.components()?.get(ComponentTypeId::BANNER_PATTERNS) {
                        Some(DataComponent::BannerPatterns(value)) => value.clone(),
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                };
                value.extend(patterns.clone());
                set(&mut stack, DataComponent::BannerPatterns(value));
            }
            Self::ToggleTooltips(toggles) => {
                let mut value = match stack.components()?.get(ComponentTypeId::TOOLTIP_DISPLAY) {
                    Some(DataComponent::TooltipDisplay(value)) => value.clone(),
                    _ => TooltipDisplay {
                        hide_tooltip: false,
                        hidden_components: ComponentTypeList {
                            entries: Vec::new(),
                        },
                    },
                };
                for (key, shown) in toggles {
                    if *shown {
                        value.hidden_components.entries.retain(|v| v != key);
                    } else if !value.hidden_components.entries.contains(key) {
                        value.hidden_components.entries.push(*key);
                    }
                }
                set(&mut stack, DataComponent::TooltipDisplay(value));
            }
        }
        Ok(stack)
    }
}
fn resolve_options(
    options: &HolderSet,
    registry: crate::predicates::PredicateRegistry,
    c: &EvalContext<'_>,
) -> Result<Vec<Identifier>> {
    match options {
        HolderSet::Direct(ids) => Ok(ids.clone()),
        HolderSet::Named(tag) => c.facts().registry_tag_values(registry, tag),
    }
}
