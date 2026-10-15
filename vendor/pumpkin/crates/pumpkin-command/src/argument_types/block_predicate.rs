use std::collections::HashMap;

use pumpkin_data::tag::{RegistryKey, get_tag_ids};
use pumpkin_data::{Block, BlockState, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

use crate::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::argument_types::block::{INVALID_BLOCK_ERROR_TYPE, UNSUPPORTED_BLOCK_NBT};
use crate::argument_types::nbt::NbtCompoundArgumentType;
use crate::context::command_context::CommandContext;
use crate::errors::command_syntax_error::CommandSyntaxError;
use crate::errors::error_types::CommandErrorType;
use crate::string_reader::StringReader;
use crate::suggestion::suggestions::{Suggestions, SuggestionsBuilder};

pub const ERROR_UNKNOWN_TAG: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENTS_BLOCK_TAG_UNKNOWN,
    translation::java::ARGUMENTS_BLOCK_TAG_UNKNOWN,
);

pub const ERROR_NO_VALUE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
);

pub const ERROR_UNCLOSED_PROPERTIES: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
);

const ERROR_UNKNOWN_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
);
const ERROR_INVALID_PROPERTY: CommandErrorType<3> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
);
const ERROR_DUPLICATE_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
);

#[derive(Clone, Debug)]
pub enum BlockPredicate {
    Block {
        block: &'static Block,
        properties: HashMap<String, String>,
        nbt: Option<NbtCompound>,
    },
    Tag {
        tag_name: String,
        block_ids: Vec<u16>,
        properties: HashMap<String, String>,
        nbt: Option<NbtCompound>,
    },
}

impl BlockPredicate {
    #[must_use]
    pub fn test(&self, block: &Block) -> bool {
        match self {
            Self::Block {
                block: expected, ..
            } => block.id == expected.id,
            Self::Tag { block_ids, .. } => block_ids.contains(&block.id.as_u16()),
        }
    }

    #[must_use]
    pub const fn requires_nbt(&self) -> bool {
        match self {
            Self::Block { nbt, .. } | Self::Tag { nbt, .. } => nbt.is_some(),
        }
    }

    /// Unspecified properties are wildcards, rather than implicit defaults.
    #[must_use]
    pub fn test_state(&self, state: &BlockState) -> bool {
        let block = Block::from_state_id(state.id);
        if !self.test(block) {
            return false;
        }
        let (Self::Block { properties, .. } | Self::Tag { properties, .. }) = self;
        if properties.is_empty() {
            return true;
        }
        block.properties(state.id).is_some_and(|actual| {
            let actual = actual.to_props();
            properties.iter().all(|(key, value)| {
                actual
                    .iter()
                    .any(|(actual_key, actual_value)| *actual_key == key && *actual_value == value)
            })
        })
    }

    pub fn ensure_supported_nbt(&self) -> Result<(), CommandSyntaxError> {
        if self.requires_nbt() {
            Err(UNSUPPORTED_BLOCK_NBT.create_without_context())
        } else {
            Ok(())
        }
    }
}

pub(crate) fn parse_properties(
    reader: &mut StringReader,
    block: Option<&'static Block>,
    holder: &str,
) -> Result<HashMap<String, String>, CommandSyntaxError> {
    let mut properties = HashMap::new();
    if reader.peek() == Some('[') {
        reader.skip();
        reader.skip_whitespace();
        while reader.can_read_char() && reader.peek() != Some(']') {
            let key = reader.read_string()?;
            if properties.contains_key(&key) {
                return Err(ERROR_DUPLICATE_PROPERTY.create(
                    reader,
                    TextComponent::text(holder.to_string()),
                    TextComponent::text(key),
                ));
            }
            if let Some(block) = block
                && !block
                    .properties(block.default_state.id)
                    .is_some_and(|props| props.to_props().iter().any(|(name, _)| *name == key))
            {
                return Err(ERROR_UNKNOWN_PROPERTY.create(
                    reader,
                    TextComponent::text(holder.to_string()),
                    TextComponent::text(key),
                ));
            }
            reader.skip_whitespace();
            if reader.peek() != Some('=') {
                return Err(ERROR_NO_VALUE.create(reader, TextComponent::text(key)));
            }
            reader.skip();
            reader.skip_whitespace();
            let val = reader.read_string()?;
            if val.is_empty() {
                return Err(ERROR_NO_VALUE.create(reader, TextComponent::text(key)));
            }
            if let Some(block) = block
                && !block.states.iter().any(|state| {
                    block.properties(state.id).is_some_and(|props| {
                        props
                            .to_props()
                            .iter()
                            .any(|(name, value)| *name == key && *value == val)
                    })
                })
            {
                return Err(ERROR_INVALID_PROPERTY.create(
                    reader,
                    TextComponent::text(holder.to_string()),
                    TextComponent::text(key),
                    TextComponent::text(val),
                ));
            }
            properties.insert(key, val);
            reader.skip_whitespace();
            if reader.peek() == Some(',') {
                reader.skip();
                reader.skip_whitespace();
            } else if reader.peek() == Some(']') {
                break;
            } else {
                return Err(ERROR_UNCLOSED_PROPERTIES.create_without_context());
            }
        }
        if reader.peek() == Some(']') {
            reader.skip();
        } else {
            return Err(ERROR_UNCLOSED_PROPERTIES.create_without_context());
        }
    }
    Ok(properties)
}

