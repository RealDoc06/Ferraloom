//! DataCommands. Accessors retain captured handles through exclusive execution.
mod manipulation;
pub mod text;
use super::{
    PreparedCommand,
    arguments::ArgumentValue,
    builtins::{CommandResult, component, integer, string},
    exceptions::error,
    storage::StorageReadError,
};
use crate::{block::entity::BlockEntityRef, entity::EntityRef, level::LevelError};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_nbt::{CompoundTag, NbtPath, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::{Argument, Component},
};
use ferraloom_util::BlockPos;
use manipulation::{Operation, path_error};

#[derive(Clone, Debug)]
pub enum DataAccessor {
    Block {
        entity: BlockEntityRef,
        pos: BlockPos,
    },
    Entity(EntityRef),
    Storage(Identifier),
}
#[derive(Debug)]
pub enum DataError {
    Syntax(CommandSyntaxException),
    Level(LevelError),
    Storage(StorageReadError),
    Serialization(ferraloom_protocol::Error),
}
impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
            Self::Level(error) => error.fmt(f),
            Self::Storage(error) => error.fmt(f),
            Self::Serialization(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for DataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::Level(error) => error,
            Self::Storage(error) => error,
            Self::Serialization(error) => error,
        })
    }
}
impl From<CommandSyntaxException> for DataError {
    fn from(error: CommandSyntaxException) -> Self {
        Self::Syntax(error)
    }
}
impl From<LevelError> for DataError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl From<StorageReadError> for DataError {
    fn from(error: StorageReadError) -> Self {
        Self::Storage(error)
    }
}
impl From<ferraloom_protocol::Error> for DataError {
    fn from(error: ferraloom_protocol::Error) -> Self {
        Self::Serialization(error)
    }
}

