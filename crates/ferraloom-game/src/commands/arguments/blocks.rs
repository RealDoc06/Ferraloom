//! Port of BlockStateParser, retaining which properties the command explicitly supplied.
use super::{identifier, nbt};
use crate::commands::exceptions::error_at;
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::Tag;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct BlockInput {
    pub state: BlockState,
    pub properties: BTreeMap<String, String>,
    pub nbt: Option<Tag>,
}
pub fn parse(reader: &mut StringReader) -> Result<BlockInput, CommandSyntaxException> {
    parse_inner(reader, &mut Completion::default(), true)
}
#[derive(Clone, Default)]
enum CompletionMode {
    #[default]
    Block,
    Tag,
    Open,
    Nbt,
    Property {
        allow_end: bool,
    },
    Equals,
    Value(String),
    Next,
    None,
}
#[derive(Default)]
struct Completion {
    enabled: bool,
    vague: bool,
    mode: CompletionMode,
    blocks: Vec<Block>,
    properties: BTreeMap<String, String>,
}
impl Completion {
    fn block(&mut self, block: Block) {
        if self.enabled {
            self.blocks = vec![block];
        }
    }
    fn property(&mut self, key: &str, value: &str) {
        if self.enabled {
            self.properties.insert(key.into(), value.into());
        }
    }
}
fn parse_inner(
    reader: &mut StringReader,
    completion: &mut Completion,
    rewind: bool,
) -> Result<BlockInput, CommandSyntaxException> {
    let start = reader.cursor();
    let result = (|| {
        if reader.can_read() && reader.peek() == 35 {
            return Err(error_at(reader, "argument.block.tag.disallowed", &[]));
        }
        let id = identifier(reader)?;
        let block = Block::from_name(&id).ok_or_else(|| {
            reader.set_cursor(start);
            error_at(
                reader,
                "argument.block.id.invalid",
                std::slice::from_ref(&id),
            )
        })?;
        completion.block(block);
        completion.mode = CompletionMode::Open;
        let mut state = block.default_state();
        let mut properties = BTreeMap::new();
        if reader.can_read() && reader.peek() == 91 {
            reader.skip();
            completion.mode = CompletionMode::Property { allow_end: true };
            reader.skip_whitespace();
            while reader.can_read() && reader.peek() != 93 {
                reader.skip_whitespace();
                let key_start = reader.cursor();
                let key = reader.read_string()?;
                if !state.properties().any(|(name, _)| name == key) {
                    reader.set_cursor(key_start);
                    return Err(error_at(
                        reader,
                        "argument.block.property.unknown",
                        &[id.clone(), key],
                    ));
                }
                if properties.contains_key(&key) {
                    reader.set_cursor(key_start);
                    return Err(error_at(
                        reader,
                        "argument.block.property.duplicate",
                        &[key, id.clone()],
                    ));
                }
                reader.skip_whitespace();
                completion.mode = CompletionMode::Equals;
                if !reader.can_read() || reader.peek() != 61 {
                    return Err(error_at(
                        reader,
                        "argument.block.property.novalue",
                        &[key, id.clone()],
                    ));
                }
                reader.skip();
                reader.skip_whitespace();
                completion.mode = CompletionMode::Value(key.clone());
                let value_start = reader.cursor();
                let value = reader.read_string()?;
                let data = block.data();
                let valid = (data.first_state..data.first_state + data.state_count)
                    .filter_map(BlockState::from_id)
                    .any(|s| s.properties().any(|(k, v)| k == key && v == value));
                if !valid {
                    reader.set_cursor(value_start);
                    return Err(error_at(
                        reader,
                        "argument.block.property.invalid",
                        &[id.clone(), value, key],
                    ));
                }
                completion.property(&key, &value);
                properties.insert(key, value);
                completion.mode = CompletionMode::Next;
                state = BlockState::from_properties(
                    block,
                    properties.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                )
                .unwrap();
                reader.skip_whitespace();
                if reader.can_read() {
                    if reader.peek() == 44 {
                        reader.skip();
                        completion.mode = CompletionMode::Property { allow_end: false };
                    } else if reader.peek() == 93 {
                        break;
                    } else {
                        return Err(error_at(reader, "argument.block.property.unclosed", &[]));
                    }
                }
            }
            if !reader.can_read() {
                return Err(error_at(reader, "argument.block.property.unclosed", &[]));
            }
            reader.skip();
            completion.mode = CompletionMode::Nbt;
        }
        let nbt = if reader.can_read() && reader.peek() == 123 {
            {
                completion.mode = CompletionMode::None;
                Some(nbt(reader, true)?)
            }
        } else {
            None
        };
        Ok(BlockInput {
            state,
            properties,
            nbt,
        })
    })();
    if rewind && result.is_err() {
        reader.set_cursor(start);
    }
    result
}

