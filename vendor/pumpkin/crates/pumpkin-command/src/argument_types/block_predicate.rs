use std::collections::HashMap;

use pumpkin_data::tag::{RegistryKey, get_tag_ids};
use pumpkin_data::{Block, BlockState, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
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

    /// Matches state and optional NBT against canonical source block-entity data.
    ///
    /// Callers opting into NBT matching must supply the equivalent of
    /// `BlockEntity.saveWithFullMetadata`, including source identity and components.
    /// `None` means no block entity; even an empty NBT predicate requires one.
    #[must_use]
    pub fn test_state_with_nbt(
        &self,
        state: &BlockState,
        block_entity_nbt: Option<&NbtCompound>,
    ) -> bool {
        if !self.test_state(state) {
            return false;
        }
        let (Self::Block { nbt, .. } | Self::Tag { nbt, .. }) = self;
        match nbt {
            None => true,
            Some(pattern) => {
                block_entity_nbt.is_some_and(|target| matches_nbt_compound(pattern, target))
            }
        }
    }

    pub fn ensure_supported_nbt(&self) -> Result<(), CommandSyntaxError> {
        if self.requires_nbt() {
            Err(UNSUPPORTED_BLOCK_NBT.create_without_context())
        } else {
            Ok(())
        }
    }
}

/// Compound subset matching from `NbtUtils.compareNbt(pattern, target, true)`.
fn matches_nbt_compound(pattern: &NbtCompound, target: &NbtCompound) -> bool {
    target.child_tags.len() >= pattern.child_tags.len()
        && pattern.child_tags.iter().all(|(key, value)| {
            target
                .child_tags
                .get(key)
                .is_some_and(|actual| matches_nbt(value, actual))
        })
}

