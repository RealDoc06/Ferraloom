//! NbtUtils#toPrettyComponent's default rich, unindented TextComponentTagVisitor.
use ferraloom_nbt::{NbtString, Tag};
use ferraloom_protocol::{
    Error, Result,
    chat::{Component, TextColor},
};

fn literal(text: impl Into<String>, color: Option<&str>) -> Result<Component> {
    let mut component = Component::literal(text);
    component.style.color = color.map(TextColor::parse).transpose()?;
    Ok(component)
}
fn string(value: &NbtString) -> Result<String> {
    value
        .to_string_checked()
        .map_err(|error| Error::invalid("NBT text component", error.to_string()))
}
fn token(result: &mut Component, value: &str) -> Result<()> {
    let color = match value {
        "b" | "B" | "s" | "I" | "L" | "f" | "d" => Some("red"),
        "<...>" => Some("gray"),
        _ => None,
    };
    result.extra.push(literal(value, color)?);
    Ok(())
}
fn quoted(value: &NbtString, color: &str) -> Result<Vec<Component>> {
    let value = string(&ferraloom_nbt::quote_utf16(value))?;
    let quote = &value[..1];
    Ok(vec![
        literal(quote, None)?,
        literal(&value[1..value.len() - 1], Some(color))?,
        literal(quote, None)?,
    ])
}
fn key(value: &NbtString) -> Result<Component> {
    let text = string(value)?;
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
    {
        literal(text, Some("aqua"))
    } else {
        let mut parts = quoted(value, "aqua")?.into_iter();
        let mut result = parts.next().expect("quoted key");
        result.extra.extend(parts);
        Ok(result)
    }
}
fn number(result: &mut Component, text: String, suffix: Option<&str>) -> Result<()> {
    result.extra.push(literal(text, Some("gold"))?);
    if let Some(suffix) = suffix {
        token(result, suffix)?;
    }
    Ok(())
}
fn array<T: ToString>(
    result: &mut Component,
    values: &[T],
    prefix: &str,
    suffix: Option<&str>,
) -> Result<()> {
    token(result, "[")?;
    token(result, prefix)?;
    token(result, ";")?;
    for (index, value) in values.iter().take(128).enumerate() {
        result.extra.push(literal(" ", None)?);
        number(result, value.to_string(), suffix)?;
        if index + 1 != values.len() {
            token(result, ",")?;
        }
    }
    if values.len() > 128 {
        token(result, "<...>")?;
    }
    token(result, "]")
}
fn visit(result: &mut Component, tag: &Tag, depth: usize) -> Result<()> {
    match tag {
        Tag::End => {}
        Tag::String(value) => result.extra.extend(quoted(value, "green")?),
        Tag::Byte(value) => number(result, value.to_string(), Some("b"))?,
        Tag::Short(value) => number(result, value.to_string(), Some("s"))?,
        Tag::Int(value) => number(result, value.to_string(), None)?,
        Tag::Long(value) => number(result, value.to_string(), Some("L"))?,
        Tag::Float(_) | Tag::Double(_) => {
            let mut text = ferraloom_nbt::to_text(tag, "", false);
            text.pop();
            number(
                result,
                text,
                Some(if matches!(tag, Tag::Float(_)) {
                    "f"
                } else {
                    "d"
                }),
            )?;
        }
        Tag::ByteArray(values) => array(result, values, "B", Some("b"))?,
        Tag::IntArray(values) => array(result, values, "I", None)?,
        Tag::LongArray(values) => array(result, values, "L", Some("L"))?,
        Tag::List(values) => {
            token(result, "[")?;
            if !values.is_empty() && depth >= 64 {
                token(result, "<...>")?;
            } else {
                let wrap = values.len() < 8 && values.iter().any(|v| v.as_double().is_none());
                for (index, value) in values.iter().enumerate() {
                    if wrap {
                        result.extra.push(Component::default());
                    }
                    visit(result, value, depth + 1)?;
                    if index + 1 != values.len() {
                        token(result, ",")?;
                        result.extra.push(literal(" ", None)?);
                    }
                }
            }
            token(result, "]")?;
        }
        Tag::Compound(values) => {
            token(result, "{")?;
            if !values.is_empty() && depth >= 64 {
                token(result, "<...>")?;
            } else {
                for (index, (name, value)) in values.iter_java_order().enumerate() {
                    result.extra.push(Component::default());
                    result.extra.push(key(name)?);
                    token(result, ":")?;
                    result.extra.push(literal(" ", None)?);
                    visit(result, value, depth + 1)?;
                    if index + 1 != values.len() {
                        token(result, ",")?;
                        result.extra.push(literal(" ", None)?);
                    }
                }
            }
            token(result, "}")?;
        }
    }
    Ok(())
}
/// Port of NbtUtils#toPrettyComponent. Default server logging leaves key sorting
/// disabled; compound entries retain the native JDK map traversal order.
pub fn pretty_component(tag: &Tag) -> Result<Component> {
    let mut result = Component::default();
    visit(&mut result, tag, 0)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_rich_nbt_feedback_components_match() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../vanilla/data-text-vectors.json")).unwrap();
        for row in &rows {
            let tag = row["input"]
                .as_str()
                .map_or(Tag::End, |s| ferraloom_nbt::parse(s).unwrap());
            let actual: serde_json::Value =
                serde_json::from_str(&pretty_component(&tag).unwrap().to_json().unwrap()).unwrap();
            assert_eq!(actual, row["component"], "{}", row["input"]);
        }
        assert_eq!(rows.len(), 39);
    }
}
