use crate::commands::{CommandSourceStack, Commands, PreparedCommand};
use ferraloom_brigadier::StringReader;
use ferraloom_data::datapack::Identifier;
use std::{fmt, sync::Arc};

/// Port of PlainTextFunction. Entries retain parsed arguments, never selector matches.
#[derive(Clone, Debug)]
pub struct PlainTextFunction {
    pub id: Identifier,
    pub entries: Arc<[PreparedCommand]>,
}
#[derive(Clone, Debug)]
pub enum CompiledFunction {
    Plain(Arc<PlainTextFunction>),
    /// Slice two supplies MacroFunction. Keep the known definition distinct
    /// from a missing function; accessing it requires the missing implementation.
    MacroPending(Identifier),
}
impl CompiledFunction {
    pub fn id(&self) -> &Identifier {
        match self {
            Self::Plain(function) => &function.id,
            Self::MacroPending(id) => id,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionParseError {
    pub message: String,
}
impl fmt::Display for FunctionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}
impl std::error::Error for FunctionParseError {}
fn failure(message: impl Into<String>) -> FunctionParseError {
    FunctionParseError {
        message: message.into(),
    }
}

/// Port of BufferedReader#lines, including bare CR and the absent trailing empty line.
pub fn read_lines(bytes: &[u8]) -> Vec<String> {
    let text = ferraloom_data::datapack::read_utf8(bytes);
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            lines.push(text[start..index].to_owned());
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            index += 1;
            start = index;
        } else {
            index += 1;
        }
    }
    if start < bytes.len() {
        lines.push(text[start..].to_owned());
    }
    lines
}
fn check_length(line: &str) -> Result<(), FunctionParseError> {
    let length = line.encode_utf16().count();
    if length > 2_000_000 {
        let truncated = StringReader::new(line).slice(0, 512);
        Err(failure(format!(
            "Command too long: {length} characters, contents: {truncated}..."
        )))
    } else {
        Ok(())
    }
}
impl CompiledFunction {
    /// Port of CommandFunction#fromLines. Java trim removes only characters <= U+0020.
    pub fn from_lines(
        id: Identifier,
        commands: &Commands,
        source: CommandSourceStack,
        lines: &[String],
    ) -> Result<Self, FunctionParseError> {
        let mut entries = Vec::new();
        let mut macro_pending = false;
        let mut index = 0;
        while index < lines.len() {
            let number = index + 1;
            let mut line = lines[index]
                .trim_matches(|ch: char| ch <= '\u{20}')
                .to_owned();
            while line.ends_with('\\') {
                index += 1;
                if index == lines.len() {
                    return Err(failure("Line continuation at end of file"));
                }
                line.pop();
                line.push_str(lines[index].trim_matches(|ch: char| ch <= '\u{20}'));
                check_length(&line)?;
            }
            index += 1;
            check_length(&line)?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('/') {
                if line.len() == 1 {
                    return Err(failure("Index 1 out of bounds for length 1"));
                }
                if line.starts_with("//") {
                    return Err(failure(format!(
                        "Unknown or invalid command '{line}' on line {number} (if you intended to make a comment, use '#' not '//')"
                    )));
                }
                let mut reader = StringReader::new(&line);
                reader.skip();
                let name = reader.read_unquoted_string();
                return Err(failure(format!(
                    "Unknown or invalid command '{line}' on line {number} (did you mean '{name}'? Do not use a preceding forwards slash.)"
                )));
            }
            if line.starts_with('$') {
                macro_pending = true;
            } else {
                entries.push(commands.prepare(&line, source.clone()).map_err(|error| {
                    failure(format!("Whilst parsing command on line {number}: {error}"))
                })?);
            }
        }
        Ok(if macro_pending {
            Self::MacroPending(id)
        } else {
            Self::Plain(Arc::new(PlainTextFunction {
                id,
                entries: entries.into(),
            }))
        })
    }
}
