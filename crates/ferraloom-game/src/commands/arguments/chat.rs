//! ComponentArgument and StyleArgument adapt the SNBT grammar and chat codecs.
use super::nbt;
use crate::commands::exceptions::error_at;
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::chat::{Component, Style};

/// Port of CommandArgumentParser#withCodec: codec errors rewind before adding context.
fn parse<T>(
    reader: &mut StringReader,
    key: &str,
    decode: impl FnOnce(Tag) -> ferraloom_protocol::Result<T>,
    diagnose: impl FnOnce(&Tag) -> String,
) -> Result<T, CommandSyntaxException> {
    let start = reader.cursor();
    let tag = nbt(reader, false)?;
    match decode(tag.clone()) {
        Ok(value) => Ok(value),
        Err(_) => {
            reader.set_cursor(start);
            Err(error_at(reader, key, &[diagnose(&tag)]))
        }
    }
}
pub fn component(reader: &mut StringReader) -> Result<Component, CommandSyntaxException> {
    parse(
        reader,
        "argument.component.invalid",
        Component::from_nbt,
        |tag| component_error(tag).0,
    )
}
pub fn style(reader: &mut StringReader) -> Result<Style, CommandSyntaxException> {
    parse(
        reader,
        "argument.style.invalid",
        Style::from_nbt,
        style_error,
    )
}

fn either(first: &str, second: &str) -> String {
    format!("Failed to parse either. First: {first}; Second: {second}")
}
/// Codec.either preserves a failing branch's partial result instead of adding either's errors.
fn component_error(tag: &Tag) -> (String, bool) {
    let (list, partial) = match tag {
        Tag::List(values) => component_list_error(&values.0),
        _ => (format!("Not a list: {tag}"), false),
    };
    if partial {
        return (list, true);
    }
    let full = if let Tag::Compound(map) = tag {
        let mut contents = map.clone();
        contents.remove("extra");
        for key in STYLE_FIELDS {
            contents.remove(key);
        }
        if Component::from_nbt(Tag::Compound(contents)).is_ok() {
            let mut errors = Vec::new();
            if let Some(extra) = map.get("extra") {
                match extra {
                    Tag::List(values) => {
                        if values.0.is_empty()
                            || values
                                .0
                                .iter()
                                .any(|value| Component::from_nbt(value.clone()).is_err())
                        {
                            errors.push(component_list_error(&values.0).0);
                        }
                    }
                    _ => errors.push(format!("Not a list: {extra}")),
                }
            }
            errors.extend(style_field_errors(map));
            return (errors.join("; "), true);
        }
        match map.get("type") {
            Some(Tag::String(kind)) => {
                let kind = kind.to_string_lossy();
                let field = match kind.as_str() {
                    "text" => Some("text"),
                    "translatable" => Some("translate"),
                    "keybind" => Some("keybind"),
                    "score" => Some("score"),
                    "selector" => Some("selector"),
                    "nbt" => Some("nbt"),
                    "object" => Some("object"),
                    _ => None,
                };
                match field {
                    Some(field) if !map.contains_key(field) => {
                        format!("No key {field} in MapLike[{tag}]")
                    }
                    Some(_) => "No matching codec found".into(),
                    None => format!("Unknown element id: {kind}"),
                }
            }
            Some(_) => "Not a string".into(),
            None => "No matching codec found".into(),
        }
    } else {
        format!("Not a map: {tag}")
    };
    (either(&either("Not a string", &list), &full), false)
}
fn component_list_error(values: &[Tag]) -> (String, bool) {
    let mut errors = Vec::new();
    let mut valid = 0;
    for value in values {
        if Component::from_nbt(value.clone()).is_ok() {
            valid += 1;
        } else {
            let (message, partial) = component_error(value);
            errors.push(message);
            valid += usize::from(partial);
        }
    }
    if valid == 0 {
        errors.push("List must have contents".into());
    }
    (errors.join("; "), valid != 0)
}
const STYLE_FIELDS: [&str; 11] = [
    "color",
    "shadow_color",
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
    "click_event",
    "hover_event",
    "insertion",
    "font",
];
fn style_error(tag: &Tag) -> String {
    match tag {
        Tag::Compound(map) => style_field_errors(map).join("; "),
        _ => format!("Not a map: {tag}"),
    }
}
fn style_field_errors(map: &CompoundTag) -> Vec<String> {
    let mut errors = Vec::new();
    for field in STYLE_FIELDS {
        let Some(value) = map.get(field) else {
            continue;
        };
        let mut single = CompoundTag::new();
        single.insert(field, value.clone());
        if Style::from_nbt(Tag::Compound(single)).is_ok() {
            continue;
        }
        let message = match field {
            "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" => {
                "Not a number".into()
            }
            "color" => match value {
                Tag::String(text) => {
                    let text = text.to_string_lossy();
                    if let Some(hex) = text.strip_prefix('#') {
                        match i32::from_str_radix(hex, 16) {
                            Ok(_) => format!("Color value out of range: {text}"),
                            Err(_) => format!("Invalid color value: {text}"),
                        }
                    } else {
                        format!("Invalid color name: {text}")
                    }
                }
                _ => "Not a string".into(),
            },
            "insertion" | "font" => "Not a string".into(),
            _ => format!("Invalid {field}: {value}"),
        };
        errors.push(message);
    }
    errors
}
