//! Pack metadata schema. Protocol-owned component/scalar codecs are supplied by
//! the caller so the data leaf never adds a dependency on protocol or game.
use super::{Error, FeatureFlags};
use serde_json::Value;

pub trait MetadataCodecs {
    type Description;
    fn description(&self, value: &Value) -> Result<Self::Description, Error>;
    fn integer(&self, value: &Value) -> Result<i32, Error>;
}

/// Port of PackFormat, ordered by major and then minor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackFormat {
    pub major: i32,
    pub minor: i32,
}
impl PackFormat {
    pub const SERVER: Self = Self {
        major: 121,
        minor: 0,
    };
    pub const LAST_PRE_MINOR: i32 = 81;
    pub const fn new(major: i32, minor: i32) -> Self {
        Self { major, minor }
    }
    fn decode(value: &Value, minor: i32, codecs: &impl MetadataCodecs) -> Result<Self, Error> {
        let (major, minor) = if let Some(values) = value.as_array() {
            if values.is_empty() || values.len() > 256 {
                return Err(invalid("pack format list size must be between 1 and 256"));
            }
            let decoded = values
                .iter()
                .map(|value| {
                    let value = codecs.integer(value)?;
                    if value < 0 {
                        return Err(invalid("pack format must be non-negative"));
                    }
                    Ok(value)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            (decoded[0], decoded.get(1).copied().unwrap_or(minor))
        } else {
            (codecs.integer(value)?, minor)
        };
        if major < 0 || minor < 0 {
            return Err(invalid("pack format must be non-negative"));
        }
        Ok(Self { major, minor })
    }
}
impl std::fmt::Display for PackFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.minor == i32::MAX {
            write!(f, "{}.*", self.major)
        } else {
            write!(f, "{}.{}", self.major, self.minor)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatRange {
    pub min: PackFormat,
    pub max: PackFormat,
}
impl FormatRange {
    pub fn contains(self, format: PackFormat) -> bool {
        self.min <= format && format <= self.max
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackCompatibility {
    TooOld,
    TooNew,
    Compatible,
}
impl PackCompatibility {
    pub fn for_version(range: FormatRange, version: PackFormat) -> Self {
        if range.max < version {
            Self::TooOld
        } else if range.min > version {
            Self::TooNew
        } else {
            Self::Compatible
        }
    }
}
#[derive(Clone, Debug)]
struct IntermediaryFormat {
    min: Option<PackFormat>,
    max: Option<PackFormat>,
    format: Option<i32>,
    supported: Option<(i32, i32)>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidData(message.into())
}
impl IntermediaryFormat {
    fn decode(value: &Value, overlay: bool, codecs: &impl MetadataCodecs) -> Result<Self, Error> {
        if !value.is_object() {
            return Err(invalid("expected pack format object"));
        }
        let min = value
            .get("min_format")
            .map(|v| PackFormat::decode(v, 0, codecs))
            .transpose()?;
        let max = value
            .get("max_format")
            .map(|v| PackFormat::decode(v, i32::MAX, codecs))
            .transpose()?;
        let format = if overlay {
            min.map(|v| v.major)
        } else {
            value
                .get("pack_format")
                .map(|v| codecs.integer(v))
                .transpose()?
        };
        let supported = value
            .get(if overlay {
                "formats"
            } else {
                "supported_formats"
            })
            .map(|v| integer_range(v, codecs))
            .transpose()?;
        Ok(Self {
            min,
            max,
            format,
            supported,
        })
    }
    fn effective_min(&self) -> i32 {
        match (self.min, self.supported) {
            (Some(a), Some(b)) => a.major.min(b.0),
            (Some(a), None) => a.major,
            (None, Some(b)) => b.0,
            (None, None) => i32::MAX,
        }
    }
    fn validate(
        &self,
        overlay: bool,
        require_old: bool,
        context: &str,
    ) -> Result<FormatRange, Error> {
        let old_name = if overlay {
            "formats"
        } else {
            "supported_formats"
        };
        let last = PackFormat::LAST_PRE_MINOR;
        if self.min.is_some() != self.max.is_some() {
            return Err(invalid(format!(
                "{context} missing field, must declare both min_format and max_format"
            )));
        }
        if require_old && self.supported.is_none() {
            return Err(invalid(format!(
                "{context} missing required field {old_name}, must be present in all overlays for any overlays to work across game versions"
            )));
        }
        if let (Some(min), Some(max)) = (self.min, self.max) {
            if min > max {
                return Err(invalid(format!(
                    "{context} min_format ({min}) is greater than max_format ({max})"
                )));
            }
            if min.major > last && !require_old {
                if self.supported.is_some() {
                    return Err(invalid(format!(
                        "{context} key {old_name} is deprecated starting from pack format {}. Remove {old_name} from your pack.mcmeta.",
                        last + 1
                    )));
                }
                if !overlay && self.format.is_some() {
                    self.validate_main(min.major, max.major)?;
                }
            } else {
                let (old_min,old_max) = self.supported.ok_or_else(|| invalid(format!("{context} declares support for format {}, but game versions supporting formats 15 to {last} require a {old_name} field. Add \"{old_name}\": [{}, {last}] or require a version greater or equal to {}.0.",min.major,min.major,last+1)))?;
                if old_min != min.major {
                    return Err(invalid(format!(
                        "{context} version declaration mismatch between {old_name} (from {old_min}) and min_format ({min})"
                    )));
                }
                if old_max != max.major && old_max != last {
                    return Err(invalid(format!(
                        "{context} version declaration mismatch between {old_name} (up to {old_max}) and max_format ({max})"
                    )));
                }
                if !overlay {
                    self.require_main(old_min, context)?;
                    self.validate_main(min.major, max.major)?;
                }
            }
            return Ok(FormatRange { min, max });
        }
        if let Some((min, max)) = self.supported {
            if max > last {
                return Err(invalid(format!(
                    "{context} declares support for version newer than {last}, but is missing mandatory fields min_format and max_format"
                )));
            }
            if !overlay {
                self.require_main(min, context)?;
                self.validate_main(min, max)?;
            }
            return Ok(FormatRange {
                min: PackFormat::new(min, 0),
                max: PackFormat::new(max, 0),
            });
        }
        if !overlay && let Some(format) = self.format {
            if format > last {
                return Err(invalid(format!(
                    "{context} declares support for version newer than {last}, but is missing mandatory fields min_format and max_format"
                )));
            }
            return Ok(FormatRange {
                min: PackFormat::new(format, 0),
                max: PackFormat::new(format, 0),
            });
        }
        Err(invalid(format!(
            "{context} could not be parsed, missing format version information"
        )))
    }
    fn require_main(&self, min: i32, context: &str) -> Result<(), Error> {
        if self.format.is_none() {
            return Err(invalid(format!(
                "{context} declares support for formats up to 81, but game versions supporting formats 15 to 81 require a pack_format field. Add \"pack_format\": {min} or require a version greater or equal to 82.0."
            )));
        }
        Ok(())
    }
    fn validate_main(&self, min: i32, max: i32) -> Result<(), Error> {
        let format = self.format.expect("present main pack format");
        if format < min || format > max {
            return Err(invalid(format!(
                "Pack declared support for versions {min} to {max} but declared main format is {format}"
            )));
        }
        if format < 15 {
            return Err(invalid(
                "Multi-version packs cannot support minimum version of less than 15, since this will leave versions in range unable to load pack.",
            ));
        }
        Ok(())
    }
}
fn integer_range(value: &Value, codecs: &impl MetadataCodecs) -> Result<(i32, i32), Error> {
    let (min, max) = if let Some(values) = value.as_array() {
        if values.len() != 2 {
            return Err(invalid("inclusive range list must have two elements"));
        }
        (codecs.integer(&values[0])?, codecs.integer(&values[1])?)
    } else if value.is_object() {
        (
            codecs.integer(
                value
                    .get("min_inclusive")
                    .ok_or_else(|| invalid("missing min_inclusive"))?,
            )?,
            codecs.integer(
                value
                    .get("max_inclusive")
                    .ok_or_else(|| invalid("missing max_inclusive"))?,
            )?,
        )
    } else {
        let value = codecs.integer(value)?;
        (value, value)
    };
    if min > max {
        return Err(invalid(
            "min_inclusive must be less than or equal to max_inclusive",
        ));
    }
    Ok((min, max))
}

/// Port of Pack#readPackMetadata, including the description-only fallback when
/// the version codec fails. The caller logs the returned fallback diagnostic.
pub struct PackMetadata<D> {
    pub description: D,
    pub formats: FormatRange,
    pub compatibility: PackCompatibility,
    pub features: FeatureFlags,
    pub overlays: Vec<String>,
    pub format_fallback: Option<String>,
}
impl<D> PackMetadata<D> {
    pub fn decode(
        value: &Value,
        codecs: &impl MetadataCodecs<Description = D>,
    ) -> Result<Self, Error> {
        let pack = value
            .get("pack")
            .ok_or_else(|| invalid("missing pack metadata"))?;
        let description = codecs.description(
            pack.get("description")
                .ok_or_else(|| invalid("missing pack description"))?,
        )?;
        let (formats, format_fallback) = match IntermediaryFormat::decode(pack, false, codecs)
            .and_then(|v| v.validate(false, false, "Pack"))
        {
            Ok(formats) => (formats, None),
            Err(error) => (
                FormatRange {
                    min: PackFormat::new(i32::MAX, 0),
                    max: PackFormat::new(i32::MAX, 0),
                },
                Some(error.to_string()),
            ),
        };
        let features = if let Some(features) = value.get("features") {
            let names = features
                .get("enabled")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("missing feature list"))?;
            FeatureFlags::from_names(
                names
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| invalid("feature id must be string"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )?
        } else {
            FeatureFlags::EMPTY
        };
        let overlays = decode_overlays(value.get("overlays"), codecs)?;
        Ok(Self {
            description,
            formats,
            compatibility: PackCompatibility::for_version(formats, PackFormat::SERVER),
            features,
            overlays,
            format_fallback,
        })
    }
}
fn decode_overlays(
    value: Option<&Value>,
    codecs: &impl MetadataCodecs,
) -> Result<Vec<String>, Error> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("missing overlay entries"))?;
    let mut decoded = Vec::new();
    for entry in entries {
        let directory = entry
            .get("directory")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("missing overlay directory"))?;
        if directory.is_empty()
            || !directory
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        {
            return Err(invalid(format!(
                "{directory} is not accepted directory name"
            )));
        }
        decoded.push((directory, IntermediaryFormat::decode(entry, true, codecs)?));
    }
    let require_old = decoded
        .iter()
        .map(|(_, v)| v.effective_min())
        .min()
        .unwrap_or(i32::MAX)
        <= PackFormat::LAST_PRE_MINOR;
    let mut result = Vec::new();
    for (directory, format) in decoded {
        if format.min.is_none() && format.max.is_none() && format.supported.is_none() {
            continue;
        }
        let range = format.validate(true, require_old, &format!("Overlay \"{directory}\""))?;
        if range.contains(PackFormat::SERVER) {
            result.push(directory.to_owned());
        }
    }
    Ok(result)
}
