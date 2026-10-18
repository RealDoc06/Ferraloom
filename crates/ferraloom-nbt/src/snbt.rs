use crate::{CompoundTag, ListTag, NbtString, Tag};
use std::fmt;

/// Brigadier-compatible raw message and UTF-16 cursor. Translation remains the command layer's job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub cursor: usize,
    pub message: String,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.message, self.cursor)
    }
}
impl std::error::Error for ParseError {}

/// Port of `TagParser#parseFully` using the 26.3 `SnbtGrammar`.
pub fn parse(input: &str) -> Result<Tag, ParseError> {
    let (tag, cursor) = parse_prefix(input)?;
    let mut p = Parser::at(input, cursor);
    p.ws();
    if p.pos < p.text.len() {
        Err(p.error("Unexpected trailing data"))
    } else {
        Ok(tag)
    }
}
pub fn parse_compound(input: &str) -> Result<CompoundTag, ParseError> {
    let tag = parse(input)?;
    if let Tag::Compound(c) = tag {
        Ok(c)
    } else {
        Err(ParseError {
            cursor: input.encode_utf16().count(),
            message: "Expected compound tag".into(),
        })
    }
}
pub fn parse_prefix(input: &str) -> Result<(Tag, usize), ParseError> {
    let mut p = Parser::at(input, 0);
    let tag = p.literal().ok_or_else(|| {
        p.best
            .clone()
            .unwrap_or_else(|| p.error("Expected a valid unquoted string"))
    })?;
    Ok((tag, p.pos))
}

/// Expected tokens at the grammar's longest error position, measured in UTF-16.
/// The command layer filters these values against the remaining input and sorts them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParseSuggestions {
    pub cursor: usize,
    pub suggestions: Vec<String>,
}
/// Port of SnbtGrammar/Grammar#parseForSuggestions. This runs the same parser and
/// records speculative expectations without changing its result or diagnostics.
pub fn parse_suggestions(input: &str) -> ParseSuggestions {
    let mut parser = Parser::at(input, 0);
    parser.expected = Some(ExpectedTokens::default());
    let success = parser.literal().is_some();
    let mut expected = parser.expected.take().unwrap();
    if success {
        expected.advance(parser.pos);
    }
    ParseSuggestions {
        cursor: expected.cursor.unwrap_or(0),
        suggestions: expected.values,
    }
}
#[derive(Default)]
struct ExpectedTokens {
    cursor: Option<usize>,
    values: Vec<String>,
}
impl ExpectedTokens {
    fn advance(&mut self, cursor: usize) {
        if self.cursor.is_none_or(|previous| cursor > previous) {
            self.cursor = Some(cursor);
            self.values.clear();
        }
    }
    fn store(&mut self, cursor: usize, values: impl IntoIterator<Item = String>) {
        self.advance(cursor);
        if self.cursor == Some(cursor) {
            self.values.extend(values);
        }
    }
}

