//! Port of ComponentUtils#resolve and ResolutionContext (26.3's updateForEntity).
mod block_data;
pub use block_data::block_data;
mod nbt_text;
use super::{
    CommandSourceStack,
    arguments::selectors::{EntityQuery, EntitySelector},
    exceptions,
};
use crate::entity::EntityRef;
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_nbt::{CompoundTag, NbtPath, Tag};
use ferraloom_protocol::chat::{
    Argument, Component, Contents, DataSource, HoverEvent, ObjectContents,
};
use std::fmt;

/// Required live reads must be supplied by the canonical server adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedContent {
    Score,
    NbtData,
}
#[derive(Debug)]
pub enum ResolutionError {
    Syntax(CommandSyntaxException),
    Unsupported(UnsupportedContent),
    Storage(super::storage::StorageReadError),
    EntityData(crate::level::LevelError),
    BlockData(crate::level::LevelError),
    Serialization(ferraloom_protocol::Error),
    InvalidContent(String),
}
impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => write!(f, "{error}"),
            Self::Unsupported(content) => {
                write!(f, "Component {content:?} resolution is not yet supported")
            }
            Self::Storage(error) => write!(f, "{error}"),
            Self::EntityData(error) => write!(f, "Required entity read failed: {error:?}"),
            Self::BlockData(error) => write!(f, "Required block entity read failed: {error:?}"),
            Self::Serialization(error) => write!(f, "Required NBT serialization failed: {error}"),
            Self::InvalidContent(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for ResolutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}
