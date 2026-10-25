//! Port of ItemParser and ItemInput, sharing the protocol's persistent component codecs.
use super::{identifier, nbt};
use crate::commands::exceptions::{error, error_at};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::Item;
use ferraloom_protocol::{
    Identifier,
    component::{ComponentTypeId, DataComponent, DataComponentPatch},
};

#[derive(Clone, Debug)]
pub struct ItemInput {
    pub item: Item,
    pub components: DataComponentPatch,
}
impl ItemInput {
    /// Port of ItemInput#createItemStack, including the overstack check before
    /// ItemStack#validateStrict's component and contained-item validation.
    pub fn create_item_stack(
        &self,
        count: i32,
    ) -> Result<ferraloom_protocol::component::ItemStack, CommandSyntaxException> {
        let stack = ferraloom_protocol::component::ItemStack::new(
            self.item,
            count,
            self.components.clone(),
        );
        let maximum = crate::entity::player::inventory::max_stack_size(&stack);
        if count > maximum {
            return Err(crate::commands::exceptions::error_with_arguments(
                "arguments.item.overstacked",
                vec![
                    crate::commands::builtins::string(self.item.name()),
                    crate::commands::builtins::integer(maximum),
                ],
            ));
        }
        crate::item::stack::validate_strict(&stack)
            .map_err(|reason| error("arguments.item.malformed", &[reason.to_string()]))?;
        Ok(stack)
    }
}
pub fn parse(reader: &mut StringReader) -> Result<ItemInput, CommandSyntaxException> {
    let start = reader.cursor();
    let parsed = parse_inner(reader, &mut CompletionMode::Item);
    if parsed.is_err() {
        reader.set_cursor(start);
    }
    parsed
}
#[derive(Clone, Copy)]
enum CompletionMode {
    Item,
    StartComponents,
    ComponentAssignmentOrRemoval,
    Component,
    Assignment,
    NextOrEnd,
    None,
}
fn parse_inner(
    reader: &mut StringReader,
    mode: &mut CompletionMode,
) -> Result<ItemInput, CommandSyntaxException> {
    let start = reader.cursor();
    let name = identifier(reader)?;
    let item = Item::from_name(&name).ok_or_else(|| {
        reader.set_cursor(start);
        error_at(reader, "argument.item.id.invalid", &[name])
    })?;
    let mut components = DataComponentPatch::default();
    *mode = CompletionMode::StartComponents;
    if reader.can_read() && reader.peek() == 91 {
        reader.skip();
        *mode = CompletionMode::ComponentAssignmentOrRemoval;
        let mut visited = Vec::new();
        while reader.can_read() && reader.peek() != 93 {
            reader.skip_whitespace();
            let remove = reader.can_read() && reader.peek() == 33;
            if remove {
                reader.skip();
                *mode = CompletionMode::Component;
            }
            let (id, key) = component_type(reader)?;
            if visited.contains(&id) {
                return Err(error(
                    "arguments.item.component.repeated",
                    &[key.to_string()],
                ));
            }
            visited.push(id);
            if remove {
                components.remove(id);
                *mode = CompletionMode::None;
            } else {
                *mode = CompletionMode::Assignment;
                reader.skip_whitespace();
                reader.expect(61)?;
                *mode = CompletionMode::None;
                reader.skip_whitespace();
                let value_start = reader.cursor();
                let value = nbt(reader, false)?;
                let value = DataComponent::from_nbt(&key, &value).map_err(|failure| {
                    reader.set_cursor(value_start);
                    error_at(
                        reader,
                        "arguments.item.component.malformed",
                        &[key.to_string(), failure.to_string()],
                    )
                })?;
                components.set(value);
            }
            reader.skip_whitespace();
            *mode = CompletionMode::NextOrEnd;
            if !reader.can_read() || reader.peek() != 44 {
                break;
            }
            reader.skip();
            reader.skip_whitespace();
            *mode = CompletionMode::ComponentAssignmentOrRemoval;
            if !reader.can_read() {
                return Err(error_at(reader, "arguments.item.component.expected", &[]));
            }
        }
        reader.expect(93)?;
        *mode = CompletionMode::None;
    }
    Ok(ItemInput { item, components })
}
/// Port of ItemParser#fillSuggestions; keep the parser cursor before its command-error rewind.
pub fn suggestions(
    context: &crate::commands::context::CommandBuildContext,
    builder: ferraloom_brigadier::SuggestionsBuilder,
) -> ferraloom_brigadier::Suggestions {
    let mut reader = StringReader::new(&builder.input);
    reader.set_cursor(builder.start);
    let mut mode = CompletionMode::Item;
    let _ = parse_inner(&mut reader, &mut mode);
    let mut builder = builder.create_offset(reader.cursor());
    match mode {
        CompletionMode::Item => {
            super::suggestions::resources(&mut builder, context.entries("minecraft:item"), "")
        }
        CompletionMode::ComponentAssignmentOrRemoval | CompletionMode::Component => {
            let assignment = matches!(mode, CompletionMode::ComponentAssignmentOrRemoval);
            if assignment {
                builder.suggest("!");
            }
            let remaining = builder.remaining_lower_case();
            for key in context.entries("minecraft:data_component_type") {
                if persistent_component_type(key).is_some()
                    && super::suggestions::resource_matches(&remaining, key)
                {
                    builder.suggest(if assignment {
                        format!("{key}=")
                    } else {
                        key.into()
                    });
                }
            }
        }
        CompletionMode::StartComponents if builder.remaining().is_empty() => {
            builder.suggest("[");
        }
        CompletionMode::Assignment if builder.remaining().is_empty() => {
            builder.suggest("=");
        }
        CompletionMode::NextOrEnd if builder.remaining().is_empty() => {
            builder.suggest(",");
            builder.suggest("]");
        }
        _ => {}
    }
    builder.build()
}
fn component_type(
    reader: &mut StringReader,
) -> Result<(ComponentTypeId, Identifier), CommandSyntaxException> {
    if !reader.can_read() {
        return Err(error_at(reader, "arguments.item.component.expected", &[]));
    }
    let start = reader.cursor();
    let key = identifier(reader)?;
    let id = persistent_component_type(&key);
    let Some(id) = id else {
        reader.set_cursor(start);
        return Err(error_at(reader, "arguments.item.component.unknown", &[key]));
    };
    Ok((
        id,
        Identifier::parse(&key).expect("parsed component identifier"),
    ))
}
pub(super) fn persistent_component_type(key: &str) -> Option<ComponentTypeId> {
    ComponentTypeId::from_name(key).filter(|id| {
        !matches!(
            *id,
            ComponentTypeId::CREATIVE_SLOT_LOCK
                | ComponentTypeId::ADDITIONAL_TRADE_COST
                | ComponentTypeId::MAP_POST_PROCESSING
        )
    })
}
