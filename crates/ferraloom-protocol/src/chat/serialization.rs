use super::*;
use crate::Error;
use ferraloom_nbt::{ListTag, NbtString};
use serde_json::{Map, Value};

type Fields = CompoundTag;
fn error(message: impl Into<String>) -> Error {
    Error::invalid("component", message)
}
fn string(tag: &Tag) -> Result<String> {
    tag.as_string()
        .map(NbtString::to_string_lossy)
        .ok_or_else(|| error("Expected string"))
}
fn field<'a>(map: &'a Fields, key: &str) -> Result<&'a Tag> {
    map.get(key).ok_or_else(|| error(format!("Missing {key}")))
}
fn text(map: &Fields, key: &str) -> Result<String> {
    string(field(map, key)?)
}
fn identifier(map: &Fields, key: &str) -> Result<Identifier> {
    Identifier::parse(&text(map, key)?)
}
fn compound(tag: &Tag) -> Result<&Fields> {
    tag.as_compound().ok_or_else(|| error("Expected object"))
}
fn boolean(tag: &Tag, nbt: bool) -> Result<bool> {
    if nbt {
        tag.as_double()
            .map(|v| v != 0.0)
            .ok_or_else(|| error("Expected boolean"))
    } else {
        match tag {
            Tag::Byte(v) if *v == 0 || *v == 1 => Ok(*v != 0),
            _ => Err(error("Expected boolean")),
        }
    }
}
fn put(map: &mut Fields, key: &str, value: impl Into<String>) {
    map.insert(key, Tag::String(value.into().into()));
}
fn opt_component(map: &Fields, key: &str, nbt: bool) -> Result<Option<Box<Component>>> {
    map.get(key)
        .map(|v| decode(v.clone(), nbt).map(Box::new))
        .transpose()
}
fn list(tag: &Tag) -> Result<Vec<Tag>> {
    match tag {
        Tag::List(v) => Ok(v.0.clone()),
        Tag::ByteArray(v) => Ok(v.iter().copied().map(Tag::Byte).collect()),
        Tag::IntArray(v) => Ok(v.iter().copied().map(Tag::Int).collect()),
        Tag::LongArray(v) => Ok(v.iter().copied().map(Tag::Long).collect()),
        _ => Err(error("Expected list")),
    }
}

