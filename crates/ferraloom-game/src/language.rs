//! Port of Language's default server language and TranslatableContents rendering.
use ferraloom_protocol::chat::Component;
use std::{collections::BTreeMap, sync::OnceLock};

/// The server's immutable default Language, shared by names and command feedback.
pub struct Language;
impl Language {
    pub fn get_instance() -> &'static Self {
        &Self
    }
    /// Port of Language#getOrDefault, including its explicit fallback overload.
    pub fn get_or_default<'a>(&self, key: &'a str, fallback: &'a str) -> &'a str {
        translations().get(key).map_or(fallback, String::as_str)
    }
    pub fn has(&self, key: &str) -> bool {
        translations().contains_key(key)
    }
}
// Language#loadFromJson converts numeric Java format specifiers to string slots.
fn normalize_formats(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let index_end = if bytes.get(end) == Some(&b'$') {
            end += 1;
            end
        } else {
            i + 1
        };
        end = end.max(index_end);
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }
        if matches!(bytes.get(end), Some(b'd' | b'f')) {
            output.push_str(&text[start..i]);
            output.push('%');
            output.push_str(&text[i + 1..index_end]);
            output.push('s');
            start = end + 1;
            i = start;
        } else {
            i += 1;
        }
    }
    output.push_str(&text[start..]);
    output
}
pub fn translations() -> &'static BTreeMap<String, String> {
    static TRANSLATIONS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TRANSLATIONS.get_or_init(|| {
        serde_json::from_str::<BTreeMap<String, String>>(ferraloom_data::DEFAULT_LANGUAGE_JSON)
            .expect("vanilla language data")
            .into_iter()
            .map(|(key, value)| (key, normalize_formats(&value)))
            .collect()
    })
}
/// Port of TranslatableContents' %s, %n$s and %% substitutions for plain error text.
pub fn render(key: &str, args: &[String]) -> String {
    render_template(Language::get_instance().get_or_default(key, key), args)
}
fn render_template(template: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    let mut next_arg = 0;
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            result.push('%');
            continue;
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            digits.push(chars.next().unwrap());
        }
        let index = if !digits.is_empty() && chars.next() == Some('$') {
            let Some(index) = digits.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) else {
                return template.into();
            };
            index
        } else if digits.is_empty() {
            let index = next_arg;
            next_arg += 1;
            index
        } else {
            return template.into();
        };
        if chars.next() != Some('s') {
            return template.into();
        }
        let Some(value) = args.get(index) else {
            return template.into();
        };
        result.push_str(value);
    }
    result
}
/// Float.toString formatting used by translatableEscape's numeric arguments.
pub fn java_float_string(value: f32) -> String {
    // The Java-checked SNBT printer shares Float.toString's exact decimal selection.
    let mut text = ferraloom_nbt::Tag::Float(value).to_string();
    text.pop(); // FloatTag appends the SNBT type suffix after the number.
    text
}

/// Server-side rendering for console feedback; clients receive the original component.
pub fn plain_text(component: &Component) -> String {
    use ferraloom_protocol::chat::{Argument, Contents};
    let mut result = match &component.contents {
        Contents::Text(text) => text.clone(),
        Contents::Translatable {
            key,
            fallback,
            args,
        } => render_template(
            Language::get_instance().get_or_default(key, fallback.as_deref().unwrap_or(key)),
            &args
                .iter()
                .map(|arg| match arg {
                    Argument::String(s) => s.clone(),
                    Argument::Number(n) => match n {
                        ferraloom_protocol::chat::Numeric::Byte(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Short(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Int(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Long(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Float(v) => java_float_string(v.0),
                        ferraloom_protocol::chat::Numeric::Double(v) => {
                            let mut text = ferraloom_nbt::Tag::Double(v.0).to_string();
                            text.pop();
                            text
                        }
                    },
                    Argument::Boolean(v) => v.to_string(),
                    Argument::Component(c) => plain_text(c),
                })
                .collect::<Vec<_>>(),
        ),
        _ => String::new(),
    };
    for extra in &component.extra {
        result.push_str(&plain_text(extra));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{EntityRef, EntityState, item::ItemEntity},
        level::{Level, LevelContext, Region},
    };
    use ferraloom_data::Item;
    use ferraloom_protocol::component::ItemStack;

    #[test]
    fn native_names_language_fallbacks_and_format_errors() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("commands/vanilla/language-vectors.json")).unwrap();
        for row in &rows {
            let component = Component::from_json(&row["component"].to_string()).unwrap();
            assert_eq!(
                plain_text(&component),
                row["text"].as_str().unwrap(),
                "{}",
                row["case"]
            );
        }
        assert_eq!(rows.len(), 13);
        assert_eq!(
            Language::get_instance().get_or_default("ferraloom.missing", "fallback"),
            "fallback"
        );
        assert!(!Language::get_instance().has("block.minecraft.grass"));
        for (raw, expected) in [
            ("%d", "%s"),
            ("%.2f", "%s"),
            ("%2$d %1$05.2f", "%2$s %1$s"),
            ("%%d", "%%s"),
        ] {
            assert_eq!(normalize_formats(raw), expected);
        }
    }

    #[test]
    fn canonical_plain_names_dispatch_before_language_rendering() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        let cow = EntityRef(1);
        level
            .add_entity(cow, EntityState::Cow(Box::default()))
            .unwrap();
        assert_eq!(level.entity_plain_text_name(cow).unwrap(), "Cow");
        let mut item = ItemEntity::new(2, 0);
        item.set_item(ItemStack::new(Item::STONE, 1, Default::default()));
        let stone = EntityRef(2);
        level
            .add_entity(stone, EntityState::Item(Box::new(item)))
            .unwrap();
        assert_eq!(level.entity_plain_text_name(stone).unwrap(), "Stone");
        level
            .entity_state_mut(cow)
            .unwrap()
            .as_entity_mut()
            .entity_data
            .set(
                crate::entity::synched_data::CUSTOM_NAME,
                Some(Box::new({
                    let mut name = Component::literal("Named Cow");
                    name.extra.push(Component::literal("!"));
                    name
                })),
            );
        assert_eq!(level.entity_plain_text_name(cow).unwrap(), "Named Cow!");
        assert_eq!(
            level.entity_plain_text_name(EntityRef(99)),
            Err(crate::level::LevelError::MissingEntity(EntityRef(99)))
        );
    }
}
