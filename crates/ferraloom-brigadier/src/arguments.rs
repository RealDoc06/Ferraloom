use crate::{
    CommandContext, CommandSyntaxException, ExceptionKind, StringReader, Suggestions,
    SuggestionsBuilder,
};
use std::{any::Any, sync::Arc};

/// Port of ArgumentType. Parsed values retain their concrete Rust type in the context.
pub trait ArgumentType: Send + Sync + 'static {
    type Value: Any + Send + Sync;
    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException>;
    fn examples(&self) -> Vec<String> {
        Vec::new()
    }
    fn list_suggestions<S>(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException> {
        Ok(builder.build())
    }
}

pub(crate) trait ErasedArgumentType<S>: Send + Sync {
    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn Any + Send + Sync>, CommandSyntaxException>;
    fn examples(&self) -> Vec<String>;
    fn list_suggestions(
        &self,
        context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException>;
    fn as_any(&self) -> &dyn Any;
}
impl<S, A: ArgumentType> ErasedArgumentType<S> for A {
    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn Any + Send + Sync>, CommandSyntaxException> {
        Ok(Arc::new(ArgumentType::parse(self, reader)?))
    }
    fn examples(&self) -> Vec<String> {
        ArgumentType::examples(self)
    }
    fn list_suggestions(
        &self,
        context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException> {
        ArgumentType::list_suggestions(self, context, builder)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

macro_rules! numeric_argument {
    ($name:ident, $ty:ty, $read:ident, $label:literal, [$($example:literal),*]) => {
        #[doc = concat!("Port of ", stringify!($name), ".")]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name { pub minimum: $ty, pub maximum: $ty }
        impl Default for $name { fn default() -> Self { Self { minimum: <$ty>::MIN, maximum: <$ty>::MAX } } }
        impl $name {
            pub fn new(minimum: $ty, maximum: $ty) -> Self { Self { minimum, maximum } }
            pub fn min(minimum: $ty) -> Self { Self { minimum, ..Self::default() } }
        }
        impl ArgumentType for $name {
            type Value = $ty;
            fn parse(&self, reader: &mut StringReader) -> Result<$ty, CommandSyntaxException> {
                let start = reader.cursor(); let value = reader.$read()?;
                let violation = if value < self.minimum { Some((ExceptionKind::TooLow($label), self.minimum)) }
                    else if value > self.maximum { Some((ExceptionKind::TooHigh($label), self.maximum)) } else { None };
                if let Some((kind, bound)) = violation {
                    reader.set_cursor(start);
                    return Err(reader.error(kind, &[java_number(value), java_number(bound)]));
                }
                Ok(value)
            }
            fn examples(&self) -> Vec<String> { vec![$($example.into()),*] }
        }
    }
}
// Java's floating-point messages include a decimal point for integral values.
fn java_number<T: std::fmt::Display + Any>(value: T) -> String {
    let mut text = value.to_string();
    if (&value as &dyn Any).is::<f32>() || (&value as &dyn Any).is::<f64>() {
        if !text.contains(['.', 'e', 'E']) && !text.contains("inf") && text != "NaN" {
            text.push_str(".0");
        }
    }
    text
}
numeric_argument!(
    IntegerArgumentType,
    i32,
    read_int,
    "Integer",
    ["0", "123", "-123"]
);
numeric_argument!(
    LongArgumentType,
    i64,
    read_long,
    "Long",
    ["0", "123", "-123"]
);
numeric_argument!(
    FloatArgumentType,
    f32,
    read_float,
    "Float",
    ["0", "1.2", ".5", "-1", "-.5", "-1234.56"]
);
numeric_argument!(
    DoubleArgumentType,
    f64,
    read_double,
    "Double",
    ["0", "1.2", ".5", "-1", "-.5", "-1234.56"]
);

/// Port of BoolArgumentType.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoolArgumentType;
impl ArgumentType for BoolArgumentType {
    type Value = bool;
    fn parse(&self, reader: &mut StringReader) -> Result<bool, CommandSyntaxException> {
        reader.read_boolean()
    }
    fn examples(&self) -> Vec<String> {
        vec!["true".into(), "false".into()]
    }
    fn list_suggestions<S>(
        &self,
        _context: &CommandContext<S>,
        mut builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException> {
        let remaining = builder.remaining_lower_case();
        for value in ["true", "false"] {
            if value.starts_with(&remaining) {
                builder.suggest(value);
            }
        }
        Ok(builder.build())
    }
}

/// Port of StringArgumentType.StringType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringArgumentType {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}
impl StringArgumentType {
    pub fn word() -> Self {
        Self::SingleWord
    }
    pub fn string() -> Self {
        Self::QuotablePhrase
    }
    pub fn greedy_string() -> Self {
        Self::GreedyPhrase
    }
    pub fn escape_if_required(input: &str) -> String {
        if input
            .encode_utf16()
            .all(StringReader::is_allowed_in_unquoted_string)
        {
            return input.into();
        }
        format!("\"{}\"", input.replace('\\', "\\\\").replace('"', "\\\""))
    }
}
impl ArgumentType for StringArgumentType {
    type Value = String;
    fn parse(&self, reader: &mut StringReader) -> Result<String, CommandSyntaxException> {
        match self {
            Self::SingleWord => Ok(reader.read_unquoted_string()),
            Self::QuotablePhrase => reader.read_string(),
            Self::GreedyPhrase => {
                let text = reader.remaining();
                reader.set_cursor(reader.total_length());
                Ok(text)
            }
        }
    }
    fn examples(&self) -> Vec<String> {
        match self {
            Self::SingleWord => vec!["word".into(), "words_with_underscores".into()],
            Self::QuotablePhrase => vec!["\"quoted phrase\"".into(), "word".into(), "\"\"".into()],
            Self::GreedyPhrase => vec![
                "word".into(),
                "words with spaces".into(),
                "\"and symbols\"".into(),
            ],
        }
    }
}