pub(crate) fn parse_nbt(
    reader: &mut StringReader,
) -> Result<Option<NbtCompound>, CommandSyntaxError> {
    if reader.peek() == Some('{') {
        let tag =
            ArgumentType::<crate::source::DummySource>::parse(&NbtCompoundArgumentType, reader)?;
        Ok(Some(tag))
    } else {
        Ok(None)
    }
}

pub struct BlockPredicateArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockPredicateArgumentType {
    type Item = BlockPredicate;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        if reader.peek() == Some('#') {
            reader.skip();
            let start = reader.cursor();
            while let Some(c) = reader.peek() {
                if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                    reader.skip();
                } else {
                    break;
                }
            }
            let tag_str = &reader.string()[start..reader.cursor()];
            let normalized = if tag_str.contains(':') {
                tag_str.to_string()
            } else {
                format!("minecraft:{tag_str}")
            };
            let stripped = normalized.strip_prefix("minecraft:").unwrap_or(&normalized);

            let tag_ids = get_tag_ids(RegistryKey::Block, &normalized)
                .or_else(|| get_tag_ids(RegistryKey::Block, stripped));

            let block_ids = match tag_ids {
                Some(ids) => ids.to_vec(),
                None => {
                    return Err(ERROR_UNKNOWN_TAG.create(reader, TextComponent::text(normalized)));
                }
            };

            let properties = parse_properties(reader, None, &normalized)?;
            let nbt = parse_nbt(reader)?;

            Ok(BlockPredicate::Tag {
                tag_name: normalized,
                block_ids,
                properties,
                nbt,
            })
        } else {
            let start = reader.cursor();
            while let Some(c) = reader.peek() {
                if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                    reader.skip();
                } else {
                    break;
                }
            }
            let block_str = &reader.string()[start..reader.cursor()];
            let normalized = if block_str.contains(':') {
                block_str.to_string()
            } else {
                format!("minecraft:{block_str}")
            };

            let block = Block::from_name(&normalized).ok_or_else(|| {
                INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized.clone()))
            })?;

            let properties = parse_properties(reader, Some(block), &normalized)?;
            let nbt = parse_nbt(reader)?;

            Ok(BlockPredicate::Block {
                block,
                properties,
                nbt,
            })
        }
    }

    fn client_side_parser(&self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockPredicate
    }

    fn examples(&self) -> Vec<String> {
        vec![
            "stone".to_string(),
            "minecraft:stone".to_string(),
            "oak_stairs[facing=east]".to_string(),
            "#stone".to_string(),
            "#stone[foo=bar]{baz:1}".to_string(),
        ]
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl BlockPredicateArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockPredicate, CommandSyntaxError> {
        Ok(context.get_argument::<BlockPredicate>(name)?.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argument_types::block::BlockArgumentType;
    use crate::source::DummySource;

    fn predicate(value: &str) -> BlockPredicate {
        ArgumentType::<DummySource>::parse(
            &BlockPredicateArgumentType,
            &mut StringReader::new(value),
        )
        .unwrap()
    }

    #[test]
    fn explicit_properties_match_without_requiring_default_values() {
        let state = ArgumentType::<DummySource>::parse(
            &BlockArgumentType,
            &mut StringReader::new("oak_stairs[facing=east,half=top,waterlogged=true]"),
        )
        .unwrap()
        .state;
        assert!(predicate("oak_stairs").test_state(state));
        assert!(predicate("oak_stairs[facing=east]").test_state(state));
        assert!(predicate("oak_stairs[half=top,facing=east]").test_state(state));
        assert!(!predicate("oak_stairs[facing=west]").test_state(state));
        assert!(!predicate("oak_stairs[waterlogged=false]").test_state(state));
        assert!(!predicate("stone").test_state(state));
    }

    #[test]
    fn direct_predicates_validate_property_names_and_values() {
        for value in [
            "stone[unknown=true]",
            "oak_stairs[facing=up]",
            "oak_stairs[half=bottom,half=top]",
        ] {
            assert!(
                ArgumentType::<DummySource>::parse(
                    &BlockPredicateArgumentType,
                    &mut StringReader::new(value)
                )
                .is_err(),
                "accepted {value}"
            );
        }
    }

    #[test]
    fn nbt_predicates_remain_parsed_but_execution_reports_unsupported() {
        let parsed = predicate("chest[facing=east]{CustomName:'\"Test\"'}");
        assert!(parsed.requires_nbt());
        assert!(parsed.ensure_supported_nbt().is_err());
    }
}
