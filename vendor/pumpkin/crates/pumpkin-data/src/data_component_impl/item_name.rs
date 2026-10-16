use std::borrow::Cow;

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

use super::{DataComponentImpl, Digest};

/// An item name's component, never inferred from its spelling or string ownership.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemNameImpl {
    Literal {
        name: Cow<'static, str>,
    },
    /// Generated defaults borrow their keys without allocating a component tree.
    Translated {
        name: Cow<'static, str>,
    },
    /// Retains supported rich content. Direct construction is unchecked; wire
    /// serializers must call checked_write_data, never forward this tag verbatim.
    Component(NbtTag),
}

impl ItemNameImpl {
    #[must_use]
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        Self::parse(data).ok()
    }

    /// Refuses invalid and explicitly unsupported forms before command/network mutation.
    /// read_data returns None on refusal; persistence callers must protect the
    /// containing data rather than silently dropping the unsupported stack.
    pub fn parse(data: &NbtTag) -> Result<Self, &'static str> {
        Ok(match normalize_component(data, 0)? {
            NbtTag::String(text) => Self::Literal {
                name: Cow::Owned(text.to_string()),
            },
            NbtTag::Compound(component) if component.child_tags.len() == 1 => {
                if let Some(key) = component.get_string("translate") {
                    Self::Translated {
                        name: Cow::Owned(key.to_owned()),
                    }
                } else {
                    Self::Component(NbtTag::Compound(component))
                }
            }
            data => Self::Component(data),
        })
    }

    /// All wire writers must use this: the public Component variant can contain
    /// unchecked legacy/native data even when it did not pass through read_data.
    pub fn checked_write_data(&self) -> Result<NbtTag, &'static str> {
        match self {
            Self::Component(tag) => normalize_component(tag, 0),
            _ => Ok(self.write_data()),
        }
    }

    /// Lossy display projection, not component identity or a Minecraft text renderer.
    /// Translation keys stay unresolved; opaque content stays visible as NBT.
    #[must_use]
    pub fn display_text(&self) -> String {
        match self {
            Self::Literal { name } | Self::Translated { name } => name.to_string(),
            Self::Component(tag) => project_text(tag),
        }
    }
}

impl DataComponentImpl for ItemNameImpl {
    // The infallible persistence trait retains unchecked native data for recovery.
    // Network serialization is fallible and must use checked_write_data instead.
    fn write_data(&self) -> NbtTag {
        match self {
            Self::Literal { name } => NbtTag::String(name.as_ref().into()),
            Self::Translated { name } => {
                let mut component = NbtCompound::new();
                component.put_string("translate", name.to_string());
                NbtTag::Compound(component)
            }
            Self::Component(tag) => tag.clone(),
        }
    }

    fn get_hash(&self) -> i32 {
        // 26.3 ITEM_NAME uses ComponentSerialization.CODEC with HashOps.CRC32C.
        // Its plain literal path hashes tag 12, UTF-16 length, then UTF-16LE units.
        // Keep this fix local: other components' checksum contracts are unaudited.
        if let Self::Literal { name } = self {
            let mut digest = Digest::new();
            digest.update(&[12]);
            digest.update(&(name.encode_utf16().count() as u32).to_le_bytes());
            for unit in name.encode_utf16() {
                digest.update(&unit.to_le_bytes());
            }
            return digest.finalize() as i32;
        }
        // Structured hashes remain local checksums, NOT vanilla prediction hashes.
        let mut digest = Digest::new();
        hash_tag(&self.write_data(), &mut digest);
        digest.finalize() as i32
    }

    default_impl!(ItemName);
}

const INVALID: &str = "Invalid item_name component (including codec partial results)";
const UNSUPPORTED: &str =
    "Unsupported item_name component; validation is not implemented for this form";

