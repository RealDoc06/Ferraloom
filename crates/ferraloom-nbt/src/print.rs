use crate::{CompoundTag, ListTag, NbtString, Tag};
use std::fmt;

#[derive(Default)]
struct Output(Vec<u16>);
impl Output {
    fn text(&mut self, text: &str) {
        self.0.extend(text.encode_utf16());
    }
    fn tag(&mut self, text: NbtString) {
        self.0.extend(text.as_utf16());
    }
    fn finish(self) -> NbtString {
        NbtString::from_utf16(self.0)
    }
}
/// Port of `StringTag#quoteAndEscape`. The UTF-16 form preserves unpaired surrogates.
pub fn quote_utf16(value: &NbtString) -> NbtString {
    let quote = value
        .as_utf16()
        .iter()
        .find_map(|c| match c {
            34 => Some(39),
            39 => Some(34),
            _ => None,
        })
        .unwrap_or(34);
    let mut out = Output::default();
    out.0.push(quote);
    for &c in value.as_utf16() {
        match c {
            92 => out.text("\\\\"),
            8 => out.text("\\b"),
            9 => out.text("\\t"),
            10 => out.text("\\n"),
            12 => out.text("\\f"),
            13 => out.text("\\r"),
            0..=31 => out.text(&format!("\\x{c:02X}")),
            _ => {
                if c == quote {
                    out.0.push(92)
                }
                out.0.push(c)
            }
        }
    }
    out.0.push(quote);
    out.finish()
}
/// UTF-8 convenience form; use `quote_utf16` for Java strings with unpaired surrogates.
pub fn quote(value: &NbtString) -> String {
    quote_utf16(value).to_string_lossy()
}
fn key(value: &NbtString, strict: bool) -> NbtString {
    let text = value.to_string_lossy();
    let simple = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    let strict_ok = text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '.' || c == '_')
        && !text.eq_ignore_ascii_case("true")
        && !text.eq_ignore_ascii_case("false");
    if simple && (!strict || strict_ok) {
        value.clone()
    } else {
        quote_utf16(value)
    }
}
// Java 25 selects the closest shortest round-tripping decimal, considering two
// significant digits when a one-digit decimal already round trips (JLS 4.2.3).
fn float_string(value: f64, single: bool) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        }
        .into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let mut decimal = String::new();
    for digits in 1..=if single { 9 } else { 17 } {
        if let Some(candidate) = decimal_candidate(value, single, digits) {
            decimal = if digits == 1 {
                decimal_candidate(value, single, 2).unwrap_or(candidate)
            } else {
                candidate
            };
            break;
        }
    }
    let (mantissa, exponent) = decimal.split_once('e').expect("finite decimal");
    let exponent: i32 = exponent.parse().expect("decimal exponent");
    let negative = mantissa.starts_with('-');
    let digits = mantissa.trim_start_matches('-').replace('.', "");
    let digits = digits.trim_end_matches('0');
    let sign = if negative { "-" } else { "" };
    if !(-3..7).contains(&exponent) {
        format!(
            "{sign}{}.{}E{exponent}",
            &digits[..1],
            if digits.len() == 1 { "0" } else { &digits[1..] }
        )
    } else if exponent < 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    } else {
        let point = exponent as usize + 1;
        if point >= digits.len() {
            format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    }
}

