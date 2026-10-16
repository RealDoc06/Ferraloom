use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{DataComponentImpl, ItemNameImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::translation;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;
use std::collections::HashSet;

use crate::argument_types::FromStringReader;
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

pub const ERROR_UNKNOWN_COMPONENT: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENTS_ITEM_COMPONENT_UNKNOWN,
    translation::java::ARGUMENTS_ITEM_COMPONENT_UNKNOWN,
);

pub const ERROR_EXPECTED_COMPONENT: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENTS_ITEM_COMPONENT_EXPECTED,
    translation::java::ARGUMENTS_ITEM_COMPONENT_EXPECTED,
);

pub const ERROR_REPEATED_COMPONENT: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENTS_ITEM_COMPONENT_REPEATED,
    translation::java::ARGUMENTS_ITEM_COMPONENT_REPEATED,
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
            let mut seen = HashSet::new();
            while reader.can_read_char() && reader.peek() != Some(']') {
                reader.skip_whitespace();
                let removed = reader.peek() == Some('!');
                if removed {
                    reader.skip();
                }
                if !reader.can_read_char() {
                    return Err(ERROR_EXPECTED_COMPONENT.create(reader));
                }
                let key_start = reader.cursor();
                let key = Identifier::from_reader(reader)?;
                let key_str = key.to_string();
                let data_comp = DataComponent::try_from_name(&key_str)
                    .filter(|component| component.is_persistent())
                    .ok_or_else(|| {
                        reader.set_cursor(key_start);
                        ERROR_UNKNOWN_COMPONENT.create(reader, TextComponent::text(key_str.clone()))
                    })?;
                if !seen.insert(data_comp) {
                    return Err(ERROR_REPEATED_COMPONENT
                        .create_without_context(TextComponent::text(key_str)));
                }

                if removed {
                    patch.push((data_comp, None));
                } else {
                    reader.skip_whitespace();
                    reader.expect('=')?;
                    reader.skip_whitespace();
                    let value_start = reader.cursor();
                    let nbt_tag = SnbtParser::parse_for_commands(reader)?;
                    let parsed = if data_comp == DataComponent::ItemName {
                        ItemNameImpl::parse(&nbt_tag).map(|name| name.to_dyn())
                    } else {
                        pumpkin_data::data_component_impl::read_data(data_comp, &nbt_tag)
                            .ok_or("Invalid or unsupported component value")
                    };
                    match parsed {
                        Ok(component) => patch.push((data_comp, Some(component))),
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
                if reader.peek() != Some(',') {
                    break;
                }
                reader.skip();
                reader.skip_whitespace();
                if !reader.can_read_char() {
                    return Err(ERROR_EXPECTED_COMPONENT.create(reader));
                }
            }
            reader.expect(']')?;
            stack.patch = patch;
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
    fn component_removal_and_nested_values_leave_the_next_argument_unread() {
        let mut reader = StringReader::new(
            "stick[!item_name,custom_data={nested:[1,2],label:'a,b]c'},damage=1,] 4",
        );
        let stack =
            <ItemStackArgumentType as ArgumentType<()>>::parse(&ItemStackArgumentType, &mut reader)
                .expect("removals, nested SNBT and a trailing comma are valid");
        assert_eq!(reader.remaining_part(), " 4");
        assert_eq!(stack.patch.len(), 3);
        assert_eq!(stack.patch[0].0, DataComponent::ItemName);
        assert!(stack.patch[0].1.is_none());
        assert!(!stack.has_data_component(DataComponent::ItemName));
        let data = stack.patch[1].1.as_ref().unwrap().write_data();
        let compound = data.extract_compound().unwrap();
        assert_eq!(compound.get_string("label"), Some("a,b]c"));
        assert_eq!(compound.get_list("nested").unwrap().len(), 2);
        assert_eq!(stack.patch[2].0, DataComponent::Damage);

        for input in ["stick[]", "stick[ !item_name ]", "stick[!item_name, ]"] {
            let mut reader = StringReader::new(input);
            assert!(
                <ItemStackArgumentType as ArgumentType<()>>::parse(
                    &ItemStackArgumentType,
                    &mut reader,
                )
                .is_ok(),
                "{input}"
            );
            assert_eq!(reader.remaining_part(), "");
        }
    }

    #[test]
    fn malformed_component_lists_refuse_instead_of_looping_or_returning_partial_stacks() {
        for input in [
            "stick[",
            "stick[ ]",
            "stick[!",
            "stick[?]",
            "stick[damage]",
            "stick[damage=1",
            "stick[damage=1,",
            "stick[damage=1 item_name='x']",
            "stick[!item_name='x']",
            "stick[item_name='x',,damage=1]",
        ] {
            let mut reader = StringReader::new(input);
            assert!(
                <ItemStackArgumentType as ArgumentType<()>>::parse(
                    &ItemStackArgumentType,
                    &mut reader,
                )
                .is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn aliases_share_duplicate_detection_and_transient_components_are_unknown() {
        for input in [
            "stick[item_name='x',minecraft:item_name='y']",
            "stick[!item_name,minecraft:item_name='y']",
            "stick[item_name='x',!minecraft:item_name]",
            "stick[!item_name,!minecraft:item_name]",
        ] {
            let mut reader = StringReader::new(input);
            let error = <ItemStackArgumentType as ArgumentType<()>>::parse(
                &ItemStackArgumentType,
                &mut reader,
            )
            .err()
            .expect("each component may occur only once, including removals");
            assert!(error.is(&ERROR_REPEATED_COMPONENT), "{input}");
        }
        for key in ["missing", "other:item_name", "creative_slot_lock"] {
            for prefix in ["!", ""] {
                let input = format!("stick[{prefix}{key}]");
                let mut reader = StringReader::new(&input);
                let error = <ItemStackArgumentType as ArgumentType<()>>::parse(
                    &ItemStackArgumentType,
                    &mut reader,
                )
                .err()
                .expect("unknown and transient components must not enter the patch");
                assert!(error.is(&ERROR_UNKNOWN_COMPONENT), "{input}");
                assert_eq!(error.context.unwrap().cursor, "stick[".len() + prefix.len());
            }
        }
    }

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
