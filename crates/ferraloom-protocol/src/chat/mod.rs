//! Port of net.minecraft.network.chat's component tree and serialization codecs.
//! Components retain explicit false style flags, which override inherited styles.
mod json;
mod serialization;
use crate::{Identifier, Reader, Result, Uuid, Wire, Writer};
use ferraloom_nbt::{CompoundTag, Tag};

/// Port of MutableComponent: contents, local style, and appended siblings.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub contents: Contents,
    pub style: Style,
    pub extra: Vec<Component>,
}
impl Component {
    pub fn new(contents: Contents) -> Self {
        Self {
            contents,
            style: Style::default(),
            extra: Vec::new(),
        }
    }
    pub fn literal(text: impl Into<String>) -> Self {
        Self::new(Contents::Text(text.into()))
    }
    pub fn translatable(key: impl Into<String>, args: Vec<Argument>) -> Self {
        Self::new(Contents::Translatable {
            key: key.into(),
            fallback: None,
            args,
        })
    }
    pub fn from_nbt(tag: Tag) -> Result<Self> {
        serialization::decode(tag, true)
    }
    pub fn to_nbt(&self) -> Result<Tag> {
        serialization::encode(self)
    }
    /// Strict JSON syntax, followed by vanilla's component codec validation.
    pub fn from_json(json: &str) -> Result<Self> {
        let value =
            serde_json::from_str(json).map_err(|e| crate::Error::invalid("JSON", e.to_string()))?;
        serialization::decode_json(value)
    }
    /// Gson's legacy lenient syntax (comments, quotes, and unquoted tokens).
    pub fn from_json_lenient(json: &str) -> Result<Self> {
        serialization::decode_json(json::parse(json)?)
    }
    pub fn to_json(&self) -> Result<String> {
        Ok(serialization::encode_json(self)?.to_string())
    }
    pub fn decode_trusted(input: &mut Reader<'_>) -> Result<Self> {
        Self::from_nbt(input.nbt(true)?)
    }
}
impl Wire for Component {
    fn decode(input: &mut Reader<'_>) -> Result<Self> {
        Self::from_nbt(input.nbt(false)?)
    }
    fn encode(&self, output: &mut Writer<'_>) -> Result<()> {
        output.nbt(&self.to_nbt()?)
    }
}
impl Default for Component {
    fn default() -> Self {
        Self::literal("")
    }
}

/// Port of the seven ComponentContents codecs, including 26.3 object contents.
#[derive(Clone, Debug, PartialEq)]
pub enum Contents {
    Text(String),
    Translatable {
        key: String,
        fallback: Option<String>,
        args: Vec<Argument>,
    },
    Score {
        name: String,
        objective: String,
    },
    Selector {
        pattern: String,
        separator: Option<Box<Component>>,
    },
    Keybind(String),
    Nbt {
        path: String,
        interpret: bool,
        plain: bool,
        separator: Option<Box<Component>>,
        source: DataSource,
    },
    Object {
        object: ObjectContents,
        fallback: Option<Box<Component>>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Argument {
    String(String),
    Number(Numeric),
    Boolean(bool),
    Component(Box<Component>),
}
/// Primitive number kinds retained by ExtraCodecs.JAVA and JavaOps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Numeric {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
}
impl Numeric {
    pub fn to_nbt(self) -> Tag {
        match self {
            Self::Byte(v) => Tag::Byte(v),
            Self::Short(v) => Tag::Short(v),
            Self::Int(v) => Tag::Int(v),
            Self::Long(v) => Tag::Long(v),
            Self::Float(v) => Tag::Float(v),
            Self::Double(v) => Tag::Double(v),
        }
    }
    pub fn from_nbt(tag: &Tag) -> Option<Self> {
        Some(match tag {
            Tag::Byte(v) => Self::Byte(*v),
            Tag::Short(v) => Self::Short(*v),
            Tag::Int(v) => Self::Int(*v),
            Tag::Long(v) => Self::Long(*v),
            Tag::Float(v) => Self::Float(*v),
            Tag::Double(v) => Self::Double(*v),
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum DataSource {
    Block(String),
    Entity(String),
    Storage(Identifier),
}
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectContents {
    Atlas {
        atlas: Identifier,
        sprite: Identifier,
    },
    Player {
        profile: ResolvableProfile,
        hat: bool,
    },
}

/// Port of ResolvableProfile's data codec. Skin patch fields use their vanilla NBT
/// values so the game layer can resolve textures independently of packet decoding.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvableProfile {
    pub name: Option<String>,
    pub id: Option<Uuid>,
    pub properties: Vec<ProfileProperty>,
    pub skin_patch: CompoundTag,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

/// Port of Style. None means inherit; Some(false) explicitly disables a flag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub color: Option<TextColor>,
    pub shadow_color: Option<i32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub click_event: Option<ClickEvent>,
    pub hover_event: Option<HoverEvent>,
    pub insertion: Option<String>,
    pub font: Option<Identifier>,
}
impl Style {
    /// Port of Style#applyTo. Local values take precedence over inherited ones.
    pub fn apply_to(&self, parent: &Self) -> Self {
        Self {
            color: self.color.clone().or_else(|| parent.color.clone()),
            shadow_color: self.shadow_color.or(parent.shadow_color),
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            underlined: self.underlined.or(parent.underlined),
            strikethrough: self.strikethrough.or(parent.strikethrough),
            obfuscated: self.obfuscated.or(parent.obfuscated),
            click_event: self
                .click_event
                .clone()
                .or_else(|| parent.click_event.clone()),
            hover_event: self
                .hover_event
                .clone()
                .or_else(|| parent.hover_event.clone()),
            insertion: self.insertion.clone().or_else(|| parent.insertion.clone()),
            font: self.font.clone().or_else(|| parent.font.clone()),
        }
    }
}

/// Named colors retain their name; custom colors serialize as uppercase #RRGGBB.
#[derive(Clone, Debug, PartialEq)]
pub struct TextColor {
    value: u32,
    name: Option<&'static str>,
}
impl TextColor {
    pub fn from_rgb(rgb: u32) -> Self {
        Self {
            value: rgb & 0xffffff,
            name: None,
        }
    }
    pub fn value(&self) -> u32 {
        self.value
    }
    pub fn parse(text: &str) -> Result<Self> {
        serialization::color(text)
    }
    pub fn serialize(&self) -> String {
        self.name
            .map_or_else(|| format!("#{:06X}", self.value), str::to_owned)
    }
}

/// Port of ClickEvent. OpenFile is deliberately absent: vanilla's CODEC rejects it.
#[derive(Clone, Debug, PartialEq)]
pub enum ClickEvent {
    OpenUrl(String),
    RunCommand(String),
    SuggestCommand(String),
    ChangePage(i32),
    CopyToClipboard(String),
    Custom {
        id: Identifier,
        payload: Option<Tag>,
    },
    /// The dialog registry/data codec is supplied by the game layer; its named
    /// or inline NBT representation is preserved without interpreting dialog UI.
    ShowDialog(Tag),
}
#[derive(Clone, Debug, PartialEq)]
pub enum HoverEvent {
    ShowText(Box<Component>),
    ShowEntity {
        entity_type: Identifier,
        uuid: Uuid,
        name: Option<Box<Component>>,
    },
    ShowItem {
        item: Identifier,
        count: u8,
        components: CompoundTag,
    },
}

#[cfg(test)]
mod tests;
