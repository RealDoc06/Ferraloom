use std::borrow::Cow;

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

use super::{DataComponentImpl, Digest, get_str_hash};

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
    /// Retains style, arguments, siblings and content the text renderer cannot model.
    Component(NbtTag),
}

impl ItemNameImpl {
    #[must_use]
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        if !is_component(data, 0) {
            return None;
        }
        Some(match data {
            NbtTag::String(text) => Self::Literal {
                name: Cow::Owned(text.to_string()),
            },
            NbtTag::Compound(component) if component.child_tags.len() == 1 => {
                if let Some(key) = component.get_string("translate") {
                    Self::Translated {
                        name: Cow::Owned(key.to_owned()),
                    }
                } else {
                    Self::Component(data.clone())
                }
            }
            _ => Self::Component(data.clone()),
        })
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
        // Preserve the existing literal checksum. Structured components use a local,
        // deterministic structural checksum, NOT verified vanilla prediction hashes.
        // Never collapse a translation or styled component to its display projection.
        if let Self::Literal { name } = self {
            return get_str_hash(name) as i32;
        }
        let mut digest = Digest::new();
        hash_tag(&self.write_data(), &mut digest);
        digest.finalize() as i32
    }

    default_impl!(ItemName);
}

/// Validate known component shapes without round-tripping through the lossy renderer.
/// Unknown nonempty compounds are retained opaquely, not claimed fully validated.
fn is_component(tag: &NbtTag, depth: usize) -> bool {
    if depth > pumpkin_nbt::MAX_NBT_DEPTH {
        return false;
    }
    match tag {
        NbtTag::String(_) => true,
        NbtTag::List(children) => {
            !children.is_empty() && children.iter().all(|child| is_component(child, depth + 1))
        }
        NbtTag::Compound(component) => {
            if component.is_empty() {
                return false;
            }
            for key in [
                "text",
                "translate",
                "keybind",
                "selector",
                "nbt",
                "type",
                "fallback",
            ] {
                if component
                    .get(key)
                    .is_some_and(|value| !matches!(value, NbtTag::String(_)))
                {
                    return false;
                }
            }
            if component
                .get("score")
                .is_some_and(|value| !matches!(value, NbtTag::Compound(_)))
            {
                return false;
            }
            for key in ["extra", "with"] {
                if let Some(value) = component.get(key) {
                    let NbtTag::List(children) = value else {
                        return false;
                    };
                    if !children.iter().all(|child| {
                        // Translation arguments can also be numeric/boolean values.
                        (key == "with"
                            && matches!(
                                child,
                                NbtTag::Byte(_)
                                    | NbtTag::Short(_)
                                    | NbtTag::Int(_)
                                    | NbtTag::Long(_)
                                    | NbtTag::Float(_)
                                    | NbtTag::Double(_)
                            ))
                            || is_component(child, depth + 1)
                    }) {
                        return false;
                    }
                }
            }
            component
                .get("separator")
                .is_none_or(|value| is_component(value, depth + 1))
        }
        _ => false,
    }
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
/// e.g. a string and a text compound need not compare equal, even if they render alike.
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
