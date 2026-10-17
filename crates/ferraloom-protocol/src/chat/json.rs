//! Gson JsonParser's legacy lenient token forms, used by login disconnects.
use crate::{Error, Result};
use serde_json::{Map, Value};

pub(crate) fn parse(input: &str) -> Result<Value> {
    let mut parser = Parser {
        input: input.trim_start_matches('\u{feff}'),
        depth: 0,
    };
    parser.space()?;
    if parser.input.starts_with(")]}'\n") {
        parser.input = &parser.input[5..];
    }
    let result = if parser.input.is_empty() {
        Value::Null
    } else {
        parser.value()?
    };
    parser.space()?;
    if !parser.input.is_empty() {
        return Err(parser.error("Trailing JSON data"));
    }
    Ok(result)
}
struct Parser<'a> {
    input: &'a str,
    depth: usize,
}
impl Parser<'_> {
    fn error(&self, text: &str) -> Error {
        Error::invalid("JSON", text)
    }
    fn take(&mut self) -> Result<char> {
        let c = self
            .input
            .chars()
            .next()
            .ok_or_else(|| self.error("Unexpected end"))?;
        self.input = &self.input[c.len_utf8()..];
        Ok(c)
    }
    fn space(&mut self) -> Result<()> {
        loop {
            self.input = self.input.trim_start_matches([' ', '\t', '\r', '\n']);
            if let Some(rest) = self
                .input
                .strip_prefix("//")
                .or_else(|| self.input.strip_prefix('#'))
            {
                self.input = rest.split_once('\n').map_or("", |p| p.1);
            } else if let Some(rest) = self.input.strip_prefix("/*") {
                self.input = rest
                    .split_once("*/")
                    .ok_or_else(|| self.error("Unterminated comment"))?
                    .1;
            } else {
                return Ok(());
            }
        }
    }
    fn quoted(&mut self, quote: char) -> Result<String> {
        let mut result = String::new();
        loop {
            match self.take()? {
                c if c == quote => return Ok(result),
                '\\' => match self.take()? {
                    'b' => result.push('\u{8}'),
                    'f' => result.push('\u{c}'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    '\n' => result.push('\n'),
                    c @ ('\'' | '"' | '/' | '\\') => result.push(c),
                    'u' => {
                        let mut units = vec![self.hex_unit()?];
                        if (0xd800..=0xdbff).contains(&units[0]) && self.input.starts_with("\\u") {
                            self.input = &self.input[2..];
                            units.push(self.hex_unit()?);
                        }
                        result.push_str(&String::from_utf16_lossy(&units));
                    }
                    _ => return Err(self.error("Invalid escape")),
                },
                c => result.push(c),
            }
        }
    }
    fn hex_unit(&mut self) -> Result<u16> {
        if self.input.len() < 4 || !self.input.as_bytes()[..4].iter().all(u8::is_ascii_hexdigit) {
            return Err(self.error("Invalid Unicode escape"));
        }
        let value = u16::from_str_radix(&self.input[..4], 16)
            .map_err(|_| self.error("Invalid Unicode escape"))?;
        self.input = &self.input[4..];
        Ok(value)
    }
    fn token(&mut self) -> Result<String> {
        let end = self
            .input
            .find(|c: char| c.is_ascii_whitespace() || "{}[]:,;=#/\\".contains(c))
            .unwrap_or(self.input.len());
        if end == 0 {
            return Err(self.error("Expected value"));
        }
        let text = self.input[..end].to_owned();
        self.input = &self.input[end..];
        Ok(text)
    }
    fn value(&mut self) -> Result<Value> {
        self.space()?;
        self.depth += 1;
        if self.depth > 512 {
            return Err(self.error("Nesting exceeds 512"));
        }
        let value = match self
            .input
            .chars()
            .next()
            .ok_or_else(|| self.error("Expected value"))?
        {
            '{' => {
                self.take()?;
                self.object()?
            }
            '[' => {
                self.take()?;
                self.array()?
            }
            c @ ('\'' | '"') => {
                self.take()?;
                Value::String(self.quoted(c)?)
            }
            _ => {
                let text = self.token()?;
                if text.eq_ignore_ascii_case("true") {
                    Value::Bool(true)
                } else if text.eq_ignore_ascii_case("false") {
                    Value::Bool(false)
                } else if text.eq_ignore_ascii_case("null") {
                    Value::Null
                } else {
                    serde_json::from_str::<Value>(&text)
                        .ok()
                        .filter(Value::is_number)
                        .unwrap_or(Value::String(text))
                }
            }
        };
        self.depth -= 1;
        Ok(value)
    }
    fn object(&mut self) -> Result<Value> {
        let mut map = Map::new();
        self.space()?;
        if self.input.starts_with('}') {
            self.take()?;
            return Ok(Value::Object(map));
        }
        loop {
            self.space()?;
            let name = match self.input.chars().next() {
                Some(c @ ('\'' | '"')) => {
                    self.take()?;
                    self.quoted(c)?
                }
                _ => self.token()?,
            };
            self.space()?;
            match self.take()? {
                ':' => {}
                '=' => {
                    if self.input.starts_with('>') {
                        self.take()?;
                    }
                }
                _ => return Err(self.error("Expected colon")),
            }
            map.insert(name, self.value()?);
            self.space()?;
            match self.take()? {
                '}' => return Ok(Value::Object(map)),
                ',' | ';' => {}
                _ => return Err(self.error("Expected object separator")),
            }
        }
    }
    fn array(&mut self) -> Result<Value> {
        let mut values = Vec::new();
        self.space()?;
        if self.input.starts_with(']') {
            self.take()?;
            return Ok(Value::Array(values));
        }
        loop {
            self.space()?;
            if self.input.starts_with([',', ';', ']']) {
                values.push(Value::Null);
            } else {
                values.push(self.value()?);
            }
            self.space()?;
            match self.take()? {
                ']' => return Ok(Value::Array(values)),
                ',' | ';' => {}
                _ => return Err(self.error("Expected array separator")),
            }
        }
    }
}