fn matches_nbt(pattern: &NbtTag, target: &NbtTag) -> bool {
    if std::ptr::eq(pattern, target) {
        return true;
    }
    match (pattern, target) {
        (NbtTag::Compound(pattern), NbtTag::Compound(target)) => {
            matches_nbt_compound(pattern, target)
        }
        (NbtTag::List(pattern), NbtTag::List(target)) => {
            if pattern.is_empty() {
                return target.is_empty();
            }
            // Unlike nbt_path::compare_nbt, the canonical comparison checks length.
            // Matches are unordered and may reuse an entry, without consuming it.
            target.len() >= pattern.len()
                && pattern
                    .iter()
                    .all(|expected| target.iter().any(|actual| matches_nbt(expected, actual)))
        }
        // Scalar types and ordered arrays retain their exact typed equality.
        _ => pattern == target,
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

    fn state(value: &str) -> &'static BlockState {
        ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut StringReader::new(value))
            .unwrap()
            .state
    }

    fn chest_nbt(fields: &str) -> NbtCompound {
        ArgumentType::<DummySource>::parse(
            &NbtCompoundArgumentType,
            &mut StringReader::new(&format!("{{id:'minecraft:chest',x:9,y:64,z:-3,{fields}}}")),
        )
        .unwrap()
    }

    #[test]
    fn canonical_nbt_matching_requires_both_state_and_source_metadata() {
        let source = state("chest[facing=east,type=left,waterlogged=true]");
        let actual =
            chest_nbt("components:{'minecraft:custom_name':'Treasure'},Items:[{Slot:0b,count:2}]");
        let parsed = predicate(
            "chest[facing=east]{id:'minecraft:chest',x:9,components:{'minecraft:custom_name':'Treasure'}}",
        );
        assert!(parsed.test_state_with_nbt(source, Some(&actual)));
        assert!(!parsed.test_state_with_nbt(source, None));
        assert!(!parsed.test_state_with_nbt(state("barrel[facing=east]"), Some(&actual)));
        assert!(!parsed.test_state_with_nbt(state("chest[facing=west]"), Some(&actual)));
        assert!(!parsed.test_state_with_nbt(source, Some(&chest_nbt(""))));
        assert!(!predicate("chest{x:10}").test_state_with_nbt(source, Some(&actual)));
        assert!(
            !predicate("chest{id:'minecraft:barrel'}").test_state_with_nbt(source, Some(&actual))
        );
        assert!(predicate("chest[facing=east]").test_state_with_nbt(source, None));
        assert!(predicate("chest[facing=east]").test_state_with_nbt(source, Some(&actual)));
    }

    #[test]
    fn tag_nbt_matching_checks_membership_and_actual_block_properties() {
        let source = state("chest[facing=east,type=left,waterlogged=true]");
        let actual = chest_nbt("Items:[]");
        let parsed = predicate("#minecraft:features_cannot_replace[facing=east]{Items:[]}");
        assert!(parsed.test_state_with_nbt(source, Some(&actual)));
        assert!(!parsed.test_state_with_nbt(source, None));
        assert!(!parsed.test_state_with_nbt(source, Some(&chest_nbt("Items:[{}]"))));
        assert!(!parsed.test_state_with_nbt(state("barrel[facing=east]"), Some(&actual)));
        assert!(!parsed.test_state_with_nbt(state("bedrock"), Some(&actual)));
        assert!(!parsed.test_state_with_nbt(state("chest[facing=west]"), Some(&actual)));
        for value in [
            "#features_cannot_replace[facing=up]{Items:[]}",
            "#features_cannot_replace[unknown=true]{Items:[]}",
        ] {
            assert!(!predicate(value).test_state_with_nbt(source, Some(&actual)));
        }
        assert!(predicate("#features_cannot_replace").test_state_with_nbt(source, None));
    }

    #[test]
    fn compound_patterns_match_nested_subsets_and_require_present_keys() {
        let source = state("chest");
        let actual = chest_nbt("data:{nested:{value:1,extra:2},other:3}");
        for value in [
            "chest{}",
            "chest{data:{}}",
            "chest{data:{nested:{value:1}}}",
        ] {
            assert!(predicate(value).test_state_with_nbt(source, Some(&actual)));
            assert!(!predicate(value).test_state_with_nbt(source, None));
        }
        for value in [
            "chest{missing:{}}",
            "chest{data:{missing:1}}",
            "chest{data:{nested:{value:2}}}",
            "chest{data:{nested:1}}",
        ] {
            assert!(!predicate(value).test_state_with_nbt(source, Some(&actual)));
        }
    }

    #[test]
    fn nbt_scalars_preserve_numeric_widths_and_string_types() {
        let source = state("chest");
        let values = ["1b", "1s", "1", "1L", "1.0f", "1.0d", "'1'"];
        for (pattern_index, pattern_value) in values.iter().enumerate() {
            let parsed = predicate(&format!("chest{{value:{pattern_value}}}"));
            for (target_index, target_value) in values.iter().enumerate() {
                let actual = chest_nbt(&format!("value:{target_value}"));
                assert_eq!(
                    parsed.test_state_with_nbt(source, Some(&actual)),
                    pattern_index == target_index,
                    "pattern {pattern_value}, target {target_value}"
                );
            }
        }
    }

    #[test]
    fn list_patterns_are_unordered_with_a_length_floor_and_reusable_matches() {
        let source = state("chest");
        for (pattern, target, expected) in [
            ("[1,2]", "[2,3,1]", true),
            ("[1,1]", "[1]", false),
            ("[1,1]", "[1,2]", true),
            ("[1,1,2]", "[1,2]", false),
            ("[1,2]", "[1,3]", false),
            ("[{a:1}]", "[{a:2},{a:1,b:3}]", true),
            ("[{a:1},{a:1}]", "[{a:1},{b:2}]", true),
            ("[[1,2]]", "[[2,1,3]]", true),
            ("[[1,1]]", "[[1]]", false),
            ("[1b]", "[1]", false),
            ("[]", "[]", true),
            ("[]", "[1]", false),
            ("[1]", "[]", false),
            ("[{}]", "[1]", false),
        ] {
            let parsed = predicate(&format!("chest{{value:{pattern}}}"));
            let actual = chest_nbt(&format!("value:{target}"));
            assert_eq!(
                parsed.test_state_with_nbt(source, Some(&actual)),
                expected,
                "pattern {pattern}, target {target}"
            );
        }
        assert!(!predicate("chest{value:[]}").test_state_with_nbt(source, Some(&chest_nbt(""))));
    }

    #[test]
    fn typed_arrays_remain_ordered_and_exact_inside_compounds_and_lists() {
        let source = state("chest");
        for (pattern, target, expected) in [
            ("[B;1b,2b]", "[B;1b,2b]", true),
            ("[I;1,2]", "[I;1,2]", true),
            ("[L;1L,2L]", "[L;1L,2L]", true),
            ("[B;1b,2b]", "[B;2b,1b]", false),
            ("[I;1,2]", "[I;2,1]", false),
            ("[L;1L,2L]", "[L;2L,1L]", false),
            ("[I;1]", "[I;1,2]", false),
            ("[I;1,2]", "[I;1]", false),
            ("[B;1b]", "[I;1]", false),
            ("[I;1]", "[L;1L]", false),
            ("[I;1]", "[1]", false),
            ("[1]", "[I;1]", false),
            ("[]", "[I;]", false),
            ("[I;]", "[]", false),
            ("[I;]", "[I;]", true),
            ("[I;]", "[I;1]", false),
            ("[{a:[I;1,2]}]", "[{a:[I;1,2],b:3}]", true),
            ("[{a:[I;1,2]}]", "[{a:[I;2,1]}]", false),
            ("[[B;1b,2b],[B;3b]]", "[[B;3b],[B;1b,2b]]", true),
            ("[[B;1b,2b]]", "[[B;2b,1b]]", false),
        ] {
            let parsed = predicate(&format!("chest{{value:{pattern}}}"));
            let actual = chest_nbt(&format!("value:{target}"));
            assert_eq!(
                parsed.test_state_with_nbt(source, Some(&actual)),
                expected,
                "pattern {pattern}, target {target}"
            );
        }
    }

    #[test]
    fn opting_into_nbt_matching_preserves_the_legacy_refusal() {
        let source = state("chest[facing=east]");
        let actual = chest_nbt("Items:[]");
        for value in [
            "chest[facing=east]{Items:[]}",
            "#features_cannot_replace[facing=east]{Items:[]}",
            "chest{}",
            "#features_cannot_replace{}",
        ] {
            let parsed = predicate(value);
            assert!(parsed.test_state_with_nbt(source, Some(&actual)));
            assert_eq!(
                parsed.ensure_supported_nbt(),
                Err(UNSUPPORTED_BLOCK_NBT.create_without_context())
            );
        }
        for value in [
            "chest[facing=east]",
            "#features_cannot_replace[facing=east]",
        ] {
            assert!(predicate(value).ensure_supported_nbt().is_ok());
        }
    }
}