impl From<super::storage::StorageReadError> for ResolutionError {
    fn from(error: super::storage::StorageReadError) -> Self {
        Self::Storage(error)
    }
}
impl From<CommandSyntaxException> for ResolutionError {
    fn from(error: CommandSyntaxException) -> Self {
        let raw = error.raw_message.as_ref() as &dyn std::any::Any;
        if let Some(required) = raw.downcast_ref::<super::arguments::selectors::EntityReadError>() {
            Self::EntityData(required.error)
        } else {
            Self::Syntax(error)
        }
    }
}
/// Reads occur synchronously at the component's native resolution point.
/// Missing objectives/scores are empty components; missing services are Unsupported.
pub trait ComponentResolver: EntityQuery {
    fn formatted_score(
        &mut self,
        _holder: &str,
        _objective: &str,
    ) -> Result<Component, ResolutionError> {
        Err(ResolutionError::Unsupported(UnsupportedContent::Score))
    }
    /// Block: loaded block entity's full metadata; entity: selector order and
    /// NbtPredicate#getEntityTagToCompare; storage: one canonical stored compound.
    fn nbt_data(
        &mut self,
        _source: &CommandSourceStack,
        _data: &DataSource,
    ) -> Result<Vec<CompoundTag>, ResolutionError> {
        Err(ResolutionError::Unsupported(UnsupportedContent::NbtData))
    }
    fn invalid_interpreted_component(&mut self, tag: &Tag, error: &str) {
        eprintln!("Failed to parse component: {tag}: {error}");
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitBehavior {
    DiscardRemaining,
    StopProcessingAndCopyRemaining,
}
/// Port of ResolutionContext. One context shares the NBT expansion budget across
/// siblings, translations, hover text, separators and interpreted components.
pub struct ResolutionContext<'a> {
    pub source: Option<&'a CommandSourceStack>,
    pub default_scoreboard_entity: Option<EntityRef>,
    pub object_info_validator: &'a dyn Fn(&ObjectContents) -> bool,
    pub depth_limit: i32,
    pub depth_limit_behavior: LimitBehavior,
    pub resolved_component_limit: i32,
    resolved_component_count: i32,
}
impl<'a> ResolutionContext<'a> {
    pub fn new(source: Option<&'a CommandSourceStack>) -> Self {
        Self {
            source,
            default_scoreboard_entity: source.and_then(|source| source.entity),
            object_info_validator: &|_| true,
            depth_limit: 100,
            depth_limit_behavior: LimitBehavior::StopProcessingAndCopyRemaining,
            resolved_component_limit: 65536,
            resolved_component_count: 0,
        }
    }
    pub fn resolved_component_count(&self) -> i32 {
        self.resolved_component_count
    }
}
/// Port of ComponentUtils#resolve(context, component), formerly updateForEntity.
pub fn resolve<R: ComponentResolver>(
    context: &mut ResolutionContext<'_>,
    component: &Component,
    resolver: &mut R,
) -> Result<Component, ResolutionError> {
    resolve_at_depth(context, component, resolver, 0)
}
pub fn resolve_at_depth<R: ComponentResolver>(
    context: &mut ResolutionContext<'_>,
    component: &Component,
    resolver: &mut R,
    depth: i32,
) -> Result<Component, ResolutionError> {
    if depth > context.depth_limit {
        return Ok(match context.depth_limit_behavior {
            LimitBehavior::DiscardRemaining => Component::literal("..."),
            LimitBehavior::StopProcessingAndCopyRemaining => component.clone(),
        });
    }
    let mut result = resolve_contents(
        context,
        &component.contents,
        resolver,
        depth.wrapping_add(1),
    )?;
    for sibling in &component.extra {
        result.extra.push(resolve_at_depth(
            context,
            sibling,
            resolver,
            depth.wrapping_add(1),
        )?);
    }
    let mut style = component.style.clone();
    if let Some(HoverEvent::ShowText(text)) = &style.hover_event {
        style.hover_event = Some(HoverEvent::ShowText(Box::new(resolve_at_depth(
            context,
            text,
            resolver,
            depth.wrapping_add(1),
        )?)));
    }
    result.style = style.apply_to(&result.style);
    Ok(result)
}
fn optional<R: ComponentResolver>(
    context: &mut ResolutionContext<'_>,
    component: Option<&Component>,
    resolver: &mut R,
    depth: i32,
) -> Result<Option<Component>, ResolutionError> {
    component
        .map(|component| resolve_at_depth(context, component, resolver, depth))
        .transpose()
}
fn resolve_contents<R: ComponentResolver>(
    context: &mut ResolutionContext<'_>,
    contents: &Contents,
    resolver: &mut R,
    depth: i32,
) -> Result<Component, ResolutionError> {
    Ok(match contents {
        Contents::Text(_) | Contents::Keybind(_) => Component::new(contents.clone()),
        Contents::Translatable {
            key,
            fallback,
            args,
        } => {
            let args = args
                .iter()
                .map(|arg| {
                    Ok(match arg {
                        Argument::Component(component) => Argument::Component(Box::new(
                            resolve_at_depth(context, component, resolver, depth)?,
                        )),
                        _ => arg.clone(),
                    })
                })
                .collect::<Result<_, ResolutionError>>()?;
            Component::new(Contents::Translatable {
                key: key.clone(),
                fallback: fallback.clone(),
                args,
            })
        }
        Contents::Selector { pattern, separator } => {
            let Some(source) = context.source else {
                return Ok(Component::default());
            };
            let separator = optional(context, separator.as_deref(), resolver, depth)?
                .unwrap_or_else(|| {
                    let mut separator = Component::literal(", ");
                    separator.style.color =
                        Some(ferraloom_protocol::chat::TextColor::parse("gray").unwrap());
                    separator
                });
            let selector = EntitySelector::parse(&mut StringReader::new(pattern), true)?;
            let entities = selector.find(source, resolver, false)?;
            let names = entities
                .into_iter()
                .map(|entity| resolver.display_name(entity))
                .collect::<Result<Vec<_>, _>>()?;
            format_list(names, &separator)
        }
        Contents::Score { name, objective } => {
            let Some(source) = context.source else {
                return Ok(Component::default());
            };
            // Codec.either selects a compiled entity selector only if the whole
            // string parses; otherwise this is a literal scoreboard holder.
            let mut reader = StringReader::new(name);
            let selector = EntitySelector::parse(&mut reader, true)
                .ok()
                .filter(|_| !reader.can_read());
            let mut holder = name.clone();
            if let Some(selector) = selector {
                let entities = selector.find(source, resolver, false)?;
                if entities.len() > 1 {
                    return Err(exceptions::error("argument.entity.toomany", &[]).into());
                }
                if let Some(entity) = entities.first() {
                    holder = scoreboard_name(resolver, *entity)?;
                }
            }
            if holder == "*"
                && let Some(entity) = context.default_scoreboard_entity
            {
                holder = scoreboard_name(resolver, entity)?;
            }
            resolver.formatted_score(&holder, objective)?
        }
        Contents::Nbt {
            path,
            interpret,
            plain,
            separator,
            source: data,
        } => {
            let Some(source) = context.source else {
                return Ok(Component::default());
            };
            if context.resolved_component_count > context.resolved_component_limit {
                return Ok(Component::default());
            }
            context.resolved_component_count = context.resolved_component_count.wrapping_add(1);
            let documents = resolver.nbt_data(source, data)?;
            let separator = optional(context, separator.as_deref(), resolver, depth)?
                .unwrap_or_else(|| Component::literal(", "));
            let path = NbtPath::parse(path)
                .map_err(|error| ResolutionError::InvalidContent(error.to_string()))?;
            let mut values = Vec::new();
            for document in documents {
                // Missing path elements are suppressed by NbtContents, including
                // a missing match in one document while later documents match.
                for tag in path.get(&Tag::Compound(document)).unwrap_or_default() {
                    if *interpret {
                        match Component::from_nbt(tag.clone()) {
                            Ok(component) => {
                                match resolve_at_depth(context, &component, resolver, depth) {
                                    Ok(component) => values.push(component),
                                    Err(
                                        error @ (ResolutionError::Unsupported(_)
                                        | ResolutionError::Storage(_)
                                        | ResolutionError::EntityData(_)
                                        | ResolutionError::BlockData(_)
                                        | ResolutionError::Serialization(_)),
                                    ) => {
                                        return Err(error);
                                    }
                                    Err(error) => resolver
                                        .invalid_interpreted_component(&tag, &error.to_string()),
                                }
                            }
                            Err(error) => {
                                resolver.invalid_interpreted_component(&tag, &error.to_string())
                            }
                        }
                    } else {
                        values.push(nbt_text::render(&tag, *plain));
                    }
                }
            }
            // Native NBT reduction appends to the first component directly;
            // selector formatList uses an empty root for multiple results.
            let mut values = values.into_iter();
            let mut result = values.next().unwrap_or_default();
            for value in values {
                result.extra.push(separator.clone());
                result.extra.push(value);
            }
            result
        }
        Contents::Object { object, fallback } => {
            let fallback = optional(context, fallback.as_deref(), resolver, depth)?;
            if (context.object_info_validator)(object) {
                Component::new(Contents::Object {
                    object: object.clone(),
                    fallback: fallback.map(Box::new),
                })
            } else {
                fallback.unwrap_or_else(|| Component::literal(object_fallback(object)))
            }
        }
    })
}
fn scoreboard_name<R: EntityQuery>(
    resolver: &R,
    entity: EntityRef,
) -> Result<String, ResolutionError> {
    let info = resolver
        .info(entity)
        .ok_or_else(|| exceptions::error("argument.entity.notfound.entity", &[]))?;
    Ok(if info.player {
        info.name.into_owned()
    } else {
        info.uuid.to_string()
    })
}

fn format_list(values: Vec<Component>, separator: &Component) -> Component {
    if values.len() == 1 {
        return values.into_iter().next().unwrap();
    }
    let mut result = Component::default();
    for value in values {
        if !result.extra.is_empty() {
            result.extra.push(separator.clone());
        }
        result.extra.push(value);
    }
    result
}
fn object_fallback(object: &ObjectContents) -> String {
    fn short(id: &str) -> &str {
        id.strip_prefix("minecraft:").unwrap_or(id)
    }
    match object {
        ObjectContents::Atlas { atlas, sprite } => {
            if atlas.as_str() == "minecraft:blocks" {
                format!("[{}]", short(sprite.as_str()))
            } else {
                format!("[{}@{}]", short(sprite.as_str()), short(atlas.as_str()))
            }
        }
        ObjectContents::Player { profile, .. } => profile.name.as_ref().map_or_else(
            || "[unknown player head]".into(),
            |name| format!("[{name} head]"),
        ),
    }
}

#[cfg(test)]
mod tests;