// At powers of two the rounding interval is asymmetric. If the closest
// decimal misses that interval, its next neighbor can still round trip.
fn decimal_candidate(value: f64, single: bool, digits: usize) -> Option<String> {
    let nearest = format!("{:.*e}", digits - 1, value);
    let roundtrips = |s: &str| {
        if single {
            s.parse::<f32>()
                .is_ok_and(|v| v.to_bits() == (value as f32).to_bits())
        } else {
            s.parse::<f64>()
                .is_ok_and(|v| v.to_bits() == value.to_bits())
        }
    };
    if roundtrips(&nearest) {
        return Some(nearest);
    }
    let (mantissa, exponent) = nearest.split_once('e')?;
    let coefficient = mantissa
        .trim_start_matches('-')
        .replace('.', "")
        .parse::<i64>()
        .ok()?;
    let scale = exponent.parse::<i32>().ok()? - digits as i32 + 1;
    for coefficient in [coefficient - 1, coefficient + 1] {
        if coefficient <= 0 {
            continue;
        }
        let coefficient = coefficient.to_string();
        let exponent = scale + coefficient.len() as i32 - 1;
        let candidate = format!(
            "{}{}.{}e{exponent}",
            if value.is_sign_negative() { "-" } else { "" },
            &coefficient[..1],
            if coefficient.len() == 1 {
                "0"
            } else {
                &coefficient[1..]
            }
        );
        if roundtrips(&candidate) {
            return Some(candidate);
        }
    }
    None
}
fn scalar(tag: &Tag) -> Option<NbtString> {
    Some(match tag {
        Tag::End => "END".into(),
        Tag::Byte(n) => format!("{n}b").into(),
        Tag::Short(n) => format!("{n}s").into(),
        Tag::Int(n) => n.to_string().into(),
        Tag::Long(n) => format!("{n}L").into(),
        Tag::Float(n) => format!("{}f", float_string(f64::from(*n), true)).into(),
        Tag::Double(n) => format!("{}d", float_string(*n, false)).into(),
        Tag::String(s) => quote_utf16(s),
        _ => return None,
    })
}
impl Tag {
    /// Exact `StringTagVisitor` output in Java UTF-16. `Display` replaces lone surrogates with U+FFFD.
    pub fn to_snbt_utf16(&self) -> NbtString {
        compact(self)
    }
}
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&compact(self).to_string_lossy())
    }
}
impl fmt::Display for CompoundTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Tag::Compound(self.clone()).fmt(f)
    }
}
impl fmt::Display for ListTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Tag::List(self.clone()).fmt(f)
    }
}
fn compact(tag: &Tag) -> NbtString {
    if let Some(s) = scalar(tag) {
        return s;
    }
    let mut out = Output::default();
    match tag {
        Tag::Compound(c) => {
            out.text("{");
            for (i, (k, v)) in c.iter().enumerate() {
                if i > 0 {
                    out.text(",")
                }
                out.tag(key(k, true));
                out.text(":");
                out.tag(compact(v));
            }
            out.text("}");
        }
        Tag::List(v) => {
            out.text("[");
            for (i, v) in v.iter().enumerate() {
                if i > 0 {
                    out.text(",")
                }
                out.tag(compact(v));
            }
            out.text("]");
        }
        _ => return array_string(tag, false, false),
    }
    out.finish()
}
fn array_string(tag: &Tag, spaces: bool, truncate: bool) -> NbtString {
    let (prefix, values) = match tag {
        Tag::ByteArray(v) => (
            "B",
            v.iter()
                .map(|v| format!("{v}{}", if truncate { "b" } else { "B" }))
                .collect::<Vec<_>>(),
        ),
        Tag::IntArray(v) => ("I", v.iter().map(ToString::to_string).collect()),
        Tag::LongArray(v) => ("L", v.iter().map(|v| format!("{v}L")).collect()),
        _ => unreachable!(),
    };
    let mut out = format!("[{prefix};");
    let count = if truncate {
        values.len().min(128)
    } else {
        values.len()
    };
    for (i, value) in values.iter().take(count).enumerate() {
        if spaces {
            out.push(' ')
        }
        out.push_str(value);
        if i + 1 < values.len() {
            out.push(',')
        }
    }
    if count < values.len() {
        out.push_str("<...>")
    }
    out.push(']');
    out.into()
}
/// Port of `SnbtPrinterTagVisitor`, including structure-template key ordering and indentation.
pub fn pretty(tag: &Tag) -> String {
    pretty_utf16(tag).to_string_lossy()
}
pub fn pretty_utf16(tag: &Tag) -> NbtString {
    pretty_inner(tag, "    ", 0, &mut Vec::new())
}
fn pretty_inner(tag: &Tag, indent: &str, depth: usize, path: &mut Vec<String>) -> NbtString {
    if let Some(value) = scalar(tag) {
        return if matches!(tag, Tag::End) {
            NbtString::default()
        } else {
            value
        };
    }
    let (open, close) = match tag {
        Tag::Compound(c) if c.is_empty() => return "{}".into(),
        Tag::List(v) if v.is_empty() => return "[]".into(),
        Tag::Compound(_) => ("{", "}"),
        Tag::List(_) => ("[", "]"),
        _ => return array_string(tag, true, false),
    };
    path.push(if open == "{" { "{}" } else { "[]" }.into());
    let path_string = path.join(".");
    let indent = if matches!(
        path_string.as_str(),
        "{}.size.[]" | "{}.data.[].{}" | "{}.palette.[].{}" | "{}.entities.[].{}"
    ) {
        ""
    } else {
        indent
    };
    let values = match tag {
        Tag::List(v) => v
            .iter()
            .map(|v| pretty_inner(v, indent, depth + 1, path))
            .collect::<Vec<_>>(),
        Tag::Compound(c) => {
            let priority: &[&str] = match path_string.as_str() {
                "{}" => &[
                    "DataVersion",
                    "author",
                    "size",
                    "data",
                    "entities",
                    "palette",
                    "palettes",
                ],
                "{}.data.[].{}" => &["pos", "state", "nbt"],
                "{}.entities.[].{}" => &["blockPos", "pos"],
                _ => &[],
            };
            let mut keys = c.iter().map(|(k, _)| k).collect::<Vec<_>>();
            keys.sort_by_key(|k| {
                priority
                    .iter()
                    .position(|p| **k == NbtString::from(*p))
                    .unwrap_or(priority.len())
            });
            keys.into_iter()
                .map(|k| {
                    path.push(k.to_string_lossy());
                    let mut out = Output::default();
                    out.tag(key(k, false));
                    out.text(": ");
                    out.tag(pretty_inner(c.get_key(k).unwrap(), indent, depth + 1, path));
                    path.pop();
                    out.finish()
                })
                .collect()
        }
        _ => unreachable!(),
    };
    path.pop();
    format_container(open, close, values, indent, depth)
}
fn format_container(
    open: &str,
    close: &str,
    values: Vec<NbtString>,
    indent: &str,
    depth: usize,
) -> NbtString {
    let mut out = Output::default();
    out.text(open);
    if !indent.is_empty() {
        out.text("\n")
    }
    for (i, value) in values.into_iter().enumerate() {
        if i > 0 {
            out.text(if indent.is_empty() { ", " } else { ",\n" })
        }
        out.text(&indent.repeat(depth + 1));
        out.tag(value);
    }
    if !indent.is_empty() {
        out.text("\n");
        out.text(&indent.repeat(depth))
    }
    out.text(close);
    out.finish()
}
/// Plain text from `TextComponentTagVisitor`, as used by `/data get`. `sort_keys` mirrors its debug option.
pub fn to_text(tag: &Tag, indent: &str, sort_keys: bool) -> String {
    to_text_utf16(tag, indent, sort_keys).to_string_lossy()
}
pub fn to_text_utf16(tag: &Tag, indent: &str, sort_keys: bool) -> NbtString {
    text_inner(tag, indent, sort_keys, 0, 0)
}
fn text_inner(tag: &Tag, indent: &str, sort: bool, depth: usize, indent_depth: usize) -> NbtString {
    if let Some(s) = scalar(tag) {
        return if matches!(tag, Tag::End) {
            NbtString::default()
        } else {
            s
        };
    }
    match tag {
        Tag::List(v) => {
            if v.is_empty() {
                return "[]".into();
            }
            if depth >= 64 {
                return "[<...>]".into();
            }
            let wrap = v.len() < 8 && v.iter().any(|t| t.as_double().is_none());
            let values = v
                .iter()
                .map(|t| text_inner(t, indent, sort, depth + 1, indent_depth + usize::from(wrap)))
                .collect::<Vec<_>>();
            format_container(
                "[",
                "]",
                values,
                if wrap { indent } else { "" },
                indent_depth,
            )
        }
        Tag::Compound(c) => {
            if c.is_empty() {
                return "{}".into();
            }
            if depth >= 64 {
                return "{<...>}".into();
            }
            let keys = if sort {
                c.iter().map(|(k, _)| k).collect()
            } else {
                c.java_keys()
            };
            let values = keys
                .into_iter()
                .map(|k| {
                    let mut out = Output::default();
                    out.tag(key(k, false));
                    out.text(": ");
                    out.tag(text_inner(
                        c.get_key(k).unwrap(),
                        indent,
                        sort,
                        depth + 1,
                        indent_depth + 1,
                    ));
                    out.finish()
                })
                .collect();
            format_container("{", "}", values, indent, indent_depth)
        }
        _ => array_string(tag, true, true),
    }
}
