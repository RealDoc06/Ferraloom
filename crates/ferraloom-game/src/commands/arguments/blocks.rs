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
        let mut state = block.default_state();
        let mut properties = BTreeMap::new();
        if reader.can_read() && reader.peek() == 91 {
            reader.skip();
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
                if !reader.can_read() || reader.peek() != 61 {
                    return Err(error_at(
                        reader,
                        "argument.block.property.novalue",
                        &[key, id.clone()],
                    ));
                }
                reader.skip();
                reader.skip_whitespace();
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
                properties.insert(key, value);
                state = BlockState::from_properties(
                    block,
                    properties.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                )
                .unwrap();
                reader.skip_whitespace();
                if reader.can_read() {
                    if reader.peek() == 44 {
                        reader.skip();
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
        }
        let nbt = if reader.can_read() && reader.peek() == 123 {
            Some(nbt(reader, true)?)
        } else {
            None
        };
        Ok(BlockInput {
            state,
            properties,
            nbt,
        })
    })();
    if result.is_err() {
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
    pub fn matches(&self, state: BlockState, block_entity: Option<&Tag>) -> bool {
        let (block_matches, properties, expected_nbt) = match self {
            Self::Block(input) => (
                input.state.block() == state.block(),
                &input.properties,
                &input.nbt,
            ),
            Self::Tag {
                blocks,
                properties,
                nbt,
                ..
            } => (blocks.contains(&state.block()), properties, nbt),
        };
        block_matches
            && properties.iter().all(|(key, value)| {
                state
                    .properties()
                    .any(|(name, actual)| name == key && actual == value)
            })
            && expected_nbt.as_ref().is_none_or(|expected| {
                ferraloom_nbt::compare_nbt(Some(expected), block_entity, true)
            })
    }
}
pub fn predicate(
    reader: &mut StringReader,
    context: &crate::commands::context::CommandBuildContext,
) -> Result<BlockPredicate, CommandSyntaxException> {
    if !reader.can_read() || reader.peek() != 35 {
        return parse(reader).map(BlockPredicate::Block);
    }
    let start = reader.cursor();
    let parsed = (|| {
        reader.skip();
        let name = identifier(reader)?;
        let blocks = context
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
        let mut properties = BTreeMap::new();
        if reader.can_read() && reader.peek() == 91 {
            reader.skip();
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
                value_start = Some(reader.cursor());
                let value = reader.read_string()?;
                properties.insert(key, value);
                reader.skip_whitespace();
                if reader.can_read() {
                    value_start = None;
                    match reader.peek() {
                        44 => reader.skip(),
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
        }
        let nbt = if reader.can_read() && reader.peek() == 123 {
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
    if parsed.is_err() {
        reader.set_cursor(start);
    }
    parsed
}
