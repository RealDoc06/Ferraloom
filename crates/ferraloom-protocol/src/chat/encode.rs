//! The component codecs' encode-side DynamicOps operations. Do not route this
//! through NBT: boolean and byte produce different vanilla HashOps hashes.
use super::*;
use crate::ops::{EncodeOps, encode_nbt};

struct Fields<'a, O: EncodeOps> {
    ops: &'a O,
    values: Vec<(O::Value, O::Value)>,
}
impl<'a, O: EncodeOps> Fields<'a, O> {
    fn new(ops: &'a O) -> Self {
        Self {
            ops,
            values: Vec::new(),
        }
    }
    fn put(&mut self, key: &str, value: O::Value) {
        self.values.push((self.ops.create_string(key), value));
    }
    fn text(&mut self, key: &str, value: &str) {
        self.put(key, self.ops.create_string(value));
    }
    fn boolean(&mut self, key: &str, value: bool) {
        self.put(key, self.ops.create_boolean(value));
    }
    fn finish(self) -> Result<O::Value> {
        self.ops.create_map(self.values)
    }
}
pub(super) fn component<O: EncodeOps>(value: &Component, ops: &O) -> Result<O::Value> {
    if let Contents::Text(text) = &value.contents
        && value.style == Style::default()
        && value.extra.is_empty()
    {
        return Ok(ops.create_string(text));
    }
    let mut map = Fields::new(ops);
    match &value.contents {
        Contents::Text(v) => map.text("text", v),
        Contents::Translatable {
            key,
            fallback,
            args,
        } => {
            map.text("translate", key);
            if let Some(v) = fallback {
                map.text("fallback", v);
            }
            if !args.is_empty() {
                map.put(
                    "with",
                    ops.create_list(
                        args.iter()
                            .map(|arg| {
                                Ok(match arg {
                                    Argument::String(v) => ops.create_string(v),
                                    Argument::Boolean(v) => ops.create_boolean(*v),
                                    Argument::Number(v) => numeric(*v, ops),
                                    Argument::Component(v) => component(v, ops)?,
                                })
                            })
                            .collect::<Result<_>>()?,
                    ),
                );
            }
        }
        Contents::Keybind(v) => map.text("keybind", v),
        Contents::Score { name, objective } => {
            let mut score = Fields::new(ops);
            score.text("name", name);
            score.text("objective", objective);
            map.put("score", score.finish()?);
        }
        Contents::Selector { pattern, separator } => {
            map.text("selector", pattern);
            if let Some(v) = separator {
                map.put("separator", component(v, ops)?);
            }
        }
        Contents::Nbt {
            path,
            interpret,
            plain,
            separator,
            source,
        } => {
            if *interpret && *plain {
                return Err(crate::Error::invalid(
                    "component",
                    "interpret and plain cannot both be on",
                ));
            }
            map.text("nbt", path);
            if *interpret {
                map.boolean("interpret", true);
            }
            if *plain {
                map.boolean("plain", true);
            }
            if let Some(v) = separator {
                map.put("separator", component(v, ops)?);
            }
            match source {
                DataSource::Entity(v) => map.text("entity", v),
                DataSource::Block(v) => map.text("block", v),
                DataSource::Storage(v) => map.text("storage", v.as_str()),
            }
        }
        Contents::Object { object, fallback } => {
            match object {
                ObjectContents::Atlas { atlas, sprite } => {
                    if atlas.as_str() != "minecraft:blocks" {
                        map.text("atlas", atlas.as_str());
                    }
                    map.text("sprite", sprite.as_str());
                }
                ObjectContents::Player { profile, hat } => {
                    map.put("player", encode_profile(profile, ops)?);
                    if !*hat {
                        map.boolean("hat", false);
                    }
                }
            }
            if let Some(v) = fallback {
                map.put("fallback", component(v, ops)?);
            }
        }
    }
    if !value.extra.is_empty() {
        map.put(
            "extra",
            ops.create_list(
                value
                    .extra
                    .iter()
                    .map(|v| component(v, ops))
                    .collect::<Result<_>>()?,
            ),
        );
    }
    style_fields(&value.style, &mut map)?;
    map.finish()
}
fn numeric<O: EncodeOps>(value: Numeric, ops: &O) -> O::Value {
    match value {
        Numeric::Byte(v) => ops.create_byte(v),
        Numeric::Short(v) => ops.create_short(v),
        Numeric::Int(v) => ops.create_int(v),
        Numeric::Long(v) => ops.create_long(v),
        Numeric::Float(v) => ops.create_float(v.0),
        Numeric::Double(v) => ops.create_double(v.0),
    }
}
pub(super) fn style<O: EncodeOps>(value: &Style, ops: &O) -> Result<O::Value> {
    let mut map = Fields::new(ops);
    style_fields(value, &mut map)?;
    map.finish()
}
fn style_fields<O: EncodeOps>(value: &Style, map: &mut Fields<'_, O>) -> Result<()> {
    let ops = map.ops;
    if let Some(v) = &value.color {
        map.text("color", &v.serialize());
    }
    if let Some(v) = value.shadow_color {
        map.put("shadow_color", ops.create_int(v));
    }
    for (key, v) in [
        ("bold", value.bold),
        ("italic", value.italic),
        ("underlined", value.underlined),
        ("strikethrough", value.strikethrough),
        ("obfuscated", value.obfuscated),
    ] {
        if let Some(v) = v {
            map.boolean(key, v);
        }
    }
    if let Some(v) = &value.click_event {
        map.put("click_event", click(v, ops)?);
    }
    if let Some(v) = &value.hover_event {
        map.put("hover_event", hover(v, ops)?);
    }
    if let Some(v) = &value.insertion {
        map.text("insertion", v);
    }
    if let Some(v) = &value.font {
        map.text("font", v.as_str());
    }
    Ok(())
}
fn click<O: EncodeOps>(value: &ClickEvent, ops: &O) -> Result<O::Value> {
    // Reuse the codec's allowed-action and command/URI validation.
    super::serialization::validate_click(value)?;
    let mut map = Fields::new(ops);
    let action = match value {
        ClickEvent::OpenUrl(v) => {
            map.text("url", v);
            "open_url"
        }
        ClickEvent::RunCommand(v) => {
            map.text("command", v);
            "run_command"
        }
        ClickEvent::SuggestCommand(v) => {
            map.text("command", v);
            "suggest_command"
        }
        ClickEvent::ChangePage(v) => {
            map.put("page", ops.create_int(*v));
            "change_page"
        }
        ClickEvent::CopyToClipboard(v) => {
            map.text("value", v);
            "copy_to_clipboard"
        }
        ClickEvent::Custom { id, payload } => {
            map.text("id", id.as_str());
            if let Some(v) = payload {
                map.put("payload", encode_nbt(v, ops)?);
            }
            "custom"
        }
        ClickEvent::ShowDialog(v) => {
            map.put("dialog", encode_nbt(v, ops)?);
            "show_dialog"
        }
    };
    map.text("action", action);
    map.finish()
}
fn hover<O: EncodeOps>(value: &HoverEvent, ops: &O) -> Result<O::Value> {
    let mut map = Fields::new(ops);
    let action = match value {
        HoverEvent::ShowText(v) => {
            map.put("value", component(v, ops)?);
            "show_text"
        }
        HoverEvent::ShowEntity {
            entity_type,
            uuid,
            name,
        } => {
            map.text("id", entity_type.as_str());
            map.put("uuid", ops.create_int_list(&uuid.to_int_array()));
            if let Some(v) = name {
                map.put("name", component(v, ops)?);
            }
            "show_entity"
        }
        HoverEvent::ShowItem {
            item,
            count,
            components,
        } => {
            if !(1..=99).contains(count) {
                return Err(crate::Error::invalid(
                    "component",
                    "Item count must be in 1..99",
                ));
            }
            map.text("id", item.as_str());
            if *count != 1 {
                map.put("count", ops.create_int((*count).into()));
            }
            if !components.is_empty() {
                map.put(
                    "components",
                    encode_nbt(&Tag::Compound(components.clone()), ops)?,
                );
            }
            "show_item"
        }
    };
    map.text("action", action);
    map.finish()
}
fn encode_profile<O: EncodeOps>(profile: &ResolvableProfile, ops: &O) -> Result<O::Value> {
    let mut map = Fields::new(ops);
    if let Some(v) = &profile.name {
        map.text("name", v);
    }
    if let Some(v) = profile.id {
        map.put("id", ops.create_int_list(&v.to_int_array()));
    }
    if !profile.properties.is_empty() {
        map.put(
            "properties",
            ops.create_list(
                profile
                    .properties
                    .iter()
                    .map(|p| {
                        let mut map = Fields::new(ops);
                        map.text("name", &p.name);
                        map.text("value", &p.value);
                        if let Some(v) = &p.signature {
                            map.text("signature", v);
                        }
                        map.finish()
                    })
                    .collect::<Result<_>>()?,
            ),
        );
    }
    for (key, value) in profile.skin_patch.iter() {
        map.put(&key.to_string_lossy(), encode_nbt(value, ops)?);
    }
    map.finish()
}
