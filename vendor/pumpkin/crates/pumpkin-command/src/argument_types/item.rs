use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{DataComponentImpl, ItemNameImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::translation;
use pumpkin_util::text::TextComponent;

use crate::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::context::command_context::CommandContext;
use crate::errors::command_syntax_error::CommandSyntaxError;
use crate::errors::error_types::CommandErrorType;
use crate::snbt::SnbtParser;
use crate::string_reader::StringReader;
use crate::suggestion::suggestions::{Suggestions, SuggestionsBuilder};

pub const ERROR_UNKNOWN_ITEM: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_ITEM_ID_INVALID,
    translation::java::ARGUMENT_ITEM_ID_INVALID,
);

pub const ERROR_INVALID_COMPONENT: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENTS_ITEM_COMPONENT_MALFORMED,
    translation::java::ARGUMENTS_ITEM_COMPONENT_MALFORMED,
);

#[derive(Clone, Copy)]
pub struct ItemStackArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for ItemStackArgumentType {
    type Item = ItemStack;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let raw_id = &reader.string()[start..reader.cursor()];
        let item = Item::from_registry_key(raw_id).ok_or_else(|| {
            let full_name = if raw_id.contains(':') {
                raw_id.to_string()
            } else {
                format!("minecraft:{raw_id}")
            };
            ERROR_UNKNOWN_ITEM.create(reader, TextComponent::text(full_name))
        })?;

        let mut stack = ItemStack::new(1, item);

        // Optional components [...]
        if reader.peek() == Some('[') {
            reader.skip();
            let mut patch = Vec::new();
            while reader.can_read_char() && reader.peek() != Some(']') {
                let key_start = reader.cursor();
                while let Some(c) = reader.peek() {
                    if c.is_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '-' {
                        reader.skip();
                    } else {
                        break;
                    }
                }
                let key_str = reader.string()[key_start..reader.cursor()]
                    .trim()
                    .to_string();

                reader.skip_whitespace();
                if reader.peek() == Some('=') {
                    reader.skip();
                    reader.skip_whitespace();
                    let value_start = reader.cursor();
                    let nbt_tag = SnbtParser::parse_for_commands(reader)?;
                    let parsed = DataComponent::try_from_name(&key_str)
                        .ok_or("Unknown data component")
                        .and_then(|data_comp| {
                            let component = if data_comp == DataComponent::ItemName {
                                ItemNameImpl::parse(&nbt_tag).map(|name| name.to_dyn())
                            } else {
                                pumpkin_data::data_component_impl::read_data(data_comp, &nbt_tag)
                                    .ok_or("Invalid or unsupported component value")
                            }?;
                            Ok((data_comp, Some(component)))
                        });
                    match parsed {
                        Ok(component) => patch.push(component),
                        Err(reason) => {
                            // Point at the rejected value, not after its closing bracket.
                            // No partially parsed stack is returned to /give.
                            reader.set_cursor(value_start);
                            return Err(ERROR_INVALID_COMPONENT.create(
                                reader,
                                TextComponent::text(key_str),
                                TextComponent::text(reason),
                            ));
                        }
                    }
                }
                reader.skip_whitespace();
                if reader.peek() == Some(',') {
                    reader.skip();
                    reader.skip_whitespace();
                }
            }
            if reader.peek() == Some(']') {
                reader.skip();
            }
            if !patch.is_empty() {
                stack.patch = patch;
            }
        }

        Ok(stack)
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::ItemStack
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl ItemStackArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<ItemStack, CommandSyntaxError> {
        context.get_argument::<ItemStack>(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_component_returns_typed_error_at_value_start() {
        for value in [
            "{color:'red'}",
            "{text:'x',bold:'wrong'}",
            "{nbt:'foo',storage:'minecraft:test'}",
        ] {
            let input = format!("give @s stick[damage=1,item_name={value}]");
            let mut reader = StringReader::new(&input);
            reader.set_cursor("give @s ".len());
            let error = <ItemStackArgumentType as ArgumentType<()>>::parse(
                &ItemStackArgumentType,
                &mut reader,
            )
            .err()
            .expect("invalid/unsupported component must refuse the command");
            assert!(error.is(&ERROR_INVALID_COMPONENT));
            let context = error.context.expect("component error needs cursor context");
            assert_eq!(context.cursor, input.find(value).unwrap());
            assert_eq!(context.input, input);
            assert_eq!(reader.cursor(), context.cursor);
        }
    }

    #[test]
    fn another_rejected_component_is_not_silently_omitted() {
        let mut reader = StringReader::new("stick[damage='wrong']");
        let error =
            <ItemStackArgumentType as ArgumentType<()>>::parse(&ItemStackArgumentType, &mut reader)
                .err()
                .expect("rejected component must not produce a successful stack");
        assert!(error.is(&ERROR_INVALID_COMPONENT));
    }
}
