//! Port of ItemParser and ItemInput, sharing the protocol's persistent component codecs.
use super::{identifier, nbt};
use crate::commands::exceptions::{error, error_at};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::Item;
use ferraloom_protocol::{
    Identifier,
    component::{ComponentTypeId, DataComponent, DataComponentPatch},
};

#[derive(Clone, Debug)]
pub struct ItemInput {
    pub item: Item,
    pub components: DataComponentPatch,
}
pub fn parse(reader: &mut StringReader) -> Result<ItemInput, CommandSyntaxException> {
    let start = reader.cursor();
    let parsed = parse_inner(reader);
    if parsed.is_err() {
        reader.set_cursor(start);
    }
    parsed
}
fn parse_inner(reader: &mut StringReader) -> Result<ItemInput, CommandSyntaxException> {
    let start = reader.cursor();
    let name = identifier(reader)?;
    let item = Item::from_name(&name).ok_or_else(|| {
        reader.set_cursor(start);
        error_at(reader, "argument.item.id.invalid", &[name])
    })?;
    let mut components = DataComponentPatch::default();
    if reader.can_read() && reader.peek() == 91 {
        reader.skip();
        let mut visited = Vec::new();
        while reader.can_read() && reader.peek() != 93 {
            reader.skip_whitespace();
            let remove = reader.can_read() && reader.peek() == 33;
            if remove {
                reader.skip();
            }
            let (id, key) = component_type(reader)?;
            if visited.contains(&id) {
                return Err(error(
                    "arguments.item.component.repeated",
                    &[key.to_string()],
                ));
            }
            visited.push(id);
            if remove {
                components.remove(id);
            } else {
                reader.skip_whitespace();
                reader.expect(61)?;
                reader.skip_whitespace();
                let value_start = reader.cursor();
                let value = nbt(reader, false)?;
                let value = DataComponent::from_nbt(&key, &value).map_err(|failure| {
                    reader.set_cursor(value_start);
                    error_at(
                        reader,
                        "arguments.item.component.malformed",
                        &[key.to_string(), failure.to_string()],
                    )
                })?;
                components.set(value);
            }
            reader.skip_whitespace();
            if !reader.can_read() || reader.peek() != 44 {
                break;
            }
            reader.skip();
            reader.skip_whitespace();
            if !reader.can_read() {
                return Err(error_at(reader, "arguments.item.component.expected", &[]));
            }
        }
        reader.expect(93)?;
    }
    Ok(ItemInput { item, components })
}
fn component_type(
    reader: &mut StringReader,
) -> Result<(ComponentTypeId, Identifier), CommandSyntaxException> {
    if !reader.can_read() {
        return Err(error_at(reader, "arguments.item.component.expected", &[]));
    }
    let start = reader.cursor();
    let key = identifier(reader)?;
    let id = ComponentTypeId::from_name(&key).filter(|id| {
        !matches!(
            *id,
            ComponentTypeId::CREATIVE_SLOT_LOCK
                | ComponentTypeId::ADDITIONAL_TRADE_COST
                | ComponentTypeId::MAP_POST_PROCESSING
        )
    });
    let Some(id) = id else {
        reader.set_cursor(start);
        return Err(error_at(reader, "arguments.item.component.unknown", &[key]));
    };
    Ok((
        id,
        Identifier::parse(&key).expect("parsed component identifier"),
    ))
}
