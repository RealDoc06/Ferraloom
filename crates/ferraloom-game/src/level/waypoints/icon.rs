//! Waypoint.Icon's persistent codec. The packet uses the existing protocol type.
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    play::{RgbColor, WaypointIcon},
};

pub fn default_icon() -> WaypointIcon {
    WaypointIcon {
        style: "minecraft:default"
            .parse()
            .expect("native style identifier"),
        color: None,
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IconDecodeError(&'static str);
impl std::fmt::Display for IconDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for IconDecodeError {}
fn number_int(tag: &Tag) -> Option<i32> {
    Some(match tag {
        Tag::Byte(value) => i32::from(*value),
        Tag::Short(value) => i32::from(*value),
        Tag::Int(value) => *value,
        Tag::Long(value) => *value as i32,
        Tag::Float(value) => *value as i32,
        Tag::Double(value) => *value as i32,
        _ => return None,
    })
}
fn rgb(tag: &Tag) -> Result<u32, IconDecodeError> {
    if let Some(color) = number_int(tag) {
        return Ok(color as u32);
    }
    let values = match tag {
        Tag::List(values) => values
            .0
            .iter()
            .map(Tag::as_float)
            .collect::<Option<Vec<_>>>(),
        Tag::ByteArray(values) => Some(values.iter().map(|value| f32::from(*value)).collect()),
        Tag::IntArray(values) => Some(values.iter().map(|value| *value as f32).collect()),
        Tag::LongArray(values) => Some(values.iter().map(|value| *value as f32).collect()),
        _ => None,
    }
    .ok_or(IconDecodeError("expected an RGB integer or three floats"))?;
    let [red, green, blue] = values.as_slice() else {
        return Err(IconDecodeError("expected three RGB floats"));
    };
    let channel = |value: f32| ferraloom_util::mth::floor(f64::from(value * 255.0_f32));
    Ok(((-1_i32).wrapping_shl(24)
        | channel(*red).wrapping_shl(16)
        | channel(*green).wrapping_shl(8)
        | channel(*blue)) as u32)
}
/// Port of Waypoint.Icon#CODEC through NbtOps. Missing data uses the native icon.
pub fn load_icon(tag: Option<&Tag>) -> Result<WaypointIcon, IconDecodeError> {
    let Some(tag) = tag else {
        return Ok(default_icon());
    };
    let tag = tag
        .as_compound()
        .ok_or(IconDecodeError("expected waypoint icon compound"))?;
    let style = tag
        .get("style")
        .and_then(Tag::as_string)
        .ok_or(IconDecodeError("missing waypoint icon style"))?;
    let style = Identifier::parse(&style.to_string_lossy())
        .map_err(|_| IconDecodeError("invalid waypoint icon style"))?;
    let color = tag.get("color").map(rgb).transpose()?.map(RgbColor);
    Ok(WaypointIcon { style, color })
}
/// Port of Waypoint.Icon#CODEC's canonical persistent representation.
pub fn save_icon(icon: &WaypointIcon) -> Tag {
    let mut tag = CompoundTag::new();
    tag.insert("style", Tag::String(icon.style.to_string().into()));
    if let Some(color) = &icon.color {
        tag.insert("color", Tag::Int(color.0 as i32));
    }
    Tag::Compound(tag)
}