pub(super) fn decode(tag: Tag, nbt: bool) -> Result<Component> {
    decode_at(tag, nbt, 0)
}
fn decode_at(tag: Tag, nbt: bool, depth: usize) -> Result<Component> {
    if depth > 512 {
        return Err(error("Component nesting exceeds 512"));
    }
    if let Tag::String(text) = tag {
        return Ok(Component::literal(text.to_string_lossy()));
    }
    if let Tag::List(elements) = tag {
        let mut elements = elements.0.into_iter();
        let mut first = decode_at(
            elements
                .next()
                .ok_or_else(|| error("Empty component list"))?,
            nbt,
            depth + 1,
        )?;
        for element in elements {
            first.extra.push(decode_at(element, nbt, depth + 1)?);
        }
        return Ok(first);
    }
    let map = compound(&tag)?;
    // Presence of a discriminator prevents fallback to any other contents codec.
    let explicit = map.get("type").map(string).transpose()?;
    let kinds: &[&str] = match explicit.as_deref() {
        Some("text") => &["text"],
        Some("translatable") => &["translate"],
        Some("keybind") => &["keybind"],
        Some("score") => &["score"],
        Some("selector") => &["selector"],
        Some("nbt") => &["nbt"],
        Some("object") => &["object"],
        Some(value) => return Err(error(format!("Unknown contents type {value}"))),
        None => &[
            "text",
            "translate",
            "keybind",
            "score",
            "selector",
            "nbt",
            "object",
        ],
    };
    let contents = kinds
        .iter()
        .find_map(|kind| decode_contents(map, kind, nbt).ok())
        .ok_or_else(|| error("No matching contents codec"))?;
    let extra = match map.get("extra") {
        None => Vec::new(),
        Some(value) => {
            let values = list(value)?;
            if values.is_empty() {
                return Err(error("extra must not be empty"));
            }
            values
                .into_iter()
                .map(|v| decode_at(v, nbt, depth + 1))
                .collect::<Result<_>>()?
        }
    };
    Ok(Component {
        contents,
        style: decode_style(map, nbt)?,
        extra,
    })
}
fn decode_contents(map: &Fields, kind: &str, nbt: bool) -> Result<Contents> {
    Ok(match kind {
        "text" => Contents::Text(text(map, "text")?),
        "translate" => Contents::Translatable {
            key: text(map, "translate")?,
            fallback: map.get("fallback").and_then(|v| string(v).ok()),
            args: map
                .get("with")
                .map(|v| {
                    list(v)?
                        .into_iter()
                        .map(|tag| {
                            Ok(match tag {
                                Tag::String(v) => Argument::String(v.to_string_lossy()),
                                Tag::Byte(v) if !nbt => Argument::Boolean(v != 0),
                                Tag::Byte(_)
                                | Tag::Short(_)
                                | Tag::Int(_)
                                | Tag::Long(_)
                                | Tag::Float(_)
                                | Tag::Double(_) => {
                                    let number = match tag {
                                        Tag::Long(v) => serde_json::Number::from(v),
                                        Tag::Int(v) => serde_json::Number::from(v),
                                        _ => serde_json::Number::from_f64(tag.as_double().unwrap())
                                            .ok_or_else(|| error("Non-finite argument"))?,
                                    };
                                    Argument::Number(number)
                                }
                                tag => {
                                    let component = decode(tag, nbt)?;
                                    if let (Contents::Text(text), true, true) = (
                                        &component.contents,
                                        component.style == Style::default(),
                                        component.extra.is_empty(),
                                    ) {
                                        Argument::String(text.clone())
                                    } else {
                                        Argument::Component(Box::new(component))
                                    }
                                }
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default(),
        },
        "keybind" => Contents::Keybind(text(map, "keybind")?),
        "score" => {
            let score = compound(field(map, "score")?)?;
            Contents::Score {
                name: text(score, "name")?,
                objective: text(score, "objective")?,
            }
        }
        "selector" => Contents::Selector {
            pattern: text(map, "selector")?,
            separator: opt_component(map, "separator", nbt)?,
        },
        "nbt" => {
            let path = text(map, "nbt")?;
            ferraloom_nbt::NbtPath::parse(&path).map_err(|e| error(e.to_string()))?;
            let interpret = map
                .get("interpret")
                .and_then(|v| boolean(v, nbt).ok())
                .unwrap_or(false);
            let plain = map
                .get("plain")
                .and_then(|v| boolean(v, nbt).ok())
                .unwrap_or(false);
            if interpret && plain {
                return Err(error("interpret and plain cannot both be on"));
            }
            let source_type = map.get("source").map(string).transpose()?;
            let source = match source_type.as_deref() {
                Some("block") => DataSource::Block(text(map, "block")?),
                Some("entity") => DataSource::Entity(text(map, "entity")?),
                Some("storage") => DataSource::Storage(identifier(map, "storage")?),
                Some(_) => return Err(error("Unknown NBT source")),
                None => {
                    if let Ok(v) = text(map, "entity") {
                        DataSource::Entity(v)
                    } else if let Ok(v) = text(map, "block") {
                        DataSource::Block(v)
                    } else {
                        DataSource::Storage(identifier(map, "storage")?)
                    }
                }
            };
            Contents::Nbt {
                path,
                interpret,
                plain,
                separator: opt_component(map, "separator", nbt).unwrap_or(None),
                source,
            }
        }
        "object" => {
            let object_type = map.get("object").map(string).transpose()?;
            let object = match object_type.as_deref() {
                Some("atlas") | None if map.contains_key("sprite") => ObjectContents::Atlas {
                    atlas: map
                        .get("atlas")
                        .map(|v| Identifier::parse(&string(v)?))
                        .transpose()?
                        .unwrap_or(Identifier::parse("minecraft:blocks")?),
                    sprite: identifier(map, "sprite")?,
                },
                Some("player") | None => ObjectContents::Player {
                    profile: decode_profile(field(map, "player")?)?,
                    hat: map
                        .get("hat")
                        .map(|v| boolean(v, nbt))
                        .transpose()?
                        .unwrap_or(true),
                },
                _ => return Err(error("Unknown object type")),
            };
            Contents::Object {
                object,
                fallback: opt_component(map, "fallback", nbt)?,
            }
        }
        _ => return Err(error("Unknown contents")),
    })
}

pub(super) fn color(text: &str) -> Result<TextColor> {
    if let Some(hex) = text.strip_prefix('#') {
        let value = i32::from_str_radix(hex.strip_prefix('+').unwrap_or(hex), 16)
            .map_err(|_| error("Invalid color"))?;
        if !(0..=0xffffff).contains(&value) {
            return Err(error("Color out of range"));
        }
        return Ok(TextColor::from_rgb(value as u32));
    }
    let (name, value) = [
        ("black", 0x000000),
        ("dark_blue", 0x0000aa),
        ("dark_green", 0x00aa00),
        ("dark_aqua", 0x00aaaa),
        ("dark_red", 0xaa0000),
        ("dark_purple", 0xaa00aa),
        ("gold", 0xffaa00),
        ("gray", 0xaaaaaa),
        ("dark_gray", 0x555555),
        ("blue", 0x5555ff),
        ("green", 0x55ff55),
        ("aqua", 0x55ffff),
        ("red", 0xff5555),
        ("light_purple", 0xff55ff),
        ("yellow", 0xffff55),
        ("white", 0xffffff),
    ]
    .into_iter()
    .find(|(name, _)| *name == text)
    .ok_or_else(|| error("Unknown color"))?;
    Ok(TextColor {
        name: Some(name),
        value,
    })
}
fn decode_style(map: &Fields, nbt: bool) -> Result<Style> {
    let flag = |name| map.get(name).map(|tag| boolean(tag, nbt)).transpose();
    let shadow_color = map
        .get("shadow_color")
        .map(|value| {
            if let Some(v) = value.as_int() {
                return Ok(v);
            }
            let rgba = list(value)?;
            if rgba.len() != 4 {
                return Err(error("Expected RGBA color"));
            }
            let channel = |i: usize| {
                rgba[i]
                    .as_float()
                    .map(|v| (v * 255.0) as i32)
                    .ok_or_else(|| error("Invalid color channel"))
            };
            Ok(channel(3)? << 24 | channel(0)? << 16 | channel(1)? << 8 | channel(2)?)
        })
        .transpose()?;
    Ok(Style {
        color: map.get("color").map(|v| color(&string(v)?)).transpose()?,
        shadow_color,
        bold: flag("bold")?,
        italic: flag("italic")?,
        underlined: flag("underlined")?,
        strikethrough: flag("strikethrough")?,
        obfuscated: flag("obfuscated")?,
        click_event: map.get("click_event").map(decode_click).transpose()?,
        hover_event: map
            .get("hover_event")
            .map(|v| decode_hover(v, nbt))
            .transpose()?,
        insertion: map.get("insertion").map(string).transpose()?,
        font: map
            .get("font")
            .map(|v| Identifier::parse(&string(v)?))
            .transpose()?,
    })
}
fn decode_click(tag: &Tag) -> Result<ClickEvent> {
    let map = compound(tag)?;
    Ok(match text(map, "action")?.as_str() {
        "open_url" => {
            let value = text(map, "url")?;
            let scheme = value.split_once(':').map(|p| p.0.to_ascii_lowercase());
            if !matches!(scheme.as_deref(), Some("http" | "https"))
                || value
                    .chars()
                    .any(|c| c.is_ascii_control() || " <>\"{}|\\^`".contains(c))
            {
                return Err(error("Invalid untrusted URI"));
            }
            ClickEvent::OpenUrl(value)
        }
        "run_command" | "suggest_command" => {
            let command = text(map, "command")?;
            if command
                .chars()
                .any(|c| c == '§' || c < ' ' || c == '\u{7f}')
            {
                return Err(error("Disallowed chat character"));
            }
            if text(map, "action")? == "run_command" {
                ClickEvent::RunCommand(command)
            } else {
                ClickEvent::SuggestCommand(command)
            }
        }
        "change_page" => {
            let page = field(map, "page")?
                .as_int()
                .ok_or_else(|| error("Invalid page"))?;
            if page <= 0 {
                return Err(error("Page must be positive"));
            }
            ClickEvent::ChangePage(page)
        }
        "copy_to_clipboard" => ClickEvent::CopyToClipboard(text(map, "value")?),
        "custom" => ClickEvent::Custom {
            id: identifier(map, "id")?,
            payload: map.get("payload").cloned(),
        },
        "show_dialog" => ClickEvent::ShowDialog(field(map, "dialog")?.clone()),
        _ => return Err(error("Unknown or forbidden click event")),
    })
}
fn decode_hover(tag: &Tag, nbt: bool) -> Result<HoverEvent> {
    let map = compound(tag)?;
    Ok(match text(map, "action")?.as_str() {
        "show_text" => HoverEvent::ShowText(Box::new(decode(field(map, "value")?.clone(), nbt)?)),
        "show_entity" => HoverEvent::ShowEntity {
            entity_type: identifier(map, "id")?,
            uuid: decode_uuid(field(map, "uuid")?)?,
            name: opt_component(map, "name", nbt)?,
        },
        "show_item" => {
            let count = map
                .get("count")
                .map(|v| v.as_int().ok_or_else(|| error("Invalid item count")))
                .transpose()?
                .unwrap_or(1);
            if !(1..=99).contains(&count) {
                return Err(error("Item count must be in 1..99"));
            }
            let item = identifier(map, "id")?;
            if item.as_str() == "minecraft:air" {
                return Err(error("Item must be non-empty"));
            }
            HoverEvent::ShowItem {
                item,
                count: count as u8,
                components: map
                    .get("components")
                    .map(compound)
                    .transpose()?
                    .cloned()
                    .unwrap_or_default(),
            }
        }
        _ => return Err(error("Unknown hover event")),
    })
}
fn decode_uuid(tag: &Tag) -> Result<Uuid> {
    if let Tag::String(s) = tag {
        let text = s.to_string_lossy().replace('-', "");
        let value = u128::from_str_radix(&text, 16).map_err(|_| error("Invalid UUID"))?;
        return Ok(Uuid::from_byte_array(value.to_be_bytes()));
    }
    let words = list(tag)?
        .into_iter()
        .map(|v| v.as_int().ok_or_else(|| error("Invalid UUID word")))
        .collect::<Result<Vec<_>>>()?;
    Ok(Uuid::from_int_array(
        words
            .try_into()
            .map_err(|_| error("UUID must have four words"))?,
    ))
}
fn encode_uuid(uuid: Uuid) -> Tag {
    Tag::IntArray(uuid.to_int_array().to_vec())
}
fn decode_profile(tag: &Tag) -> Result<ResolvableProfile> {
    if let Tag::String(name) = tag {
        return Ok(ResolvableProfile {
            name: Some(name.to_string_lossy()),
            id: None,
            properties: Vec::new(),
            skin_patch: Fields::new(),
        });
    }
    let map = compound(tag)?;
    let mut properties = Vec::new();
    if let Some(value) = map.get("properties") {
        for value in list(value)? {
            let property = compound(&value)?;
            properties.push(ProfileProperty {
                name: text(property, "name")?,
                value: text(property, "value")?,
                signature: property.get("signature").map(string).transpose()?,
            });
        }
    }
    let mut skin_patch = map.clone();
    for key in ["name", "id", "properties"] {
        skin_patch.remove(key);
    }
    Ok(ResolvableProfile {
        name: map.get("name").map(string).transpose()?,
        id: map.get("id").map(decode_uuid).transpose()?,
        properties,
        skin_patch,
    })
}

pub(super) fn encode(component: &Component) -> Result<Tag> {
    if let Contents::Text(text) = &component.contents {
        if component.style == Style::default() && component.extra.is_empty() {
            return Ok(Tag::String(text.as_str().into()));
        }
    }
    let mut map = Fields::new();
    match &component.contents {
        Contents::Text(text) => put(&mut map, "text", text),
        Contents::Translatable {
            key,
            fallback,
            args,
        } => {
            put(&mut map, "translate", key);
            if let Some(v) = fallback {
                put(&mut map, "fallback", v);
            }
            if !args.is_empty() {
                let values = args
                    .iter()
                    .map(|arg| {
                        Ok(match arg {
                            Argument::String(v) => Tag::String(v.as_str().into()),
                            Argument::Boolean(v) => Tag::Byte(i8::from(*v)),
                            Argument::Number(v) => Tag::Double(
                                v.as_f64().ok_or_else(|| error("Invalid argument number"))?,
                            ),
                            Argument::Component(v) => encode(v)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                map.insert("with", Tag::List(ListTag(values)));
            }
        }
        Contents::Keybind(v) => put(&mut map, "keybind", v),
        Contents::Score { name, objective } => {
            let mut score = Fields::new();
            put(&mut score, "name", name);
            put(&mut score, "objective", objective);
            map.insert("score", Tag::Compound(score));
        }
        Contents::Selector { pattern, separator } => {
            put(&mut map, "selector", pattern);
            encode_opt_component(&mut map, "separator", separator)?;
        }
        Contents::Nbt {
            path,
            interpret,
            plain,
            separator,
            source,
        } => {
            if *interpret && *plain {
                return Err(error("interpret and plain cannot both be on"));
            }
            put(&mut map, "nbt", path);
            if *interpret {
                map.insert("interpret", Tag::Byte(1));
            }
            if *plain {
                map.insert("plain", Tag::Byte(1));
            }
            encode_opt_component(&mut map, "separator", separator)?;
            match source {
                DataSource::Block(v) => put(&mut map, "block", v),
                DataSource::Entity(v) => put(&mut map, "entity", v),
                DataSource::Storage(v) => put(&mut map, "storage", v.as_str()),
            }
        }
        Contents::Object { object, fallback } => {
            match object {
                ObjectContents::Atlas { atlas, sprite } => {
                    if atlas.as_str() != "minecraft:blocks" {
                        put(&mut map, "atlas", atlas.as_str());
                    }
                    put(&mut map, "sprite", sprite.as_str());
                }
                ObjectContents::Player { profile, hat } => {
                    map.insert("player", encode_profile(profile));
                    if !hat {
                        map.insert("hat", Tag::Byte(0));
                    }
                }
            }
            encode_opt_component(&mut map, "fallback", fallback)?;
        }
    }
    if !component.extra.is_empty() {
        map.insert(
            "extra",
            Tag::List(ListTag(
                component.extra.iter().map(encode).collect::<Result<_>>()?,
            )),
        );
    }
    encode_style(&component.style, &mut map)?;
    Ok(Tag::Compound(map))
}
fn encode_opt_component(map: &mut Fields, key: &str, value: &Option<Box<Component>>) -> Result<()> {
    if let Some(value) = value {
        map.insert(key, encode(value)?);
    }
    Ok(())
}
fn encode_style(style: &Style, map: &mut Fields) -> Result<()> {
    if let Some(color) = &style.color {
        put(map, "color", color.serialize());
    }
    if let Some(shadow) = style.shadow_color {
        map.insert("shadow_color", Tag::Int(shadow));
    }
    for (key, value) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ] {
        if let Some(value) = value {
            map.insert(key, Tag::Byte(i8::from(value)));
        }
    }
    if let Some(click) = &style.click_event {
        map.insert("click_event", encode_click(click)?);
    }
    if let Some(hover) = &style.hover_event {
        map.insert("hover_event", encode_hover(hover)?);
    }
    if let Some(value) = &style.insertion {
        put(map, "insertion", value);
    }
    if let Some(value) = &style.font {
        put(map, "font", value.as_str());
    }
    Ok(())
}
fn encode_click(click: &ClickEvent) -> Result<Tag> {
    let mut map = Fields::new();
    let action = match click {
        ClickEvent::OpenUrl(v) => {
            put(&mut map, "url", v);
            "open_url"
        }
        ClickEvent::RunCommand(v) => {
            put(&mut map, "command", v);
            "run_command"
        }
        ClickEvent::SuggestCommand(v) => {
            put(&mut map, "command", v);
            "suggest_command"
        }
        ClickEvent::ChangePage(v) => {
            map.insert("page", Tag::Int(*v));
            "change_page"
        }
        ClickEvent::CopyToClipboard(v) => {
            put(&mut map, "value", v);
            "copy_to_clipboard"
        }
        ClickEvent::Custom { id, payload } => {
            put(&mut map, "id", id.as_str());
            if let Some(v) = payload {
                map.insert("payload", v.clone());
            }
            "custom"
        }
        ClickEvent::ShowDialog(v) => {
            map.insert("dialog", v.clone());
            "show_dialog"
        }
    };
    put(&mut map, "action", action);
    let tag = Tag::Compound(map);
    decode_click(&tag)?;
    Ok(tag)
}
fn encode_hover(hover: &HoverEvent) -> Result<Tag> {
    let mut map = Fields::new();
    let action = match hover {
        HoverEvent::ShowText(v) => {
            map.insert("value", encode(v)?);
            "show_text"
        }
        HoverEvent::ShowEntity {
            entity_type,
            uuid,
            name,
        } => {
            put(&mut map, "id", entity_type.as_str());
            map.insert("uuid", encode_uuid(*uuid));
            encode_opt_component(&mut map, "name", name)?;
            "show_entity"
        }
        HoverEvent::ShowItem {
            item,
            count,
            components,
        } => {
            if !(1..=99).contains(count) {
                return Err(error("Item count must be in 1..99"));
            }
            put(&mut map, "id", item.as_str());
            if *count != 1 {
                map.insert("count", Tag::Int((*count).into()));
            }
            if !components.is_empty() {
                map.insert("components", Tag::Compound(components.clone()));
            }
            "show_item"
        }
    };
    put(&mut map, "action", action);
    Ok(Tag::Compound(map))
}
fn encode_profile(profile: &ResolvableProfile) -> Tag {
    let mut map = profile.skin_patch.clone();
    if let Some(value) = &profile.name {
        put(&mut map, "name", value);
    }
    if let Some(value) = profile.id {
        map.insert("id", encode_uuid(value));
    }
    if !profile.properties.is_empty() {
        let values = profile
            .properties
            .iter()
            .map(|p| {
                let mut map = Fields::new();
                put(&mut map, "name", &p.name);
                put(&mut map, "value", &p.value);
                if let Some(v) = &p.signature {
                    put(&mut map, "signature", v);
                }
                Tag::Compound(map)
            })
            .collect();
        map.insert("properties", Tag::List(ListTag(values)));
    }
    Tag::Compound(map)
}

// JsonOps and NbtOps differ in booleans and numeric/list representation. Keep
// that conversion here, so the public component tree has one canonical type.
fn json_to_tag(value: Value) -> Result<Tag> {
    Ok(match value {
        Value::Null => Tag::End,
        Value::Bool(v) => Tag::Byte(i8::from(v)),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                Tag::Long(v)
            } else {
                Tag::Double(v.as_f64().ok_or_else(|| error("Invalid number"))?)
            }
        }
        Value::String(v) => Tag::String(v.into()),
        Value::Array(values) => Tag::List(ListTag(
            values.into_iter().map(json_to_tag).collect::<Result<_>>()?,
        )),
        Value::Object(values) => {
            let mut map = Fields::new();
            for (key, value) in values {
                if !value.is_null() {
                    map.insert(key, json_to_tag(value)?);
                }
            }
            Tag::Compound(map)
        }
    })
}
pub(super) fn decode_json(value: Value) -> Result<Component> {
    decode(json_to_tag(value)?, false)
}
fn tag_to_json(tag: Tag, key: &str) -> Result<Value> {
    Ok(match tag {
        Tag::End => Value::Null,
        Tag::Byte(v)
            if matches!(
                key,
                "bold"
                    | "italic"
                    | "underlined"
                    | "strikethrough"
                    | "obfuscated"
                    | "interpret"
                    | "plain"
                    | "hat"
            ) =>
        {
            Value::Bool(v != 0)
        }
        Tag::Byte(v) => Value::from(v),
        Tag::Short(v) => Value::from(v),
        Tag::Int(v) => Value::from(v),
        Tag::Long(v) => Value::from(v),
        Tag::Float(v) => Value::from(v),
        Tag::Double(v) => Value::from(v),
        Tag::String(v) => Value::String(v.to_string_lossy()),
        Tag::List(values) => Value::Array(
            values
                .0
                .into_iter()
                .map(|v| tag_to_json(v, ""))
                .collect::<Result<_>>()?,
        ),
        Tag::ByteArray(values) => Value::Array(values.into_iter().map(Value::from).collect()),
        Tag::IntArray(values) => Value::Array(values.into_iter().map(Value::from).collect()),
        Tag::LongArray(values) => Value::Array(values.into_iter().map(Value::from).collect()),
        Tag::Compound(values) => {
            let mut result = Map::new();
            for (key, value) in values.iter() {
                let key = key.to_string_lossy();
                result.insert(key.clone(), tag_to_json(value.clone(), &key)?);
            }
            Value::Object(result)
        }
    })
}
pub(super) fn encode_json(component: &Component) -> Result<Value> {
    let mut value = tag_to_json(encode(component)?, "")?;
    // Preserve the primitive kind of translation arguments in JsonOps.
    if let (Contents::Translatable { args, .. }, Value::Object(map)) =
        (&component.contents, &mut value)
    {
        if !args.is_empty() {
            map.insert(
                "with".into(),
                Value::Array(
                    args.iter()
                        .map(|a| match a {
                            Argument::String(v) => Ok(Value::String(v.clone())),
                            Argument::Number(v) => Ok(Value::Number(v.clone())),
                            Argument::Boolean(v) => Ok(Value::Bool(*v)),
                            Argument::Component(v) => encode_json(v),
                        })
                        .collect::<Result<_>>()?,
                ),
            );
        }
    }
    Ok(value)
}
