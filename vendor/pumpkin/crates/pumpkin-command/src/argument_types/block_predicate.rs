use std::collections::{BTreeSet, HashMap};

use pumpkin_data::tag::{RegistryKey, get_latest_map, get_tag_ids};
use pumpkin_data::{Block, BlockId, BlockState, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;

use crate::argument_types::FromStringReader;
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

pub const ERROR_NO_VALUE: CommandErrorType<2> = CommandErrorType::new(
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
    /// Callers own codec normalization; matching preserves the supplied float zero signs.
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
        nbt.as_ref().is_none_or(|pattern| {
            block_entity_nbt.is_some_and(|target| matches_nbt_compound(pattern, target))
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
        // FloatTag/DoubleTag record equality equates all NaNs but distinguishes signed zero.
        (NbtTag::Float(pattern), NbtTag::Float(target)) => {
            pattern.to_bits() == target.to_bits() || (pattern.is_nan() && target.is_nan())
        }
        (NbtTag::Double(pattern), NbtTag::Double(target)) => {
            pattern.to_bits() == target.to_bits() || (pattern.is_nan() && target.is_nan())
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
    let mut parser = BlockPredicateParser {
        block,
        ..BlockPredicateParser::default()
    };
    parser.read_properties(reader, holder)?;
    Ok(parser.properties)
}

#[derive(Default)]
enum PredicateSuggestions {
    #[default]
    BlockIdOrTag,
    Tag,
    OpenPropertiesOrNbt,
    OpenNbt,
    PropertyName {
        allow_end: bool,
    },
    Equals,
    PropertyValue(String),
    NextPropertyOrEnd,
    Nothing,
}

/// The same parse supplies arguments and completions; only argument errors rewind entry.
#[derive(Default)]
struct BlockPredicateParser {
    block: Option<&'static Block>,
    tag_ids: Option<&'static [u16]>,
    properties: HashMap<String, String>,
    suggestions: PredicateSuggestions,
}

impl BlockPredicateParser {
    fn parse(&mut self, reader: &mut StringReader) -> Result<BlockPredicate, CommandSyntaxError> {
        let start = reader.cursor();
        let (mut predicate, holder) = if reader.peek() == Some('#') {
            reader.skip();
            self.suggestions = PredicateSuggestions::Tag;
            let normalized = Identifier::from_reader(reader)?.to_string();
            let stripped = normalized.strip_prefix("minecraft:").unwrap_or(&normalized);
            let Some(ids) = get_tag_ids(RegistryKey::Block, &normalized)
                .or_else(|| get_tag_ids(RegistryKey::Block, stripped))
            else {
                reader.set_cursor(start);
                return Err(ERROR_UNKNOWN_TAG.create(reader, TextComponent::text(normalized)));
            };
            self.tag_ids = Some(ids);
            (
                BlockPredicate::Tag {
                    tag_name: normalized,
                    block_ids: ids.to_vec(),
                    properties: HashMap::new(),
                    nbt: None,
                },
                // The retained readTag leaves the parser's diagnostic id at its default.
                "minecraft:".to_string(),
            )
        } else {
            let normalized = Identifier::from_reader(reader)?.to_string();
            let Some(block) = Block::from_name(&normalized) else {
                reader.set_cursor(start);
                return Err(
                    INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized))
                );
            };
            self.block = Some(block);
            (
                BlockPredicate::Block {
                    block,
                    properties: HashMap::new(),
                    nbt: None,
                },
                normalized,
            )
        };

        self.suggestions = PredicateSuggestions::OpenPropertiesOrNbt;
        if reader.peek() == Some('[') {
            self.read_properties(reader, &holder)?;
            self.suggestions = PredicateSuggestions::OpenNbt;
        }
        if reader.peek() == Some('{') {
            self.suggestions = PredicateSuggestions::Nothing;
        }
        let nbt = parse_nbt(reader)?;
        let (BlockPredicate::Block {
            properties,
            nbt: parsed_nbt,
            ..
        }
        | BlockPredicate::Tag {
            properties,
            nbt: parsed_nbt,
            ..
        }) = &mut predicate;
        *properties = std::mem::take(&mut self.properties);
        *parsed_nbt = nbt;
        Ok(predicate)
    }

    fn read_properties(
        &mut self,
        reader: &mut StringReader,
        holder: &str,
    ) -> Result<(), CommandSyntaxError> {
        if reader.peek() != Some('[') {
            return Ok(());
        }
        reader.skip();
        self.suggestions = PredicateSuggestions::PropertyName { allow_end: true };
        let mut value_start = None;
        reader.skip_whitespace();
        while reader.can_read_char() && reader.peek() != Some(']') {
            // Deliberately after the loop condition: ',]' closes, ', ]' reads another key.
            reader.skip_whitespace();
            let key_start = reader.cursor();
            let key = reader.read_string()?;
            if let Some(block) = self.block
                && !property_names(block).contains(&key.as_str())
            {
                reader.set_cursor(key_start);
                return Err(ERROR_UNKNOWN_PROPERTY.create(
                    reader,
                    TextComponent::text(holder.to_string()),
                    TextComponent::text(key),
                ));
            }
            if self.properties.contains_key(&key) {
                reader.set_cursor(key_start);
                return Err(ERROR_DUPLICATE_PROPERTY.create(
                    reader,
                    TextComponent::text(key),
                    TextComponent::text(holder.to_string()),
                ));
            }
            reader.skip_whitespace();
            if self.block.is_some() {
                self.suggestions = PredicateSuggestions::Equals;
            }
            if reader.peek() != Some('=') {
                if self.block.is_none() {
                    reader.set_cursor(key_start);
                }
                return Err(ERROR_NO_VALUE.create(
                    reader,
                    TextComponent::text(key),
                    TextComponent::text(holder.to_string()),
                ));
            }
            reader.skip();
            reader.skip_whitespace();
            self.suggestions = PredicateSuggestions::PropertyValue(key.clone());
            let start = reader.cursor();
            value_start = Some(start);
            let val = reader.read_string()?;
            if let Some(block) = self.block
                && !property_values(block, &key).contains(val.as_str())
            {
                reader.set_cursor(start);
                return Err(ERROR_INVALID_PROPERTY.create(
                    reader,
                    TextComponent::text(holder.to_string()),
                    TextComponent::text(val),
                    TextComponent::text(key),
                ));
            }
            self.properties.insert(key, val);
            if self.block.is_some() {
                self.suggestions = PredicateSuggestions::NextPropertyOrEnd;
            }
            reader.skip_whitespace();
            if reader.can_read_char() {
                value_start = None;
                match reader.peek() {
                    Some(',') => {
                        reader.skip();
                        self.suggestions = PredicateSuggestions::PropertyName { allow_end: false };
                    }
                    Some(']') => break,
                    _ => return Err(ERROR_UNCLOSED_PROPERTIES.create(reader)),
                }
            }
        }
        if reader.can_read_char() {
            reader.skip();
        } else {
            // Vague values remain strings, and an unfinished final value is replaceable.
            if self.block.is_none()
                && let Some(start) = value_start
            {
                reader.set_cursor(start);
            }
            return Err(ERROR_UNCLOSED_PROPERTIES.create(reader));
        }
        Ok(())
    }

    fn blocks(&self) -> impl Iterator<Item = &'static Block> + '_ {
        self.block.into_iter().chain(
            self.tag_ids
                .into_iter()
                .flatten()
                .filter_map(|id| BlockId::new(*id))
                .map(Block::from_id),
        )
    }

    fn suggest(&self, mut builder: SuggestionsBuilder) -> Suggestions {
        match &self.suggestions {
            PredicateSuggestions::BlockIdOrTag | PredicateSuggestions::Tag => {
                let tags = get_latest_map(RegistryKey::Block)
                    .keys()
                    .map(|tag| format!("#{tag}"));
                // Resource prefix filtering is project behavior; the Java helper is unclosed.
                builder = builder.filter_and_suggest_iter(tags);
                if matches!(self.suggestions, PredicateSuggestions::BlockIdOrTag) {
                    let blocks = (0..BlockId::COUNT)
                        .filter_map(BlockId::new)
                        .map(Block::from_id)
                        .map(|block| format!("minecraft:{}", block.name));
                    builder = builder.filter_and_suggest_iter(blocks);
                }
            }
            PredicateSuggestions::OpenPropertiesOrNbt | PredicateSuggestions::OpenNbt => {
                if builder.remaining().is_empty() {
                    if matches!(self.suggestions, PredicateSuggestions::OpenPropertiesOrNbt)
                        && self.blocks().any(|block| !property_names(block).is_empty())
                    {
                        builder = builder.suggest("[");
                    }
                    if self
                        .blocks()
                        .any(|block| block.default_state.block_entity_type != u16::MAX)
                    {
                        builder = builder.suggest("{");
                    }
                }
            }
            PredicateSuggestions::PropertyName { allow_end } => {
                if *allow_end && builder.remaining().is_empty() {
                    builder = builder.suggest("]");
                }
                let prefix = builder.remaining().to_lowercase();
                for name in self.blocks().flat_map(property_names) {
                    if !self.properties.contains_key(name) && name.starts_with(&prefix) {
                        builder = builder.suggest(format!("{name}="));
                    }
                }
            }
            PredicateSuggestions::Equals => {
                if builder.remaining().is_empty() {
                    builder = builder.suggest("=");
                }
            }
            PredicateSuggestions::PropertyValue(key) => {
                // addSuggestions supplies every possible value, without prefix filtering.
                for value in self.blocks().flat_map(|block| property_values(block, key)) {
                    builder = builder.suggest(value);
                }
                if self.block.is_none() {
                    if self
                        .blocks()
                        .flat_map(property_names)
                        .any(|name| !self.properties.contains_key(name))
                    {
                        builder = builder.suggest(",");
                    }
                    builder = builder.suggest("]");
                }
            }
            PredicateSuggestions::NextPropertyOrEnd => {
                if builder.remaining().is_empty() {
                    builder = builder.suggest("]");
                    if self
                        .blocks()
                        .flat_map(property_names)
                        .any(|name| !self.properties.contains_key(name))
                    {
                        builder = builder.suggest(",");
                    }
                }
            }
            PredicateSuggestions::Nothing => {}
        }
        builder.build()
    }
}

fn property_names(block: &Block) -> Vec<&'static str> {
    block
        .properties(block.default_state.id)
        .map(|props| props.to_props().into_iter().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

/// Keeps the existing canonical registry-string admission; Property.getValue aliases are open.
fn property_values(block: &Block, key: &str) -> BTreeSet<&'static str> {
    block
        .states
        .iter()
        .filter_map(|state| block.properties(state.id))
        .flat_map(|props| props.to_props())
        .filter_map(|(name, value)| (name == key).then_some(value))
        .collect()
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
        let start = reader.cursor();
        let result = BlockPredicateParser::default().parse(reader);
        if result.is_err() {
            reader.set_cursor(start);
        }
        result
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
        let mut reader = StringReader::new(builder.input.as_str());
        reader.set_cursor(builder.start);
        let mut parser = BlockPredicateParser::default();
        let _ = parser.parse(&mut reader);
        parser.suggest(builder.create_offset(reader.cursor()))
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
            &mut StringReader::new(format!("{{id:'minecraft:chest',x:9,y:64,z:-3,{fields}}}")),
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

    #[test]
    fn parsed_float_and_double_predicates_distinguish_signed_zero_and_width() {
        // These Pumpkin parser fixtures do not establish vanilla parsing behavior.
        let source = state("chest");
        let values = ["0.0f", "-0.0f", "0.0d", "-0.0d"];
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
    fn typed_nonfinite_fixtures_use_record_equality_in_nested_compounds_and_lists() {
        use NbtTag::{Double, Float};

        let source = state("chest");
        // Typed fixtures for the normative record contract, not runtime goldens or SNBT input.
        let float_nan = f32::from_bits(0x7fc0_0001);
        let double_nan = f64::from_bits(0x7ff8_0000_0000_0001);
        for (pattern, target, expected) in [
            (Float(float_nan), Float(float_nan), true),
            (Float(float_nan), Float(f32::from_bits(0x7fc0_0042)), true),
            (Float(float_nan), Float(f32::from_bits(0xffc0_0042)), true),
            (Float(f32::INFINITY), Float(f32::INFINITY), true),
            (Float(f32::NEG_INFINITY), Float(f32::NEG_INFINITY), true),
            (Float(f32::INFINITY), Float(f32::NEG_INFINITY), false),
            (Float(f32::NEG_INFINITY), Float(f32::INFINITY), false),
            (Float(float_nan), Float(1.0), false),
            (Float(1.0), Float(float_nan), false),
            (Double(double_nan), Double(double_nan), true),
            (
                Double(double_nan),
                Double(f64::from_bits(0x7ff8_0000_0000_0042)),
                true,
            ),
            (
                Double(double_nan),
                Double(f64::from_bits(0xfff8_0000_0000_0042)),
                true,
            ),
            (Double(f64::INFINITY), Double(f64::INFINITY), true),
            (Double(f64::NEG_INFINITY), Double(f64::NEG_INFINITY), true),
            (Double(f64::INFINITY), Double(f64::NEG_INFINITY), false),
            (Double(f64::NEG_INFINITY), Double(f64::INFINITY), false),
            (Double(double_nan), Double(1.0), false),
            (Double(1.0), Double(double_nan), false),
            (Float(float_nan), Double(double_nan), false),
            (Double(double_nan), Float(float_nan), false),
            (Float(f32::INFINITY), Double(f64::INFINITY), false),
            (Double(f64::INFINITY), Float(f32::INFINITY), false),
        ] {
            let mut pattern_entry = NbtCompound::new();
            pattern_entry.put("value", pattern);
            let mut pattern_nbt = NbtCompound::new();
            pattern_nbt.put_list("values", vec![NbtTag::Compound(pattern_entry)]);
            let mut parsed = predicate("chest{}");
            let (BlockPredicate::Block { nbt, .. } | BlockPredicate::Tag { nbt, .. }) = &mut parsed;
            *nbt = Some(pattern_nbt);

            let mut target_entry = NbtCompound::new();
            target_entry.put("value", target);
            let mut actual = chest_nbt("");
            actual.put_list("values", vec![NbtTag::Compound(target_entry)]);
            assert_eq!(
                parsed.test_state_with_nbt(source, Some(&actual)),
                expected,
                "pattern {parsed:?}, target {actual:?}"
            );
        }
    }
}