/// Port of BlockPredicateArgument's concrete-block and tag predicate alternatives.
#[derive(Clone, Debug)]
pub enum BlockPredicate {
    Block(BlockInput),
    Tag {
        name: String,
        blocks: Vec<Block>,
        properties: BTreeMap<String, String>,
        nbt: Option<Tag>,
    },
}
impl BlockPredicate {
    /// Port of BlockPredicateArgument.Result#requiresNbt.
    pub fn requires_nbt(&self) -> bool {
        self.expected_nbt().is_some()
    }
    fn expected_nbt(&self) -> Option<&Tag> {
        match self {
            Self::Block(input) => input.nbt.as_ref(),
            Self::Tag { nbt, .. } => nbt.as_ref(),
        }
    }
    pub fn matches_state(&self, state: BlockState) -> bool {
        let (block_matches, properties) = match self {
            Self::Block(input) => (input.state.block() == state.block(), &input.properties),
            Self::Tag {
                blocks, properties, ..
            } => (blocks.contains(&state.block()), properties),
        };
        block_matches
            && properties.iter().all(|(key, value)| {
                state
                    .properties()
                    .any(|(name, actual)| name == key && actual == value)
            })
    }
    pub fn matches(&self, state: BlockState, block_entity: Option<&Tag>) -> bool {
        self.matches_state(state)
            && self.expected_nbt().is_none_or(|expected| {
                ferraloom_nbt::compare_nbt(Some(expected), block_entity, true)
            })
    }
}

pub fn predicate(
    reader: &mut StringReader,
    context: &crate::commands::context::CommandBuildContext,
) -> Result<BlockPredicate, CommandSyntaxException> {
    predicate_inner(reader, context, &mut Completion::default(), true)
}
fn predicate_inner(
    reader: &mut StringReader,
    context: &crate::commands::context::CommandBuildContext,
    completion: &mut Completion,
    rewind: bool,
) -> Result<BlockPredicate, CommandSyntaxException> {
    if !reader.can_read() || reader.peek() != 35 {
        return parse_inner(reader, completion, rewind).map(BlockPredicate::Block);
    }
    completion.vague = true;
    completion.mode = CompletionMode::Tag;
    let start = reader.cursor();
    let parsed = (|| {
        reader.skip();
        let name = identifier(reader)?;
        let blocks: Vec<_> = context
            .tag("minecraft:block", &name)
            .ok_or_else(|| {
                reader.set_cursor(start);
                error_at(
                    reader,
                    "arguments.block.tag.unknown",
                    std::slice::from_ref(&name),
                )
            })?
            .iter()
            .filter_map(|name| Block::from_name(name))
            .collect();
        if completion.enabled {
            completion.blocks = blocks.clone();
        }
        completion.mode = CompletionMode::Open;
        let mut properties = BTreeMap::new();
        if reader.can_read() && reader.peek() == 91 {
            reader.skip();
            completion.mode = CompletionMode::Property { allow_end: true };
            let mut value_start = None;
            reader.skip_whitespace();
            while reader.can_read() && reader.peek() != 93 {
                reader.skip_whitespace();
                let key_start = reader.cursor();
                let key = reader.read_string()?;
                // The tag parser intentionally leaves its concrete-block id at the empty default.
                if properties.contains_key(&key) {
                    reader.set_cursor(key_start);
                    return Err(error_at(
                        reader,
                        "argument.block.property.duplicate",
                        &[key, "minecraft:".into()],
                    ));
                }
                reader.skip_whitespace();
                if !reader.can_read() || reader.peek() != 61 {
                    reader.set_cursor(key_start);
                    return Err(error_at(
                        reader,
                        "argument.block.property.novalue",
                        &[key, "minecraft:".into()],
                    ));
                }
                reader.skip();
                reader.skip_whitespace();
                completion.mode = CompletionMode::Value(key.clone());
                value_start = Some(reader.cursor());
                let value = reader.read_string()?;
                completion.property(&key, &value);
                properties.insert(key, value);
                reader.skip_whitespace();
                if reader.can_read() {
                    value_start = None;
                    match reader.peek() {
                        44 => {
                            reader.skip();
                            completion.mode = CompletionMode::Property { allow_end: false };
                        }
                        93 => break,
                        _ => return Err(error_at(reader, "argument.block.property.unclosed", &[])),
                    }
                }
            }
            if !reader.can_read() {
                if let Some(start) = value_start {
                    reader.set_cursor(start);
                }
                return Err(error_at(reader, "argument.block.property.unclosed", &[]));
            }
            reader.skip();
            completion.mode = CompletionMode::Nbt;
        }
        let nbt = if reader.can_read() && reader.peek() == 123 {
            completion.mode = CompletionMode::None;
            Some(nbt(reader, true)?)
        } else {
            None
        };
        Ok(BlockPredicate::Tag {
            name,
            blocks,
            properties,
            nbt,
        })
    })();
    if rewind && parsed.is_err() {
        reader.set_cursor(start);
    }
    parsed
}

