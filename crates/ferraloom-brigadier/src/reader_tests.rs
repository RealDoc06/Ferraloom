// Tests ported from Mojang Brigadier 1.3.11 StringReaderTest (MIT).
use super::*;
#[test]
fn can_read() {
    let mut reader = StringReader::new("abc");
    assert!(reader.can_read());
    reader.skip();
    assert!(reader.can_read());
    reader.skip();
    assert!(reader.can_read());
    reader.skip();
    assert!(!reader.can_read());
}
#[test]
fn get_remaining_length() {
    let mut reader = StringReader::new("abc");
    assert_eq!(reader.remaining_length(), 3);
    reader.set_cursor(1);
    assert_eq!(reader.remaining_length(), 2);
    reader.set_cursor(2);
    assert_eq!(reader.remaining_length(), 1);
    reader.set_cursor(3);
    assert_eq!(reader.remaining_length(), 0);
}
#[test]
fn can_read_length() {
    let reader = StringReader::new("abc");
    assert!(reader.can_read_length(1));
    assert!(reader.can_read_length(2));
    assert!(reader.can_read_length(3));
    assert!(!reader.can_read_length(4));
    assert!(!reader.can_read_length(5));
}
#[test]
fn peek() {
    let mut reader = StringReader::new("abc");
    assert_eq!(reader.peek(), 'a' as u16);
    assert_eq!(reader.cursor(), 0);
    reader.set_cursor(2);
    assert_eq!(reader.peek(), 'c' as u16);
    assert_eq!(reader.cursor(), 2);
}
#[test]
fn peek_length() {
    let mut reader = StringReader::new("abc");
    assert_eq!(reader.peek_offset(0), 'a' as u16);
    assert_eq!(reader.peek_offset(2), 'c' as u16);
    assert_eq!(reader.cursor(), 0);
    reader.set_cursor(1);
    assert_eq!(reader.peek_offset(1), 'c' as u16);
    assert_eq!(reader.cursor(), 1);
}
#[test]
fn read() {
    let mut reader = StringReader::new("abc");
    assert_eq!(reader.read(), 'a' as u16);
    assert_eq!(reader.read(), 'b' as u16);
    assert_eq!(reader.read(), 'c' as u16);
    assert_eq!(reader.cursor(), 3);
}
#[test]
fn skip() {
    let mut reader = StringReader::new("abc");
    reader.skip();
    assert_eq!(reader.cursor(), 1);
}
#[test]
fn get_remaining() {
    let mut reader = StringReader::new("Hello!");
    assert_eq!(reader.remaining(), "Hello!");
    reader.set_cursor(3);
    assert_eq!(reader.remaining(), "lo!");
    reader.set_cursor(6);
    assert_eq!(reader.remaining(), "");
}
#[test]
fn get_read() {
    let mut reader = StringReader::new("Hello!");
    assert_eq!(reader.read_so_far(), "");
    reader.set_cursor(3);
    assert_eq!(reader.read_so_far(), "Hel");
    reader.set_cursor(6);
    assert_eq!(reader.read_so_far(), "Hello!");
}
#[test]
fn skip_whitespace_none() {
    let mut reader = StringReader::new("Hello!");
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 0);
}
#[test]
fn skip_whitespace_mixed() {
    let mut reader = StringReader::new(" \t \t\nHello!");
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 5);
}
#[test]
fn skip_whitespace_empty() {
    let mut reader = StringReader::new("");
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 0);
}
#[test]
fn read_unquoted_string() {
    let mut reader = StringReader::new("hello world");
    assert_eq!(reader.read_unquoted_string(), "hello");
    assert_eq!(reader.read_so_far(), "hello");
    assert_eq!(reader.remaining(), " world");
}
#[test]
fn read_unquoted_string_empty() {
    let mut reader = StringReader::new("");
    assert_eq!(reader.read_unquoted_string(), "");
    assert_eq!(reader.read_so_far(), "");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_unquoted_string_empty_with_remaining() {
    let mut reader = StringReader::new(" hello world");
    assert_eq!(reader.read_unquoted_string(), "");
    assert_eq!(reader.read_so_far(), "");
    assert_eq!(reader.remaining(), " hello world");
}
#[test]
fn read_quoted_string() {
    let mut reader = StringReader::new("\"hello world\"");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "\"hello world\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_single_quoted_string() {
    let mut reader = StringReader::new("'hello world'");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "'hello world'");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_mixed_quoted_string_double_inside_single() {
    let mut reader = StringReader::new("'hello \"world\"'");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
    assert_eq!(reader.read_so_far(), "'hello \"world\"'");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_mixed_quoted_string_single_inside_double() {
    let mut reader = StringReader::new("\"hello 'world'\"");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello 'world'");
    assert_eq!(reader.read_so_far(), "\"hello 'world'\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_quoted_string_empty() {
    let mut reader = StringReader::new("");
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.read_so_far(), "");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_quoted_string_empty_quoted() {
    let mut reader = StringReader::new("\"\"");
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.read_so_far(), "\"\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_quoted_string_empty_quoted_with_remaining() {
    let mut reader = StringReader::new("\"\" hello world");
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.read_so_far(), "\"\"");
    assert_eq!(reader.remaining(), " hello world");
}
#[test]
fn read_quoted_string_with_escaped_quote() {
    let mut reader = StringReader::new("\"hello \\\"world\\\"\"");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
    assert_eq!(reader.read_so_far(), "\"hello \\\"world\\\"\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_quoted_string_with_escaped_escapes() {
    let mut reader = StringReader::new("\"\\\\o/\"");
    assert_eq!(reader.read_quoted_string().unwrap(), "\\o/");
    assert_eq!(reader.read_so_far(), "\"\\\\o/\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_quoted_string_with_remaining() {
    let mut reader = StringReader::new("\"hello world\" foo bar");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "\"hello world\"");
    assert_eq!(reader.remaining(), " foo bar");
}
#[test]
fn read_quoted_string_with_immediate_remaining() {
    let mut reader = StringReader::new("\"hello world\"foo bar");
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "\"hello world\"");
    assert_eq!(reader.remaining(), "foo bar");
}
#[test]
fn read_quoted_string_no_open() {
    let ex = StringReader::new("hello world\"")
        .read_quoted_string()
        .unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedStartOfQuote);
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_quoted_string_no_close() {
    let ex = StringReader::new("\"hello world")
        .read_quoted_string()
        .unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedEndOfQuote);
    assert_eq!(ex.cursor.unwrap(), 12);
}
#[test]
fn read_quoted_string_invalid_escape() {
    let ex = StringReader::new("\"hello\\nworld\"")
        .read_quoted_string()
        .unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidEscape);
    assert_eq!(ex.cursor.unwrap(), 7);
}
#[test]
fn read_quoted_string_invalid_quote_escape() {
    let ex = StringReader::new("'hello\\\"'world")
        .read_quoted_string()
        .unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidEscape);
    assert_eq!(ex.cursor.unwrap(), 7);
}
#[test]
fn read_string_no_quotes() {
    let mut reader = StringReader::new("hello world");
    assert_eq!(reader.read_string().unwrap(), "hello");
    assert_eq!(reader.read_so_far(), "hello");
    assert_eq!(reader.remaining(), " world");
}
#[test]
fn read_string_single_quotes() {
    let mut reader = StringReader::new("'hello world'");
    assert_eq!(reader.read_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "'hello world'");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_string_double_quotes() {
    let mut reader = StringReader::new("\"hello world\"");
    assert_eq!(reader.read_string().unwrap(), "hello world");
    assert_eq!(reader.read_so_far(), "\"hello world\"");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_int() {
    let mut reader = StringReader::new("1234567890");
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_int_negative() {
    let mut reader = StringReader::new("-1234567890");
    assert_eq!(reader.read_int().unwrap(), -1234567890);
    assert_eq!(reader.read_so_far(), "-1234567890");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_int_invalid() {
    let ex = StringReader::new("12.34").read_int().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidNumber("integer"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_int_none() {
    let ex = StringReader::new("").read_int().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedNumber("integer"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_int_with_remaining() {
    let mut reader = StringReader::new("1234567890 foo bar");
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), " foo bar");
}
#[test]
fn read_int_with_remaining_immediate() {
    let mut reader = StringReader::new("1234567890foo bar");
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), "foo bar");
}
#[test]
fn read_long() {
    let mut reader = StringReader::new("1234567890");
    assert_eq!(reader.read_long().unwrap(), 1234567890i64);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_long_negative() {
    let mut reader = StringReader::new("-1234567890");
    assert_eq!(reader.read_long().unwrap(), -1234567890i64);
    assert_eq!(reader.read_so_far(), "-1234567890");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_long_invalid() {
    let ex = StringReader::new("12.34").read_long().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidNumber("long"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_long_none() {
    let ex = StringReader::new("").read_long().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedNumber("long"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_long_with_remaining() {
    let mut reader = StringReader::new("1234567890 foo bar");
    assert_eq!(reader.read_long().unwrap(), 1234567890i64);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), " foo bar");
}
#[test]
fn read_long_with_remaining_immediate() {
    let mut reader = StringReader::new("1234567890foo bar");
    assert_eq!(reader.read_long().unwrap(), 1234567890i64);
    assert_eq!(reader.read_so_far(), "1234567890");
    assert_eq!(reader.remaining(), "foo bar");
}
#[test]
fn read_double() {
    let mut reader = StringReader::new("123");
    assert_eq!(reader.read_double().unwrap(), 123.0);
    assert_eq!(reader.read_so_far(), "123");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_double_with_decimal() {
    let mut reader = StringReader::new("12.34");
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_double_negative() {
    let mut reader = StringReader::new("-123");
    assert_eq!(reader.read_double().unwrap(), -123.0);
    assert_eq!(reader.read_so_far(), "-123");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_double_invalid() {
    let ex = StringReader::new("12.34.56").read_double().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidNumber("double"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_double_none() {
    let ex = StringReader::new("").read_double().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedNumber("double"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_double_with_remaining() {
    let mut reader = StringReader::new("12.34 foo bar");
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), " foo bar");
}
#[test]
fn read_double_with_remaining_immediate() {
    let mut reader = StringReader::new("12.34foo bar");
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), "foo bar");
}
#[test]
fn read_float() {
    let mut reader = StringReader::new("123");
    assert_eq!(reader.read_float().unwrap(), 123.0f32);
    assert_eq!(reader.read_so_far(), "123");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_float_with_decimal() {
    let mut reader = StringReader::new("12.34");
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_float_negative() {
    let mut reader = StringReader::new("-123");
    assert_eq!(reader.read_float().unwrap(), -123.0f32);
    assert_eq!(reader.read_so_far(), "-123");
    assert_eq!(reader.remaining(), "");
}
#[test]
fn read_float_invalid() {
    let ex = StringReader::new("12.34.56").read_float().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidNumber("float"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_float_none() {
    let ex = StringReader::new("").read_float().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedNumber("float"));
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_float_with_remaining() {
    let mut reader = StringReader::new("12.34 foo bar");
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), " foo bar");
}
#[test]
fn read_float_with_remaining_immediate() {
    let mut reader = StringReader::new("12.34foo bar");
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.read_so_far(), "12.34");
    assert_eq!(reader.remaining(), "foo bar");
}
#[test]
fn expect_correct() {
    let mut reader = StringReader::new("abc");
    reader.expect('a' as u16).unwrap();
    assert_eq!(reader.cursor(), 1);
}
#[test]
fn expect_incorrect() {
    let mut reader = StringReader::new("bcd");

    let ex = reader.expect('a' as u16).unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedSymbol);
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn expect_none() {
    let mut reader = StringReader::new("");

    let ex = reader.expect('a' as u16).unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedSymbol);
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_boolean_correct() {
    let mut reader = StringReader::new("true");
    assert!(reader.read_boolean().unwrap());
    assert_eq!(reader.read_so_far(), "true");
}
#[test]
fn read_boolean_incorrect() {
    let mut reader = StringReader::new("tuesday");

    let ex = reader.read_boolean().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::InvalidBool);
    assert_eq!(ex.cursor.unwrap(), 0);
}
#[test]
fn read_boolean_none() {
    let mut reader = StringReader::new("");

    let ex = reader.read_boolean().unwrap_err();

    assert_eq!(ex.kind, ExceptionKind::ExpectedBool);
    assert_eq!(ex.cursor.unwrap(), 0);
}
