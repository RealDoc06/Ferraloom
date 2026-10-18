//! Port of BrigadierExceptions. Errors retain a translatable component on the wire.
use ferraloom_brigadier::{
    BuiltInExceptionProvider, CommandSyntaxException, ExceptionKind, Message, MessageRef,
    StringReader,
};
use ferraloom_protocol::chat::{Argument, Component, Numeric};
use std::sync::Arc;

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
pub use crate::language::{java_float_string, plain_text, render, translations};

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