pub(crate) struct Parser {
    text: Vec<u16>,
    pub(crate) pos: usize,
    best: Option<ParseError>,
    expected: Option<ExpectedTokens>,
}
#[derive(Clone)]
struct Integer {
    negative: bool,
    radix: u32,
    digits: String,
    signed: Option<bool>,
    kind: Option<char>,
}
impl Parser {
    pub(crate) fn at(input: &str, pos: usize) -> Self {
        Self {
            text: input.encode_utf16().collect(),
            pos,
            best: None,
            expected: None,
        }
    }
    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            cursor: self.pos,
            message: message.into(),
        }
    }
    fn record<T>(&mut self, cursor: usize, message: impl Into<String>) -> Option<T> {
        self.expect(cursor, []);
        if self.best.as_ref().is_none_or(|e| cursor > e.cursor) {
            self.best = Some(ParseError {
                cursor,
                message: message.into(),
            });
        }
        None
    }
    fn expect(&mut self, cursor: usize, values: impl IntoIterator<Item = String>) {
        if let Some(expected) = &mut self.expected {
            expected.store(cursor, values);
        }
    }
    fn fail<T>(&mut self, message: impl Into<String>) -> Option<T> {
        self.record(self.pos, message)
    }
    fn attempt<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let pos = self.pos;
        let result = f(self);
        if result.is_none() {
            self.pos = pos;
        }
        result
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(java_whitespace) {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<u16> {
        self.text.get(self.pos).copied()
    }
    fn chars(&mut self, chars: &str) -> Option<char> {
        self.ws();
        let start = self.pos;
        if let Some(c) = self.peek() {
            self.pos += 1;
            if chars.chars().any(|v| v as u16 == c) {
                return char::from_u32(u32::from(c));
            }
        }
        self.expect(start, chars.chars().map(|c| c.to_string()));
        self.record(
            start,
            format!(
                "Expected literal {}",
                chars
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        )
    }
    fn ch(&mut self, c: char) -> Option<()> {
        self.chars(&c.to_string()).map(|_| ())
    }
    fn optional_char(&mut self, c: char) -> bool {
        self.attempt(|p| p.ch(c)).is_some()
    }
    fn sign(&mut self) -> bool {
        if self.attempt(|p| p.ch('+')).is_some() {
            false
        } else {
            self.attempt(|p| p.ch('-')).is_some()
        }
    }
    fn run(&mut self, radix: u32) -> Option<String> {
        self.ws();
        let start = self.pos;
        while self.peek().is_some_and(|c| {
            c == 95 || char::from_u32(u32::from(c)).is_some_and(|c| c.is_digit(radix))
        }) {
            self.pos += 1;
        }
        let end = self.pos;
        if end == start {
            return self.fail(match radix {
                2 => "Expected a binary number",
                16 => "Expected a hexadecimal number",
                _ => "Expected a decimal number",
            });
        }
        if self.text[start] == 95 || self.text[end - 1] == 95 {
            self.pos = start;
            return self
                .fail("Underscore characters are not allowed at the start or end of a number");
        }
        Some(String::from_utf16_lossy(&self.text[start..end]))
    }
    fn suffix(&mut self) -> Option<(Option<bool>, char)> {
        for (prefix, signed) in [("uU", false), ("sS", true)] {
            if let Some(v) = self.attempt(|p| {
                p.chars(prefix)?;
                let kind = p.type_suffix()?;
                Some((Some(signed), kind))
            }) {
                return Some(v);
            }
        }
        self.type_suffix().map(|c| (None, c))
    }
    fn type_suffix(&mut self) -> Option<char> {
        for chars in ["bB", "sS", "iI", "lL"] {
            if let Some(c) = self.attempt(|p| p.chars(chars)) {
                return Some(c.to_ascii_lowercase());
            }
        }
        None
    }
    fn integer(&mut self) -> Option<Integer> {
        let negative = self.sign();
        let mut radix = 10;
        let digits = if self.attempt(|p| p.ch('0')).is_some() {
            if self.attempt(|p| p.chars("xX")).is_some() {
                radix = 16;
                self.run(16)?
            } else if let Some(binary) = self.attempt(|p| {
                p.chars("bB")?;
                p.run(2)
            }) {
                radix = 2;
                binary
            } else if self.attempt(|p| p.run(10)).is_some() {
                return self.fail("Decimal numbers can't start with 0");
            } else {
                "0".into()
            }
        } else {
            self.run(10)?
        };
        let (signed, kind) = self
            .attempt(Self::suffix)
            .map(|(s, k)| (s, Some(k)))
            .unwrap_or((None, None));
        Some(Integer {
            negative,
            radix,
            digits,
            signed,
            kind,
        })
    }
    fn number(&mut self, n: &Integer, default: char) -> Option<Tag> {
        let kind = n.kind.unwrap_or(default);
        let bits = match kind {
            'b' => 8,
            's' => 16,
            'i' => 32,
            'l' => 64,
            _ => return self.fail("Expected an integer number"),
        };
        let signed = n.signed.unwrap_or(n.radix == 10);
        if !signed && n.negative {
            return self.fail("Expected a non-negative number");
        }
        let digits = n.digits.replace('_', "");
        let fixed = if n.negative {
            format!("-{digits}")
        } else {
            digits.clone()
        };
        let parsed = u64::from_str_radix(&digits, n.radix).ok().filter(|v| {
            if signed {
                if n.negative {
                    *v <= 1u64 << (bits - 1)
                } else {
                    *v < (1u64 << (bits - 1))
                }
            } else {
                bits == 64 || *v < (1u64 << bits)
            }
        });
        let Some(v) = parsed else {
            let detail = if signed && bits <= 16 {
                format!("Value out of range. Value:\"{fixed}\" Radix:{}", n.radix)
            } else if !signed && bits == 8 {
                format!("out of range: {fixed}")
            } else if !signed && bits == 16 {
                format!(
                    "out of range: {}",
                    u64::from_str_radix(&digits, n.radix).unwrap_or(0)
                )
            } else if !signed && bits >= 32 {
                format!(
                    "String value {fixed} exceeds range of unsigned {}.",
                    if bits == 32 { "int" } else { "long" }
                )
            } else {
                format!(
                    "For input string: \"{fixed}\"{}",
                    if n.radix == 10 {
                        String::new()
                    } else {
                        format!(" under radix {}", n.radix)
                    }
                )
            };
            return self.fail(format!("Failed to parse number: {detail}"));
        };
        let value = if n.negative {
            (v as i64).wrapping_neg()
        } else {
            v as i64
        };
        Some(match kind {
            'b' => Tag::Byte(value as i8),
            's' => Tag::Short(value as i16),
            'l' => Tag::Long(value),
            _ => Tag::Int(value as i32),
        })
    }
    fn exponent(&mut self) -> Option<String> {
        self.chars("eE")?;
        let minus = self.sign();
        let digits = self.run(10)?;
        Some(format!("e{}{digits}", if minus { "-" } else { "" }))
    }
    fn float_suffix(&mut self) -> Option<char> {
        self.attempt(|p| p.chars("fF"))
            .or_else(|| self.chars("dD"))
            .map(|c| c.to_ascii_lowercase())
    }
    fn float(&mut self) -> Option<Tag> {
        let negative = self.sign();
        for branch in 0..4 {
            let result = self.attempt(|p| {
                let (whole, fraction) = match branch {
                    0 => {
                        let w = p.run(10)?;
                        p.ch('.')?;
                        let f = p.attempt(|p| p.run(10));
                        (w, f)
                    }
                    1 => {
                        p.ch('.')?;
                        (String::new(), Some(p.run(10)?))
                    }
                    _ => (p.run(10)?, None),
                };
                let exponent = if branch == 2 {
                    Some(p.exponent()?)
                } else {
                    p.attempt(Self::exponent)
                };
                let suffix = if branch == 3 {
                    Some(p.float_suffix()?)
                } else {
                    p.attempt(Self::float_suffix)
                };
                let mut value = if negative { format!("-{whole}") } else { whole };
                if let Some(fraction) = fraction {
                    value.push('.');
                    value.push_str(&fraction)
                }
                if let Some(exponent) = exponent {
                    value.push_str(&exponent)
                }
                let value = value.replace('_', "");
                if suffix == Some('f') {
                    let v = value.parse::<f32>().ok()?;
                    if !v.is_finite() {
                        return p.fail("Non-finite numbers are not allowed");
                    }
                    Some(Tag::Float(if v == 0.0 { 0.0 } else { v }))
                } else {
                    let v = value.parse::<f64>().ok()?;
                    if !v.is_finite() {
                        return p.fail("Non-finite numbers are not allowed");
                    }
                    Some(Tag::Double(if v == 0.0 { 0.0 } else { v }))
                }
            });
            if result.is_some() {
                return result;
            }
        }
        None
    }
    fn unquoted(&mut self) -> Option<NbtString> {
        self.ws();
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| matches!(c,48..=57|65..=90|97..=122|95|45|46|43))
        {
            self.pos += 1;
        }
        if self.pos == start {
            self.fail("Expected a valid unquoted string")
        } else {
            Some(NbtString::from_utf16(self.text[start..self.pos].to_vec()))
        }
    }
    fn quoted(&mut self) -> Option<NbtString> {
        let quote = self
            .attempt(|p| p.ch('"').map(|_| '"'))
            .or_else(|| self.ch('\'').map(|_| '\''))? as u16;
        let mut out = Vec::new();
        loop {
            let start = self.pos;
            while self.peek().is_some_and(|c| c != 34 && c != 39 && c != 92) {
                out.push(self.text[self.pos]);
                self.pos += 1;
            }
            if self.peek() == Some(quote) {
                self.pos += 1;
                return Some(NbtString::from_utf16(out));
            }
            if self.peek().is_none() {
                self.expect(self.pos, ["\"", "'", "\\"].map(String::from));
                return self.fail("Invalid string contents");
            }
            if self.peek() == Some(92) {
                self.pos += 1;
                self.ws();
                let Some(escape) = self.peek() else {
                    self.expect(
                        self.pos,
                        [
                            "b", "s", "t", "n", "f", "r", "\\", "'", "\"", "x", "u", "U", "N",
                        ]
                        .map(String::from),
                    );
                    return self.fail("Expected literal b");
                };
                self.pos += 1;
                match escape {
                    98 => out.push(8),
                    115 => out.push(32),
                    116 => out.push(9),
                    110 => out.push(10),
                    102 => out.push(12),
                    114 => out.push(13),
                    92 | 39 | 34 => out.push(escape),
                    120 | 117 | 85 => {
                        let size = match escape {
                            120 => 2,
                            117 => 4,
                            _ => 8,
                        };
                        let start = self.pos;
                        let mut code = 0u32;
                        for _ in 0..size {
                            let c = self
                                .peek()
                                .and_then(|c| char::from_u32(u32::from(c)))
                                .and_then(|c| c.to_digit(16));
                            let Some(c) = c else {
                                return self.record(
                                    start,
                                    format!("Expected a character literal of length {size}"),
                                );
                            };
                            code = (code << 4) | c;
                            self.pos += 1;
                        }
                        if code > 0x10ffff {
                            return self
                                .fail(format!("Invalid Unicode character value: U+{code:08X}"));
                        }
                        if code <= 65535 {
                            out.push(code as u16)
                        } else {
                            let c = code - 0x10000;
                            out.push((0xd800 + (c >> 10)) as u16);
                            out.push((0xdc00 + (c & 1023)) as u16);
                        }
                    }
                    78 => {
                        self.ch('{')?;
                        let start = self.pos;
                        while self
                            .peek()
                            .is_some_and(|c| matches!(c,45|32|48..=57|65..=90|97..=122))
                        {
                            self.pos += 1;
                        }
                        if self.pos == start {
                            return self.fail("Invalid Unicode character name");
                        }
                        let name = String::from_utf16_lossy(&self.text[start..self.pos]);
                        self.ch('}')?;
                        let Some(c) = named_codepoint(&name) else {
                            return self.fail("Invalid Unicode character name");
                        };
                        if c <= 65535 {
                            out.push(c as u16);
                        } else {
                            let c = c - 0x10000;
                            out.push((0xd800 + (c >> 10)) as u16);
                            out.push((0xdc00 + (c & 1023)) as u16);
                        }
                    }
                    _ => {
                        self.expect(
                            self.pos - 1,
                            [
                                "b", "s", "t", "n", "f", "r", "\\", "'", "\"", "x", "u", "U", "N",
                            ]
                            .map(String::from),
                        );
                        return self.record(self.pos - 1, "Expected literal b");
                    }
                }
            } else {
                out.push(self.text[self.pos]);
                self.pos += 1;
            }
            if self.pos == start {
                return None;
            }
        }
    }
    fn key(&mut self) -> Option<NbtString> {
        self.attempt(Self::quoted).or_else(|| self.unquoted())
    }
    fn map_entry(&mut self) -> Option<(NbtString, Tag)> {
        let key = self.key()?;
        self.ch(':')?;
        let value = self.literal()?;
        if key.is_empty() {
            return self.fail("Key cannot be empty");
        }
        Some((key, value))
    }
    fn compound(&mut self) -> Option<Tag> {
        self.ch('{')?;
        let mut c = CompoundTag::new();
        if let Some((k, v)) = self.attempt(Self::map_entry) {
            c.insert(k, v);
            while self.optional_char(',') {
                if let Some((k, v)) = self.attempt(Self::map_entry) {
                    c.insert(k, v);
                } else {
                    break;
                }
            }
        }
        self.ch('}')?;
        Some(Tag::Compound(c))
    }
    fn array(&mut self) -> Option<Tag> {
        let kind = self
            .attempt(|p| p.ch('B').map(|_| 'b'))
            .or_else(|| self.attempt(|p| p.ch('L').map(|_| 'l')))
            .or_else(|| self.ch('I').map(|_| 'i'))?;
        self.ch(';')?;
        let mut entries = Vec::new();
        if let Some(n) = self.attempt(Self::integer) {
            entries.push(n);
            while self.optional_char(',') {
                if let Some(n) = self.attempt(Self::integer) {
                    entries.push(n)
                } else {
                    break;
                }
            }
        }
        self.ch(']')?;
        let mut tags = Vec::new();
        for n in entries {
            if n.kind.is_some_and(|k| match kind {
                'b' => k != 'b',
                'i' => !matches!(k, 'b' | 's' | 'i'),
                _ => !matches!(k, 'b' | 's' | 'i' | 'l'),
            }) {
                return self.fail("Invalid array element type");
            }
            tags.push(self.number(&n, kind)?);
        }
        Some(match kind {
            'b' => Tag::ByteArray(tags.iter().map(|t| t.as_byte().unwrap()).collect()),
            'i' => Tag::IntArray(tags.iter().map(|t| t.as_int().unwrap()).collect()),
            _ => Tag::LongArray(tags.iter().map(|t| t.as_long().unwrap()).collect()),
        })
    }
    fn list(&mut self) -> Option<Tag> {
        self.ch('[')?;
        if let Some(array) = self.attempt(Self::array) {
            return Some(array);
        }
        let mut v = ListTag::default();
        if let Some(tag) = self.attempt(Self::literal) {
            v.push(tag);
            while self.optional_char(',') {
                if let Some(tag) = self.attempt(Self::literal) {
                    v.push(tag)
                } else {
                    break;
                }
            }
        }
        self.ch(']')?;
        Some(Tag::List(v))
    }
    fn builtin(&mut self) -> Option<Tag> {
        let name = self.unquoted()?.to_string_lossy();
        let mut args = None;
        if let Some(values) = self.attempt(|p| {
            p.ch('(')?;
            let mut values = Vec::new();
            if let Some(v) = p.attempt(Self::literal) {
                values.push(v);
                while p.optional_char(',') {
                    if let Some(v) = p.attempt(Self::literal) {
                        values.push(v)
                    } else {
                        break;
                    }
                }
            }
            p.ch(')')?;
            Some(values)
        }) {
            args = Some(values)
        }
        if name.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.')) {
            self.expect(
                self.pos,
                ["true", "false", "bool", "uuid"].map(String::from),
            );
            return self.fail("Unquoted strings can't start with digits 0-9, + or -");
        }
        if let Some(values) = args {
            if values.len() == 1 && name == "bool" {
                return if let Some(value) = values[0].as_double() {
                    Some(Tag::Byte(i8::from(value != 0.0)))
                } else {
                    self.fail("Expected a number or a boolean")
                };
            }
            if values.len() == 1 && name == "uuid" {
                let value = values[0]
                    .as_string()
                    .and_then(|s| parse_uuid(&s.to_string_lossy()));
                return if let Some(v) = value {
                    Some(Tag::IntArray(v))
                } else {
                    self.fail("Expected a string representing a valid UUID")
                };
            }
            return self.fail(format!("No such operation: {name}/{}", values.len()));
        }
        Some(if name.eq_ignore_ascii_case("true") {
            Tag::Byte(1)
        } else if name.eq_ignore_ascii_case("false") {
            Tag::Byte(0)
        } else {
            Tag::String(name.into())
        })
    }
    fn literal(&mut self) -> Option<Tag> {
        self.ws();
        self.literal_inner()
    }
    fn literal_inner(&mut self) -> Option<Tag> {
        if self
            .peek()
            .is_some_and(|c| matches!(c, 43 | 45 | 46 | 48..=57))
        {
            if let Some(value) = self.attempt(Self::float) {
                return Some(value);
            }
            if let Some(value) = self.attempt(|p| {
                let n = p.integer()?;
                p.number(&n, 'i')
            }) {
                return Some(value);
            }
        }
        match self.peek() {
            Some(34 | 39) => self.quoted().map(Tag::String),
            Some(123) => self.compound(),
            Some(91) => self.list(),
            _ => self.builtin(),
        }
    }
}
fn java_whitespace(c: u16) -> bool {
    matches!(c,9..=13|28..=32|0x1680|0x2000..=0x2006|0x2008..=0x200a|0x2028|0x2029|0x205f|0x3000)
}
fn parse_uuid(s: &str) -> Option<Vec<i32>> {
    if s.len() > 36 {
        return None;
    }
    let parts: Vec<_> = s.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let mut numbers = Vec::new();
    for p in parts {
        let p = p.strip_prefix('+').unwrap_or(p);
        numbers.push(i64::from_str_radix(p, 16).ok()? as u64)
    }
    let high =
        ((numbers[0] & 0xffffffff) << 32) | ((numbers[1] & 0xffff) << 16) | (numbers[2] & 0xffff);
    let low = ((numbers[3] & 0xffff) << 48) | (numbers[4] & 0xffffffffffff);
    Some(vec![
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32,
    ])
}

// Character.codePointOf uses Java block names for ranges with algorithmic Unicode names.
fn named_codepoint(name: &str) -> Option<u32> {
    use crate::java_names::{CONTROLS, RANGES};
    let name = name.trim().to_ascii_uppercase();
    if let Some((cp, _)) = CONTROLS.iter().find(|(_, n)| *n == name) {
        return Some(*cp);
    }
    if let Some((block, hex)) = name.rsplit_once(' ')
        && let Ok(cp) = u32::from_str_radix(hex, 16)
        && RANGES
            .iter()
            .any(|(first, last, n)| *first <= cp && cp <= *last && *n == block)
        && name == format!("{block} {cp:X}")
    {
        return Some(cp);
    }
    let c = unicode_names2::character(&name)?;
    let cp = u32::from(c);
    if RANGES
        .iter()
        .any(|(first, last, _)| *first <= cp && cp <= *last)
        || CONTROLS.iter().any(|(control, _)| *control == cp)
    {
        return None;
    }
    (unicode_names2::name(c)?.to_string() == name).then_some(cp)
}
