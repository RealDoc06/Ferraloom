//! Port of BrigadierExceptions. Errors retain a translatable component on the wire.
use ferraloom_brigadier::{
    BuiltInExceptionProvider, CommandSyntaxException, ExceptionKind, Message, MessageRef,
    StringReader,
};
use ferraloom_protocol::chat::{Argument, Component};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug)]
pub struct TranslatedMessage {
    pub component: Component,
    pub rendered: String,
}
impl Message for TranslatedMessage {
    fn text(&self) -> &str {
        &self.rendered
    }
}
pub fn translations() -> &'static BTreeMap<String, String> {
    static TRANSLATIONS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TRANSLATIONS.get_or_init(|| {
        serde_json::from_str(include_str!("vanilla/translations.json"))
            .expect("vanilla command translations")
    })
}
/// Port of TranslatableContents' %s, %n$s and %% substitutions for plain error text.
pub fn render(key: &str, args: &[String]) -> String {
    let template = translations().get(key).map_or(key, String::as_str);
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    let mut next_arg = 0;
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            result.push('%');
            continue;
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            digits.push(chars.next().unwrap());
        }
        let index = if chars.peek() == Some(&'$') {
            chars.next();
            digits.parse::<usize>().unwrap_or(1).saturating_sub(1)
        } else {
            let i = next_arg;
            next_arg += 1;
            i
        };
        if chars.next() == Some('s') {
            result.push_str(args.get(index).map_or("", String::as_str));
        } else {
            result.push('%');
            result.push_str(&digits);
        }
    }
    result
}
pub fn translated(key: &str, args: &[String]) -> TranslatedMessage {
    TranslatedMessage {
        component: Component::translatable(
            key,
            args.iter().cloned().map(Argument::String).collect(),
        ),
        rendered: render(key, args),
    }
}
pub fn error(key: &str, args: &[String]) -> CommandSyntaxException {
    CommandSyntaxException::new(
        ExceptionKind::Custom(key.into()),
        Arc::new(translated(key, args)),
    )
}
pub fn error_at(reader: &StringReader, key: &str, args: &[String]) -> CommandSyntaxException {
    error(key, args).with_context(reader.string(), reader.cursor())
}
pub fn error_component(error: &CommandSyntaxException) -> Component {
    let message = error.raw_message.as_ref() as &dyn std::any::Any;
    message.downcast_ref::<TranslatedMessage>().map_or_else(
        || Component::literal(error.raw_message.text()),
        |message| message.component.clone(),
    )
}
#[derive(Default)]
pub struct BrigadierExceptions;
impl BuiltInExceptionProvider for BrigadierExceptions {
    fn message(&self, kind: &ExceptionKind, args: &[String]) -> MessageRef {
        use ExceptionKind::*;
        let mut args = args.to_vec();
        let key = match kind {
            TooLow(t) | TooHigh(t) => {
                args.swap(0, 1);
                format!(
                    "argument.{}.{}",
                    t.to_lowercase(),
                    if matches!(kind, TooLow(_)) {
                        "low"
                    } else {
                        "big"
                    }
                )
            }
            ExpectedNumber(t) => format!("parsing.{}.expected", t.to_lowercase()),
            InvalidNumber(t) => format!("parsing.{}.invalid", t.to_lowercase()),
            LiteralIncorrect => "argument.literal.incorrect".into(),
            ExpectedStartOfQuote => "parsing.quote.expected.start".into(),
            ExpectedEndOfQuote => "parsing.quote.expected.end".into(),
            InvalidEscape => "parsing.quote.escape".into(),
            InvalidBool => "parsing.bool.invalid".into(),
            ExpectedBool => "parsing.bool.expected".into(),
            ExpectedSymbol => "parsing.expected".into(),
            UnknownCommand => "command.unknown.command".into(),
            UnknownArgument => "command.unknown.argument".into(),
            ExpectedArgumentSeparator => "command.expected.separator".into(),
            ParseException => "command.exception".into(),
            Custom(key) => key.clone(),
        };
        Arc::new(translated(&key, &args))
    }
}
