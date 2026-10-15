use crate::{
    argument_types::argument_type::{ArgumentType, JavaClientArgumentType},
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};
use std::collections::HashMap;

use pumpkin_data::{Block, BlockState, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

use super::block_predicate::{parse_nbt, parse_properties};
use crate::errors::error_types::LiteralCommandErrorType;

pub const INVALID_BLOCK_ERROR_TYPE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
);

pub const UNSUPPORTED_BLOCK_NBT: LiteralCommandErrorType =
    LiteralCommandErrorType::new("Block NBT placement and matching are not implemented yet");
const UNSUPPORTED_STATE_FILTER: LiteralCommandErrorType =
    LiteralCommandErrorType::new("This command does not support block state filters yet");

/// Retains the placement state separately from the block's immutable default.
#[derive(Debug, Clone)]
pub struct BlockInput {
    pub block: &'static Block,
    pub state: &'static BlockState,
    pub properties: HashMap<String, String>,
    pub nbt: Option<NbtCompound>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockArgumentType {
    type Item = BlockInput;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let block_name = &reader.string()[start..reader.cursor()];
        let normalized = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("minecraft:{block_name}")
        };

        let block = Block::from_name(&normalized).ok_or_else(|| {
            INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized.clone()))
        })?;
        let properties = parse_properties(reader, Some(block), &normalized)?;
        let nbt = parse_nbt(reader)?;
        let state = if properties.is_empty() {
            block.default_state
        } else {
            let defaults = block.properties(block.default_state.id);
            let mut complete: HashMap<String, String> = defaults
                .map(|defaults| {
                    defaults
                        .to_props()
                        .into_iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            complete.extend(properties.clone());
            let complete: Vec<_> = complete
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            block.state_from_properties(&complete).ok_or_else(|| {
                INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized))
            })?
        };
        Ok(BlockInput {
            block,
            state,
            properties,
            nbt,
        })
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockState
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl BlockArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<&'static Block, CommandSyntaxError> {
        let input = Self::get_input(context, name)?;
        if input.nbt.is_some() {
            return Err(UNSUPPORTED_BLOCK_NBT.create_without_context());
        }
        if !input.properties.is_empty() {
            return Err(UNSUPPORTED_STATE_FILTER.create_without_context());
        }
        Ok(input.block)
    }

    pub fn get_input<'a, S: crate::source::CommandSource>(
        context: &'a CommandContext<S>,
        name: &str,
    ) -> Result<&'a BlockInput, CommandSyntaxError> {
        context.get_argument(name)
    }

    pub fn get_state<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<&'static BlockState, CommandSyntaxError> {
        let input = Self::get_input(context, name)?;
        if input.nbt.is_some() {
            return Err(UNSUPPORTED_BLOCK_NBT.create_without_context());
        }
        Ok(input.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::DummySource;

    fn parse(value: &str) -> Result<BlockInput, CommandSyntaxError> {
        ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut StringReader::new(value))
    }

    #[test]
    fn placement_merges_explicit_properties_with_defaults() {
        let parsed = parse("oak_stairs[facing=east,half=top]").unwrap();
        let properties = parsed.block.properties(parsed.state.id).unwrap().to_props();
        assert!(properties.contains(&("facing", "east")));
        assert!(properties.contains(&("half", "top")));
        assert!(properties.contains(&("waterlogged", "false")));
        assert_ne!(parsed.state.id, parsed.block.default_state.id);
    }

    #[test]
    fn rejects_unknown_duplicate_missing_and_invalid_properties() {
        for value in [
            "stone[unknown=true]",
            "oak_stairs[facing=up]",
            "oak_stairs[facing=east,facing=west]",
            "oak_stairs[facing=]",
            "oak_stairs[facing=east",
            "oak_stairs[facing=east half=top]",
        ] {
            assert!(parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn keeps_nbt_and_stops_before_the_next_command_argument() {
        let mut reader = StringReader::new("chest[facing=east]{CustomName:'\"Test\"'} replace");
        let parsed = ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut reader).unwrap();
        assert!(parsed.nbt.is_some());
        assert_eq!(&reader.string()[reader.cursor()..], " replace");
    }
}
