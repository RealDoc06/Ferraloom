//! Port of ResourceOrIdArgument's identifier branch and SlotSourceArgument's range prefix.
use super::{identifier, nbt, slots, word};
use crate::commands::{
    context::CommandBuildContext,
    exceptions::{error, error_at},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryReference {
    pub registry: String,
    pub key: String,
}
pub fn parse_reference(
    reader: &mut StringReader,
    context: &CommandBuildContext,
    registry: &str,
) -> Result<RegistryReference, CommandSyntaxException> {
    let start = reader.cursor();
    reader.skip_whitespace();
    let id_start = reader.cursor();
    let parsed = identifier(reader);
    if reader.cursor() != id_start
        && let Ok(key) = parsed
    {
        if !context.contains(registry, &key) {
            return Err(error_at(
                reader,
                "argument.resource_or_id.no_such_element",
                &[key, registry.into()],
            ));
        }
        return Ok(RegistryReference {
            registry: registry.into(),
            key,
        });
    }
    // The grammar's SNBT alternative is parsed, but never accepted without its gameplay codec.
    reader.set_cursor(start);
    nbt(reader, false)?;
    Err(error(
        "ferraloom.argument.pending",
        &[format!("inline {registry}")],
    ))
}
pub fn try_slots(reader: &mut StringReader) -> Option<slots::SlotRange> {
    let start = reader.cursor();
    let name = word(reader);
    if let Some(slots) = slots::lookup(&name) {
        Some(slots::SlotRange { name, slots })
    } else {
        reader.set_cursor(start);
        None
    }
}