/// Port of BlockStateParser#fillSuggestions, including its retained error cursor.
pub fn suggestions(
    context: &crate::commands::context::CommandBuildContext,
    builder: ferraloom_brigadier::SuggestionsBuilder,
    for_testing: bool,
) -> ferraloom_brigadier::Suggestions {
    let mut reader = StringReader::new(&builder.input);
    reader.set_cursor(builder.start);
    let mut completion = Completion {
        enabled: true,
        ..Default::default()
    };
    if for_testing {
        let _ = predicate_inner(&mut reader, context, &mut completion, false);
    } else {
        let _ = parse_inner(&mut reader, &mut completion, false);
    }
    let mut builder = builder.create_offset(reader.cursor());
    let empty = builder.remaining().is_empty();
    let has_properties = completion
        .blocks
        .iter()
        .any(|block| block.default_state().properties().next().is_some());
    let has_entity = completion
        .blocks
        .iter()
        .any(|block| block.default_state().has_block_entity());
    match completion.mode.clone() {
        CompletionMode::Block | CompletionMode::Tag => {
            if matches!(completion.mode, CompletionMode::Block) {
                super::suggestions::resources(&mut builder, context.entries("minecraft:block"), "");
            }
            if for_testing {
                if let Some(tags) = context.tags.get("minecraft:block") {
                    super::suggestions::resources(
                        &mut builder,
                        tags.keys().map(String::as_str),
                        "#",
                    );
                }
            }
        }
        CompletionMode::Open if empty => {
            if has_properties {
                builder.suggest("[");
            }
            if has_entity {
                builder.suggest("{");
            }
        }
        CompletionMode::Nbt if empty && has_entity => {
            builder.suggest("{");
        }
        CompletionMode::Property { allow_end } => {
            if allow_end && empty {
                builder.suggest("]");
            }
            let remaining = builder.remaining_lower_case();
            for block in &completion.blocks {
                for (key, _) in block.default_state().properties() {
                    if !completion.properties.contains_key(key) && key.starts_with(&remaining) {
                        builder.suggest(format!("{key}="));
                    }
                }
            }
        }
        CompletionMode::Equals if empty => {
            builder.suggest("=");
        }
        CompletionMode::Value(ref key) => {
            for block in &completion.blocks {
                let data = block.data();
                let values = (data.first_state..data.first_state + data.state_count)
                    .filter_map(BlockState::from_id)
                    .flat_map(|state| state.properties())
                    .filter_map(|(name, value)| (name == key).then_some(value))
                    .collect::<std::collections::BTreeSet<_>>();
                for value in values {
                    if let Ok(value) = value.parse::<i32>() {
                        builder.suggest_integer(value);
                    } else {
                        builder.suggest(value);
                    }
                }
            }
            if completion.vague {
                if has_unused_property(&completion) {
                    builder.suggest(",");
                }
                builder.suggest("]");
            }
        }
        CompletionMode::Next if empty => {
            builder.suggest("]");
            if has_unused_property(&completion) {
                builder.suggest(",");
            }
        }
        _ => {}
    }
    builder.build()
}
fn has_unused_property(completion: &Completion) -> bool {
    completion.blocks.iter().any(|block| {
        block
            .default_state()
            .properties()
            .any(|(key, _)| !completion.properties.contains_key(key))
    })
}