/// An error-free subset of 26.3 ComponentSerialization, not a complete text codec.
/// Rebuild selected content so ignored malformed alternatives never reach the wire.
/// Unknown fields/grammars are refused rather than assumed safe or silently erased.
fn normalize_component(tag: &NbtTag, depth: usize) -> Result<NbtTag, &'static str> {
    if depth > pumpkin_nbt::MAX_NBT_DEPTH {
        return Err(INVALID);
    }
    match tag {
        NbtTag::String(_) => Ok(tag.clone()),
        NbtTag::List(children) => normalize_list(children, depth),
        NbtTag::Compound(source) => {
            let mut component = NbtCompound::new();
            if let Some(kind) = source.get("type") {
                let NbtTag::String(kind) = kind else {
                    return Err(INVALID);
                };
                component = normalize_content(kind, source, depth)?;
            } else {
                // FuzzyCodec accepts error-free branches, not their partial values.
                // Keep every supported successful alternative: their tie ordering is
                // not established here, so do not guess which one Java selects.
                let mut unsupported = false;
                for kind in [
                    "text",
                    "translatable",
                    "keybind",
                    "score",
                    "selector",
                    "nbt",
                    "object",
                ] {
                    match normalize_content(kind, source, depth) {
                        Ok(content) => {
                            for (key, value) in content.child_tags {
                                component.put(&key, value);
                            }
                        }
                        Err(UNSUPPORTED) => unsupported = true,
                        Err(_) => {}
                    }
                }
                if unsupported {
                    return Err(UNSUPPORTED);
                }
                if component.is_empty() {
                    return Err(INVALID);
                }
            }
            for (key, value) in &source.child_tags {
                match key.as_ref() {
                    "type" | "text" | "translate" | "fallback" | "with" | "keybind" | "score"
                    | "selector" | "separator" | "nbt" | "interpret" | "plain" | "source"
                    | "storage" | "entity" | "block" | "object" | "sprite" | "atlas" | "player"
                    | "hat" => {}
                    "extra" => {
                        let NbtTag::List(children) = value else {
                            return Err(INVALID);
                        };
                        component.put_list(key, normalize_children(children, depth, false)?);
                    }
                    "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" => {
                        // NbtOps' BOOL reads a Number, not a string "true"/"false".
                        if !is_number(value) {
                            return Err(INVALID);
                        }
                        component.put(key, value.clone());
                    }
                    "color" => {
                        let NbtTag::String(color) = value else {
                            return Err(INVALID);
                        };
                        if !color.is_ascii() {
                            // Java's radix parser also accepts some Unicode digits.
                            return Err(UNSUPPORTED);
                        }
                        if !valid_color(color) {
                            return Err(INVALID);
                        }
                        component.put(key, value.clone());
                    }
                    "shadow_color" => {
                        // ARGB_COLOR_CODEC also accepts a four-float vector. That
                        // alternative is explicitly unsupported until modeled.
                        if !is_number(value) {
                            return Err(UNSUPPORTED);
                        }
                        component.put(key, value.clone());
                    }
                    "insertion" => {
                        if !matches!(value, NbtTag::String(_)) {
                            return Err(INVALID);
                        }
                        component.put(key, value.clone());
                    }
                    "font" => {
                        let NbtTag::String(font) = value else {
                            return Err(UNSUPPORTED);
                        };
                        if !supported_identifier(font) {
                            return Err(UNSUPPORTED);
                        }
                        component.put(key, value.clone());
                    }
                    "click_event" | "hover_event" => {
                        component.put(key, normalize_event(key, value, depth)?);
                    }
                    _ => return Err(UNSUPPORTED),
                }
            }
            // Proven canonical literal form, including explicit type:text and
            // discarded invalid alternative branches. Styled literals stay rich.
            if component.child_tags.len() == 1
                && let Some(text) = component.get_string("text")
            {
                return Ok(NbtTag::String(text.into()));
            }
            Ok(NbtTag::Compound(component))
        }
        _ => Err(INVALID),
    }
}

/// ComponentSerialization.createFromList keeps the first component's content
/// and style, then appends the remaining components after its existing extras.
fn normalize_list(children: &[NbtTag], depth: usize) -> Result<NbtTag, &'static str> {
    let mut children = normalize_children(children, depth, false)?.into_iter();
    let first = children.next().ok_or(INVALID)?;
    if children.len() == 0 {
        return Ok(first);
    }
    let mut component = match first {
        NbtTag::String(text) => {
            let mut component = NbtCompound::new();
            component.put("text", NbtTag::String(text));
            component
        }
        NbtTag::Compound(component) => component,
        _ => return Err(INVALID),
    };
    let mut extra = match component.child_tags.remove("extra") {
        Some(NbtTag::List(extra)) => extra,
        None => Vec::new(),
        Some(_) => return Err(INVALID),
    };
    extra.extend(children);
    component.put_list("extra", extra);
    Ok(NbtTag::Compound(component))
}

fn normalize_children(
    children: &[NbtTag],
    depth: usize,
    arguments: bool,
) -> Result<Vec<NbtTag>, &'static str> {
    if children.is_empty() && !arguments {
        return Err(INVALID);
    }
    children
        .iter()
        .map(|child| {
            if arguments && is_number(child) {
                Ok(child.clone())
            } else {
                normalize_component(child, depth + 1)
            }
        })
        .collect()
}