/// Captured block handles stay rooted until the host drops. Every method runs
/// synchronously in the command's exclusive call frame, including failed writes.
pub trait DataAccess {
    fn resolve(
        &mut self,
        command: &PreparedCommand,
        kind: &str,
        argument: &str,
    ) -> Result<DataAccessor, DataError>;
    fn get_data(&mut self, accessor: &DataAccessor) -> Result<CompoundTag, DataError>;
    fn set_data(&mut self, accessor: &DataAccessor, value: CompoundTag) -> Result<(), DataError>;
    /// Storage#get aliases an existing compound. Reflect partial mutations even
    /// on failure, without creating a missing key or marking SavedData dirty.
    fn publish_read_mutation(
        &mut self,
        accessor: &DataAccessor,
        value: &CompoundTag,
    ) -> Result<(), DataError>;
    fn display_name(&self, entity: EntityRef) -> Result<Component, DataError>;
}
impl DataAccessor {
    fn identity(&self, host: &impl DataAccess) -> Result<(&'static str, Vec<Argument>), DataError> {
        Ok(match self {
            Self::Block { pos, .. } => (
                "block",
                vec![integer(pos.x), integer(pos.y), integer(pos.z)],
            ),
            Self::Entity(entity) => ("entity", vec![component(host.display_name(*entity)?)]),
            Self::Storage(id) => ("storage", vec![string(id.to_string())]),
        })
    }
    fn feedback(
        &self,
        host: &impl DataAccess,
        operation: &str,
        tail: Vec<Argument>,
    ) -> Result<Component, DataError> {
        let (kind, mut arguments) = self.identity(host)?;
        arguments.extend(tail);
        Ok(Component::translatable(
            format!("commands.data.{kind}.{operation}"),
            arguments,
        ))
    }
}
fn path<'a>(command: &'a PreparedCommand, name: &str) -> &'a NbtPath {
    let Some(ArgumentValue::Path(path)) = command.argument(name) else {
        unreachable!("parsed NBT path")
    };
    path
}
fn source(command: &PreparedCommand, host: &mut impl DataAccess) -> Result<Vec<Tag>, DataError> {
    if command.literals.iter().any(|literal| literal == "value") {
        let Some(ArgumentValue::Tag(value)) = command.argument("value") else {
            unreachable!("parsed NBT value")
        };
        return Ok(vec![value.clone()]);
    }
    let Some((index, operation)) = command
        .literals
        .iter()
        .enumerate()
        .find(|(_, literal)| matches!(literal.as_str(), "from" | "string"))
    else {
        return Err(error("ferraloom.command.pending", &["data modify compute".into()]).into());
    };
    let accessor = host.resolve(command, &command.literals[index + 1], "source")?;
    let data = Tag::Compound(host.get_data(&accessor)?);
    let values = if let Some(ArgumentValue::Path(path)) = command.argument("sourcePath") {
        path.get(&data).map_err(path_error)?
    } else {
        vec![data]
    };
    if operation == "string" {
        Ok(manipulation::stringify(
            values,
            command.integer("start"),
            command.integer("end"),
        )?)
    } else {
        Ok(values)
    }
}
/// Port of DataCommands' get/merge/remove/modify callbacks. Resolves source
/// accessors before target accessors, at the native execution capture point.
pub fn execute(
    command: &PreparedCommand,
    host: &mut impl DataAccess,
) -> Result<CommandResult, DataError> {
    let operation = command.literals[1].as_str();
    let sources = if operation == "modify" {
        source(command, host)?
    } else {
        Vec::new()
    };
    let accessor = host.resolve(command, &command.literals[2], "target")?;
    let data = host.get_data(&accessor)?;
    match operation {
        "get" => {
            let (value, message) = if command.argument("path").is_some() {
                let path = path(command, "path");
                let tags = path.get(&Tag::Compound(data)).map_err(path_error)?;
                if tags.len() != 1 {
                    return Err(error("commands.data.get.multiple", &[]).into());
                }
                let tag = &tags[0];
                if let Some(ArgumentValue::Double(scale)) = command.argument("scale") {
                    let number = tag.as_double().ok_or_else(|| {
                        error("commands.data.get.invalid", &[path.as_str().into()])
                    })?;
                    let result = ferraloom_util::mth::floor(number * scale);
                    let (kind, mut arguments) = accessor.identity(host)?;
                    arguments.insert(0, string(path.as_str()));
                    arguments.push(string(scale_text(*scale)));
                    arguments.push(integer(result));
                    (
                        result,
                        Component::translatable(format!("commands.data.{kind}.get"), arguments),
                    )
                } else {
                    let result = manipulation::size(tag).ok_or_else(|| {
                        error("commands.data.get.unknown", &[path.as_str().into()])
                    })?;
                    (
                        result,
                        accessor.feedback(
                            host,
                            "query",
                            vec![component(text::pretty_component(tag)?)],
                        )?,
                    )
                }
            } else {
                (
                    1,
                    accessor.feedback(
                        host,
                        "query",
                        vec![component(text::pretty_component(&Tag::Compound(data))?)],
                    )?,
                )
            };
            Ok(CommandResult::success(value, message, false))
        }
        "merge" => {
            let Some(ArgumentValue::Tag(Tag::Compound(nbt))) = command.argument("nbt") else {
                unreachable!("parsed NBT compound")
            };
            if NbtPath::is_too_deep(&Tag::Compound(nbt.clone()), 0) {
                return Err(path_error(ferraloom_nbt::PathError::TooDeep).into());
            }
            let mut result = data.copy();
            result.merge(nbt);
            if result == data {
                return Err(error("commands.data.merge.failed", &[]).into());
            }
            host.set_data(&accessor, result)?;
            Ok(CommandResult::success(
                1,
                accessor.feedback(host, "modified", vec![])?,
                true,
            ))
        }
        "remove" | "modify" => {
            let mut root = Tag::Compound(data);
            let result = if operation == "remove" {
                Ok(path(command, "path").remove(&mut root) as i32)
            } else {
                let manipulation = match command.literals[3].as_str() {
                    "set" => Operation::Set,
                    "merge" => Operation::Merge,
                    "append" => Operation::Insert(-1),
                    "prepend" => Operation::Insert(0),
                    "insert" => {
                        Operation::Insert(command.integer("index").expect("parsed insert index"))
                    }
                    _ => unreachable!("data manipulator"),
                };
                manipulation::modify(
                    &mut root,
                    path(command, "targetPath"),
                    &sources,
                    manipulation,
                )
            };
            let Tag::Compound(data) = root else {
                unreachable!("compound root")
            };
            host.publish_read_mutation(&accessor, &data)?;
            let count = result?;
            if count == 0 {
                return Err(error("commands.data.merge.failed", &[]).into());
            }
            host.set_data(&accessor, data)?;
            Ok(CommandResult::success(
                count,
                accessor.feedback(host, "modified", vec![])?,
                true,
            ))
        }
        _ => unreachable!("data command operation"),
    }
}

/// String.format(Locale.ROOT, "%.2f", scale) rounds the native shortest decimal
/// representation using HALF_UP, rather than rounding the binary f64 value.
pub fn scale_text(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value.is_sign_negative() {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let text = Tag::Double(value.abs()).to_string();
    let text = text.strip_suffix('d').unwrap_or(&text);
    let (mantissa, exponent) =
        text.split_once(['E', 'e'])
            .map_or((text, 0), |(mantissa, exponent)| {
                (
                    mantissa,
                    exponent.parse::<i32>().expect("native decimal exponent"),
                )
            });
    let mut digits = mantissa
        .bytes()
        .filter(|byte| *byte != b'.')
        .collect::<Vec<_>>();
    let decimal = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let retained = decimal + 2;
    if retained < 0 {
        digits = vec![b'0'];
    } else {
        let retained = retained as usize;
        let round = digits.get(retained).is_some_and(|digit| *digit >= b'5');
        digits.resize(digits.len().max(retained), b'0');
        digits.truncate(retained);
        if round {
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                if *digit == b'9' {
                    *digit = b'0';
                } else {
                    *digit += 1;
                    carry = false;
                    break;
                }
            }
            if carry {
                digits.insert(0, b'1');
            }
        }
        if digits.is_empty() {
            digits.push(b'0');
        }
    }
    while digits.len() < 3 {
        digits.insert(0, b'0');
    }
    digits.insert(digits.len() - 2, b'.');
    if value.is_sign_negative() {
        digits.insert(0, b'-');
    }
    String::from_utf8(digits).expect("decimal digits")
}

#[cfg(test)]
mod tests;
