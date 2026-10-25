//! DataCommands' path operations. Sources are captured before resolving the target.
use crate::commands::{
    builtins::integer,
    exceptions::{error, error_with_arguments},
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_nbt::{CompoundTag, NbtPath, NbtString, PathError, Tag};

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Set,
    Insert(i32),
    Merge,
}

/// Port of NbtPathArgument's runtime exception types, without parser context.
pub fn path_error(cause: PathError) -> CommandSyntaxException {
    match cause {
        PathError::Syntax(_) => error("arguments.nbtpath.node.invalid", &[]),
        PathError::NothingFound(path) => error("arguments.nbtpath.nothing_found", &[path]),
        PathError::ExpectedList(tag) => {
            error("commands.data.modify.expected_list", &[tag.to_string()])
        }
        PathError::InvalidIndex(index) => {
            error_with_arguments("commands.data.modify.invalid_index", vec![integer(index)])
        }
        PathError::TooDeep => error("arguments.nbtpath.too_deep", &[]),
    }
}

/// Port of DataCommands#decorateModification's three native manipulators.
/// The caller retains partial changes even when a later target fails.
pub fn modify(
    target: &mut Tag,
    path: &NbtPath,
    source: &[Tag],
    operation: Operation,
) -> Result<i32, CommandSyntaxException> {
    match operation {
        Operation::Set => path
            .set(target, source.last().expect("nonempty captured source"))
            .map(|count| count as i32)
            .map_err(path_error),
        Operation::Insert(index) => path
            .insert(index, target, source)
            .map(|count| count as i32)
            .map_err(path_error),
        Operation::Merge => {
            // All sources are validated and combined before selecting targets.
            let mut combined = CompoundTag::new();
            for tag in source {
                if NbtPath::is_too_deep(tag, 0) {
                    return Err(path_error(PathError::TooDeep));
                }
                let Tag::Compound(compound) = tag else {
                    return Err(error(
                        "commands.data.modify.expected_object",
                        &[tag.to_string()],
                    ));
                };
                combined.merge(compound);
            }
            let mut result = Ok(0);
            path.get_or_create_and_update(target, &Tag::Compound(CompoundTag::new()), |tag| {
                let Ok(count) = &mut result else { return };
                if let Tag::Compound(compound) = tag {
                    let original = compound.copy();
                    compound.merge(&combined);
                    *count += i32::from(original != *compound);
                } else {
                    result = Err(error(
                        "commands.data.modify.expected_object",
                        &[tag.to_string()],
                    ));
                }
            })
            .map_err(path_error)?;
            result
        }
    }
}

/// Port of DataCommands#getAsText and #stringifyTagList, retaining UTF-16 indices.
pub fn stringify(
    values: Vec<Tag>,
    start: Option<i32>,
    end: Option<i32>,
) -> Result<Vec<Tag>, CommandSyntaxException> {
    values
        .into_iter()
        .map(|tag| {
            let text = match tag {
                Tag::String(text) => text,
                Tag::Byte(_)
                | Tag::Short(_)
                | Tag::Int(_)
                | Tag::Long(_)
                | Tag::Float(_)
                | Tag::Double(_) => NbtString::from(tag.to_string()),
                _ => {
                    return Err(error(
                        "commands.data.modify.expected_value",
                        &[tag.to_string()],
                    ));
                }
            };
            let Some(start) = start else {
                return Ok(Tag::String(text));
            };
            let length = text.len() as i32;
            let offset = |index: i32| {
                if index >= 0 {
                    index
                } else {
                    length.wrapping_add(index)
                }
            };
            let start = offset(start);
            let end = end.map_or(length, offset);
            if start < 0 || end > length || start > end {
                return Err(error_with_arguments(
                    "commands.data.modify.invalid_substring",
                    vec![integer(start), integer(end)],
                ));
            }
            Ok(Tag::String(NbtString::from_utf16(
                text.as_utf16()[start as usize..end as usize].to_vec(),
            )))
        })
        .collect()
}

/// Port of DataCommands#getData(path)'s command result, including UTF-16 length.
pub fn size(tag: &Tag) -> Option<i32> {
    Some(match tag {
        Tag::End => return None,
        Tag::ByteArray(values) => values.len() as i32,
        Tag::IntArray(values) => values.len() as i32,
        Tag::LongArray(values) => values.len() as i32,
        Tag::List(values) => values.len() as i32,
        Tag::Compound(values) => values.len() as i32,
        Tag::String(text) => text.len() as i32,
        _ => ferraloom_util::mth::floor(tag.as_double().expect("numeric tag")),
    })
}