fn normalize_content(
    kind: &str,
    source: &NbtCompound,
    depth: usize,
) -> Result<NbtCompound, &'static str> {
    let mut content = NbtCompound::new();
    match kind {
        "text" | "keybind" => {
            content.put_string(kind, source.get_string(kind).ok_or(INVALID)?.to_owned());
        }
        "translatable" => {
            content.put_string(
                "translate",
                source.get_string("translate").ok_or(INVALID)?.to_owned(),
            );
            // The official fallback field is lenientOptionalFieldOf(STRING).
            if let Some(fallback) = source.get_string("fallback") {
                content.put_string("fallback", fallback.to_owned());
            }
            if let Some(with) = source.get("with") {
                let NbtTag::List(children) = with else {
                    return Err(INVALID);
                };
                content.put_list("with", normalize_children(children, depth, true)?);
            }
        }
        "score" => {
            let score = source.get_compound("score").ok_or(INVALID)?;
            let mut normalized = NbtCompound::new();
            for key in ["name", "objective"] {
                normalized.put_string(key, score.get_string(key).ok_or(INVALID)?.to_owned());
            }
            require_keys(score, &["name", "objective"])?;
            content.put_compound("score", normalized);
        }
        "selector" => {
            let selector = source.get_string("selector").ok_or(INVALID)?;
            // A deliberate subset, not a substitute for EntitySelectorParser.
            // Bracketed selectors, UUIDs and other name forms are unsupported.
            if !matches!(selector, "@a" | "@e" | "@s" | "@p" | "@r" | "@n")
                && !simple_player_name(selector)
            {
                return Err(UNSUPPORTED);
            }
            content.put_string("selector", selector.to_owned());
            if let Some(separator) = source.get("separator") {
                content.put("separator", normalize_component(separator, depth + 1)?);
            }
        }
        "nbt" => {
            source.get_string("nbt").ok_or(INVALID)?;
            if !["storage", "entity", "block"]
                .iter()
                .any(|key| source.get(key).is_some())
            {
                return Err(INVALID);
            }
            // Requires NbtPathArgument plus source-specific grammar validation.
            return Err(UNSUPPORTED);
        }
        "object" => return normalize_object(source),
        _ => return Err(INVALID),
    }
    Ok(content)
}

fn normalize_object(source: &NbtCompound) -> Result<NbtCompound, &'static str> {
    if let Some(kind) = source.get("object") {
        let NbtTag::String(kind) = kind else {
            return Err(INVALID);
        };
        return normalize_object_content(kind, source);
    }
    let mut content = NbtCompound::new();
    for kind in ["atlas", "player"] {
        match normalize_object_content(kind, source) {
            Ok(branch) => {
                for (key, value) in branch.child_tags {
                    content.put(&key, value);
                }
            }
            Err(UNSUPPORTED) => return Err(UNSUPPORTED),
            Err(_) => {}
        }
    }
    if content.is_empty() {
        return Err(INVALID);
    }
    Ok(content)
}

fn normalize_object_content(kind: &str, source: &NbtCompound) -> Result<NbtCompound, &'static str> {
    let mut content = NbtCompound::new();
    match kind {
        "atlas" => {
            let sprite = source.get_string("sprite").ok_or(INVALID)?;
            if !supported_identifier(sprite) {
                return Err(UNSUPPORTED);
            }
            content.put_string("sprite", sprite.to_owned());
            if let Some(atlas) = source.get("atlas") {
                let NbtTag::String(atlas) = atlas else {
                    return Err(INVALID);
                };
                if !supported_identifier(atlas) {
                    return Err(UNSUPPORTED);
                }
                content.put_string("atlas", atlas.to_string());
            }
        }
        "player" => {
            let player = source.get("player").ok_or(INVALID)?;
            let name = match player {
                NbtTag::String(name) => name.as_ref(),
                NbtTag::Compound(profile) => {
                    require_keys(profile, &["name"])?;
                    profile.get_string("name").ok_or(UNSUPPORTED)?
                }
                _ => return Err(INVALID),
            };
            // Full profiles, textures and skin patches need their actual codecs.
            if !simple_player_name(name) {
                return Err(UNSUPPORTED);
            }
            let mut profile = NbtCompound::new();
            profile.put_string("name", name.to_owned());
            content.put_compound("player", profile);
            if let Some(hat) = source.get("hat") {
                if !is_number(hat) {
                    return Err(INVALID);
                }
                content.put("hat", hat.clone());
            }
        }
        _ => return Err(INVALID),
    }
    Ok(content)
}

fn normalize_event(key: &str, tag: &NbtTag, depth: usize) -> Result<NbtTag, &'static str> {
    let NbtTag::Compound(event) = tag else {
        return Err(INVALID);
    };
    let action = event.get_string("action").ok_or(INVALID)?;
    let mut result = NbtCompound::new();
    result.put_string("action", action.to_owned());
    match (key, action) {
        ("click_event", "copy_to_clipboard") => {
            require_keys(event, &["action", "value"])?;
            result.put_string(
                "value",
                event.get_string("value").ok_or(INVALID)?.to_owned(),
            );
        }
        ("hover_event", "show_text") => {
            require_keys(event, &["action", "value"])?;
            result.put(
                "value",
                normalize_component(event.get("value").ok_or(INVALID)?, depth + 1)?,
            );
        }
        // URL/command/dialog/item/entity events have their own codecs and
        // restrictions. Do not treat an arbitrary action/value pair as valid.
        _ => return Err(UNSUPPORTED),
    }
    Ok(NbtTag::Compound(result))
}

