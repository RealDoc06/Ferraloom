//! Port of BrigadierExceptions. Errors retain a translatable component on the wire.
use ferraloom_brigadier::{
    BuiltInExceptionProvider, CommandSyntaxException, ExceptionKind, Message, MessageRef,
    StringReader,
};
use ferraloom_protocol::chat::{Argument, Component, Numeric};
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
/// Float.toString formatting used by translatableEscape's numeric arguments.
pub fn java_float_string(value: f32) -> String {
    // The Java-checked SNBT printer shares Float.toString's exact decimal selection.
    let mut text = ferraloom_nbt::Tag::Float(value).to_string();
    text.pop(); // FloatTag appends the SNBT type suffix after the number.
    text
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
pub fn error_with_arguments(key: &str, arguments: Vec<Argument>) -> CommandSyntaxException {
    let component = Component::translatable(key, arguments);
    let rendered = plain_text(&component);
    CommandSyntaxException::new(
        ExceptionKind::Custom(key.into()),
        Arc::new(TranslatedMessage {
            component,
            rendered,
        }),
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
            ExpectedNumber(t) => format!("parsing.{}.expected", parsing_number_name(t)),
            InvalidNumber(t) => format!("parsing.{}.invalid", parsing_number_name(t)),
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
        let mut message = translated(&key, &args);
        if let TooLow(kind) | TooHigh(kind) = kind {
            message.component = Component::translatable(
                key,
                args.iter()
                    .map(|value| {
                        Argument::Number(match *kind {
                            "Integer" => Numeric::Int(value.parse().expect("integer bound")),
                            "Long" => Numeric::Long(value.parse().expect("long bound")),
                            "Float" => {
                                let value = value.parse::<f32>().expect("float bound");
                                Numeric::Float(value.into())
                            }
                            "Double" => {
                                let value = value.parse::<f64>().expect("double bound");
                                Numeric::Double(value.into())
                            }
                            _ => unreachable!("Brigadier numeric argument type"),
                        })
                    })
                    .collect(),
            );
        }
        Arc::new(message)
    }
}

fn parsing_number_name(kind: &str) -> String {
    if kind.eq_ignore_ascii_case("integer") {
        "int".into()
    } else {
        kind.to_lowercase()
    }
}

/// Server-side rendering for console feedback; clients receive the original component.
pub fn plain_text(component: &Component) -> String {
    use ferraloom_protocol::chat::{Argument, Contents};
    let mut result = match &component.contents {
        Contents::Text(text) => text.clone(),
        Contents::Translatable { key, args, .. } => render(
            key,
            &args
                .iter()
                .map(|arg| match arg {
                    Argument::String(s) => s.clone(),
                    Argument::Number(n) => match n {
                        ferraloom_protocol::chat::Numeric::Byte(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Short(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Int(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Long(v) => v.to_string(),
                        ferraloom_protocol::chat::Numeric::Float(v) => java_float_string(v.0),
                        ferraloom_protocol::chat::Numeric::Double(v) => {
                            let mut text = ferraloom_nbt::Tag::Double(v.0).to_string();
                            text.pop();
                            text
                        }
                    },
                    Argument::Boolean(v) => v.to_string(),
                    Argument::Component(c) => plain_text(c),
                })
                .collect::<Vec<_>>(),
        ),
        _ => String::new(),
    };
    for extra in &component.extra {
        result.push_str(&plain_text(extra));
    }
    result
}
/// Port of Commands#finishParsing's second failure line and its suggest-command action.
pub fn syntax_context(error: &CommandSyntaxException, command: &str) -> Option<Component> {
    use ferraloom_protocol::chat::{ClickEvent, TextColor};
    let input = error.input.as_ref()?.encode_utf16().collect::<Vec<_>>();
    let cursor = error.cursor?.min(input.len());
    let mut context = Component::literal("");
    context.style.color = Some(TextColor::parse("gray").unwrap());
    context.style.click_event = Some(ClickEvent::SuggestCommand(format!("/{command}")));
    if cursor > 10 {
        context.extra.push(Component::literal("..."));
    }
    if cursor > 0 {
        context
            .extra
            .push(Component::literal(String::from_utf16_lossy(
                &input[cursor.saturating_sub(10)..cursor],
            )));
    }
    if cursor < input.len() {
        let mut remaining = Component::literal(String::from_utf16_lossy(&input[cursor..]));
        remaining.style.color = Some(TextColor::parse("red").unwrap());
        remaining.style.underlined = Some(true);
        context.extra.push(remaining);
    }
    let mut here = Component::translatable("command.context.here", vec![]);
    here.style.color = Some(TextColor::parse("red").unwrap());
    here.style.italic = Some(true);
    context.extra.push(here);
    Some(context)
}
