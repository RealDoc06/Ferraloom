use std::{str::FromStr, sync::Arc};
use crate::{BuiltInExceptionProvider, BuiltInExceptions, CommandSyntaxException, ExceptionKind};

/// Port of StringReader. Positions count Java UTF-16 code units, including surrogate pairs.
#[derive(Clone)]
pub struct StringReader {
    string: String,
    units: Arc<[u16]>,
    cursor: usize,
    provider: Arc<dyn BuiltInExceptionProvider>,
}
impl StringReader {
    pub fn new(input: impl Into<String>) -> Self {
        Self::with_provider(input, Arc::new(BuiltInExceptions))
    }
    pub fn with_provider(input: impl Into<String>, provider: Arc<dyn BuiltInExceptionProvider>) -> Self {
        let string = input.into();
        Self { units: string.encode_utf16().collect(), string, cursor: 0, provider }
    }
    pub fn string(&self) -> &str { &self.string }
    pub fn cursor(&self) -> usize { self.cursor }
    pub fn set_cursor(&mut self, cursor: usize) { self.cursor = cursor; }
    pub fn total_length(&self) -> usize { self.units.len() }
    pub fn remaining_length(&self) -> usize { self.units.len() - self.cursor }
    pub fn can_read(&self) -> bool { self.can_read_length(1) }
    pub fn can_read_length(&self, length: usize) -> bool { length <= self.remaining_length() }
    pub fn peek(&self) -> u16 { self.peek_offset(0) }
    pub fn peek_offset(&self, offset: usize) -> u16 { self.units[self.cursor + offset] }
    pub fn read(&mut self) -> u16 { let value = self.peek(); self.skip(); value }
    pub fn skip(&mut self) { self.cursor += 1; }
    pub fn read_so_far(&self) -> String { self.slice(0, self.cursor) }
    pub fn remaining(&self) -> String { self.slice(self.cursor, self.total_length()) }
    pub fn slice(&self, start: usize, end: usize) -> String { String::from_utf16_lossy(&self.units[start..end]) }
    pub fn is_allowed_number(c: u16) -> bool { matches!(c, 48..=57 | 46 | 45) }
    pub fn is_quoted_string_start(c: u16) -> bool { c == 34 || c == 39 }
    pub fn is_allowed_in_unquoted_string(c: u16) -> bool {
        matches!(c, 48..=57 | 65..=90 | 97..=122 | 95 | 45 | 46 | 43)
    }
    pub fn skip_whitespace(&mut self) {
        // Character.isWhitespace differs from Unicode White_Space at NBSP and NEL.
        while self.can_read() && matches!(self.peek(), 9..=13 | 28..=32 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200a | 0x2028 | 0x2029 | 0x205f | 0x3000) { self.skip(); }
    }
    pub fn error(&self, kind: ExceptionKind, arguments: &[String]) -> CommandSyntaxException {
        let message = self.provider.message(&kind, arguments);
        CommandSyntaxException::new(kind, message).with_context(&self.string, self.cursor)
    }
    fn number<T: FromStr>(&mut self, name: &'static str) -> Result<T, CommandSyntaxException> {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_number(self.peek()) { self.skip(); }
        let value = self.slice(start, self.cursor);
        if value.is_empty() { return Err(self.error(ExceptionKind::ExpectedNumber(name), &[])); }
        value.parse().map_err(|_| {
            self.cursor = start;
            self.error(ExceptionKind::InvalidNumber(name), &[value])
        })
    }
    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxException> { self.number("integer") }
    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxException> { self.number("long") }
    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxException> { self.number("float") }
    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxException> { self.number("double") }
    pub fn read_unquoted_string(&mut self) -> String {
        let start = self.cursor;
        while self.can_read() && Self::is_allowed_in_unquoted_string(self.peek()) { self.skip(); }
        self.slice(start, self.cursor)
    }
    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxException> {
        if !self.can_read() { return Ok(String::new()); }
        let next = self.peek();
        if !Self::is_quoted_string_start(next) { return Err(self.error(ExceptionKind::ExpectedStartOfQuote, &[])); }
        self.skip(); self.read_string_until(next)
    }
    pub fn read_string_until(&mut self, terminator: u16) -> Result<String, CommandSyntaxException> {
        let mut result = Vec::new();
        let mut escaped = false;
        while self.can_read() {
            let c = self.read();
            if escaped {
                if c == terminator || c == 92 { result.push(c); escaped = false; }
                else { self.cursor -= 1; return Err(self.error(ExceptionKind::InvalidEscape, &[String::from_utf16_lossy(&[c])])); }
            } else if c == 92 { escaped = true; }
            else if c == terminator { return Ok(String::from_utf16_lossy(&result)); }
            else { result.push(c); }
        }
        Err(self.error(ExceptionKind::ExpectedEndOfQuote, &[]))
    }
    pub fn read_string(&mut self) -> Result<String, CommandSyntaxException> {
        if self.can_read() && Self::is_quoted_string_start(self.peek()) {
            let terminator = self.read(); self.read_string_until(terminator)
        } else { Ok(self.read_unquoted_string()) }
    }
    pub fn read_boolean(&mut self) -> Result<bool, CommandSyntaxException> {
        let start = self.cursor;
        let value = self.read_string()?;
        match value.as_str() {
            "" => Err(self.error(ExceptionKind::ExpectedBool, &[])),
            "true" => Ok(true), "false" => Ok(false),
            _ => { self.cursor = start; Err(self.error(ExceptionKind::InvalidBool, &[value])) }
        }
    }
    pub fn expect(&mut self, c: u16) -> Result<(), CommandSyntaxException> {
        if !self.can_read() || self.peek() != c { return Err(self.error(ExceptionKind::ExpectedSymbol, &[String::from_utf16_lossy(&[c])])); }
        self.skip(); Ok(())
    }
}
