use std::{error::Error, fmt, sync::Arc};

/// Port of Message. Implementations may retain Minecraft's translatable component.
pub trait Message: fmt::Debug + Send + Sync {
    fn text(&self) -> &str;
}
impl Message for String {
    fn text(&self) -> &str {
        self
    }
}
pub type MessageRef = Arc<dyn Message>;

/// Identifies the built-in exception independently of its rendered message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExceptionKind {
    ExpectedNumber(&'static str),
    InvalidNumber(&'static str),
    TooLow(&'static str),
    TooHigh(&'static str),
    LiteralIncorrect,
    ExpectedStartOfQuote,
    ExpectedEndOfQuote,
    InvalidEscape,
    InvalidBool,
    ExpectedBool,
    ExpectedSymbol,
    UnknownCommand,
    UnknownArgument,
    ExpectedArgumentSeparator,
    ParseException,
    Custom(String),
}

/// Port of BuiltInExceptionProvider, scoped to a dispatcher/reader rather than global state.
pub trait BuiltInExceptionProvider: Send + Sync {
    fn message(&self, kind: &ExceptionKind, arguments: &[String]) -> MessageRef;
}
#[derive(Debug, Default)]
pub struct BuiltInExceptions;
impl BuiltInExceptionProvider for BuiltInExceptions {
    fn message(&self, kind: &ExceptionKind, a: &[String]) -> MessageRef {
        use ExceptionKind::*;
        Arc::new(match kind {
            ExpectedNumber(t) => format!("Expected {t}"),
            InvalidNumber(t) => format!("Invalid {t} '{}'", a[0]),
            TooLow(t) => format!("{t} must not be less than {}, found {}", a[1], a[0]),
            TooHigh(t) => format!("{t} must not be more than {}, found {}", a[1], a[0]),
            LiteralIncorrect => format!("Expected literal {}", a[0]),
            ExpectedStartOfQuote => "Expected quote to start a string".into(),
            ExpectedEndOfQuote => "Unclosed quoted string".into(),
            InvalidEscape => format!("Invalid escape sequence '{}' in quoted string", a[0]),
            InvalidBool => format!("Invalid bool, expected true or false but found '{}'", a[0]),
            ExpectedBool => "Expected bool".into(),
            ExpectedSymbol => format!("Expected '{}'", a[0]),
            UnknownCommand => "Unknown command".into(),
            UnknownArgument => "Incorrect argument for command".into(),
            ExpectedArgumentSeparator => {
                "Expected whitespace to end one argument, but found trailing data".into()
            }
            ParseException => format!("Could not parse command: {}", a[0]),
            Custom(message) => message.clone(),
        })
    }
}

/// Port of CommandSyntaxException, including its ten-character context window.
#[derive(Clone, Debug)]
pub struct CommandSyntaxException {
    pub kind: ExceptionKind,
    pub raw_message: MessageRef,
    pub input: Option<String>,
    pub cursor: Option<usize>,
}
impl CommandSyntaxException {
    pub fn new(kind: ExceptionKind, message: MessageRef) -> Self {
        Self {
            kind,
            raw_message: message,
            input: None,
            cursor: None,
        }
    }
    pub fn with_context(mut self, input: impl Into<String>, cursor: usize) -> Self {
        self.input = Some(input.into());
        self.cursor = Some(cursor);
        self
    }
    pub fn context(&self) -> Option<String> {
        let input: Vec<u16> = self.input.as_ref()?.encode_utf16().collect();
        let cursor = self.cursor?.min(input.len());
        Some(format!(
            "{}{}<--[HERE]",
            if cursor > 10 { "..." } else { "" },
            String::from_utf16_lossy(&input[cursor.saturating_sub(10)..cursor])
        ))
    }
}
impl fmt::Display for CommandSyntaxException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.raw_message.text())?;
        if let (Some(context), Some(cursor)) = (self.context(), self.cursor) {
            write!(f, " at position {cursor}: {context}")?;
        }
        Ok(())
    }
}
impl Error for CommandSyntaxException {}

/// Port of SimpleCommandExceptionType and the dynamic exception factories.
#[derive(Clone)]
pub struct CommandExceptionType {
    pub kind: ExceptionKind,
    message: Arc<dyn Fn(&[String]) -> MessageRef + Send + Sync>,
}
impl CommandExceptionType {
    pub fn new(
        kind: ExceptionKind,
        message: impl Fn(&[String]) -> MessageRef + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            message: Arc::new(message),
        }
    }
    pub fn simple(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::new(ExceptionKind::Custom(message.clone()), move |_| {
            Arc::new(message.clone())
        })
    }
    pub fn create(&self, arguments: &[String]) -> CommandSyntaxException {
        CommandSyntaxException::new(self.kind.clone(), (self.message)(arguments))
    }
    pub fn create_with_context(
        &self,
        reader: &crate::StringReader,
        arguments: &[String],
    ) -> CommandSyntaxException {
        self.create(arguments)
            .with_context(reader.string(), reader.cursor())
    }
}