fn require_keys(compound: &NbtCompound, allowed: &[&str]) -> Result<(), &'static str> {
    if compound
        .child_tags
        .keys()
        .all(|key| allowed.contains(&key.as_ref()))
    {
        Ok(())
    } else {
        Err(UNSUPPORTED)
    }
}

fn is_number(tag: &NbtTag) -> bool {
    // NbtOps accepts every numeric tag for BOOL/INT; TranslatableContents permits
    // every Java Number argument. Neither codec imposes a finite-number check.
    matches!(
        tag,
        NbtTag::Byte(_)
            | NbtTag::Short(_)
            | NbtTag::Int(_)
            | NbtTag::Long(_)
            | NbtTag::Float(_)
            | NbtTag::Double(_)
    )
}

fn simple_player_name(name: &str) -> bool {
    (1..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

// Nonempty ASCII identifiers are a safe subset. Other forms need the exact
// Identifier codec, so failure here means unsupported, not proven Java-invalid.
fn supported_identifier(value: &str) -> bool {
    let (namespace, path) = value.split_once(':').unwrap_or(("minecraft", value));
    !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
}

fn valid_color(color: &str) -> bool {
    if let Some(hex) = color.strip_prefix('#') {
        // TextColor.parseColor uses Integer.parseInt(radix=16), range 0..ffffff.
        return i32::from_str_radix(hex, 16).is_ok_and(|rgb| (0..=0xffffff).contains(&rgb));
    }
    matches!(
        color,
        "black"
            | "dark_blue"
            | "dark_green"
            | "dark_aqua"
            | "dark_red"
            | "dark_purple"
            | "gold"
            | "gray"
            | "dark_gray"
            | "blue"
            | "green"
            | "aqua"
            | "red"
            | "light_purple"
            | "yellow"
            | "white"
    )
}

fn project_text(tag: &NbtTag) -> String {
    match tag {
        NbtTag::String(text) => text.to_string(),
        NbtTag::List(children) => children.iter().map(project_text).collect(),
        NbtTag::Compound(component) => {
            let Some(text) = component
                .get_string("text")
                .or_else(|| component.get_string("translate"))
            else {
                return component.to_string();
            };
            let mut text = text.to_owned();
            if let Some(NbtTag::List(extra)) = component.get("extra") {
                for child in extra {
                    text.push_str(&project_text(child));
                }
            }
            text
        }
        _ => format!("{tag:?}"),
    }
}

fn hash_bytes(bytes: &[u8], digest: &mut Digest) {
    digest.update(&(bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

/// Stable across compound insertion order. NBT representation equality is conservative:
/// Rich forms are not fully canonicalized; plain text compounds normalize to literals.
fn hash_tag(tag: &NbtTag, digest: &mut Digest) {
    digest.update(&[tag.get_type_id()]);
    match tag {
        NbtTag::End => {}
        NbtTag::Byte(value) => digest.update(&value.to_le_bytes()),
        NbtTag::Short(value) => digest.update(&value.to_le_bytes()),
        NbtTag::Int(value) => digest.update(&value.to_le_bytes()),
        NbtTag::Long(value) => digest.update(&value.to_le_bytes()),
        // NBT equality considers the two signs of zero equal.
        NbtTag::Float(value) => {
            digest.update(&if *value == 0.0 { 0.0 } else { *value }.to_le_bytes())
        }
        NbtTag::Double(value) => {
            digest.update(&if *value == 0.0 { 0.0 } else { *value }.to_le_bytes())
        }
        NbtTag::String(value) => hash_bytes(value.as_bytes(), digest),
        NbtTag::ByteArray(values) => {
            digest.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                digest.update(&value.to_le_bytes());
            }
        }
        NbtTag::IntArray(values) => {
            digest.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                digest.update(&value.to_le_bytes());
            }
        }
        NbtTag::LongArray(values) => {
            digest.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                digest.update(&value.to_le_bytes());
            }
        }
        NbtTag::List(values) => {
            digest.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                hash_tag(value, digest);
            }
        }
        NbtTag::Compound(component) => {
            let mut entries: Vec<_> = component.child_tags.iter().collect();
            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            digest.update(&(entries.len() as u64).to_le_bytes());
            for (key, value) in entries {
                hash_bytes(key.as_bytes(), digest);
                hash_tag(value, digest);
            }
        }
    }
}
