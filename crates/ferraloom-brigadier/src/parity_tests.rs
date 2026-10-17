// Additional coverage for integration behavior absent from the upstream JUnit suite.
use super::*;
use std::sync::Arc;
#[test]
fn utf16_reader_and_context_offsets() {
    let mut r = StringReader::new("\"🦀é\" bad");
    assert_eq!(r.read_string().unwrap(), "🦀é");
    assert_eq!(r.cursor(), 5);
    r.skip();
    let e = r.read_int().unwrap_err();
    assert_eq!(e.cursor, Some(6));
    assert_eq!(e.context().as_deref(), Some("\"🦀é\" <--[HERE]"));
}
#[test]
fn utf16_suggestion_replacement() {
    let s = Suggestion::new(StringRange::between(3, 4), "世界");
    assert_eq!(s.apply("🦀 a!"), "🦀 世界!");
}
#[test]
fn java_whitespace_excludes_nonbreaking_spaces() {
    let mut r = StringReader::new("\t\u{2000}\u{a0}a");
    r.skip_whitespace();
    assert_eq!(r.cursor(), 2);
    assert_eq!(r.peek(), 0xa0);
}
#[test]
fn bounded_arguments_restore_start() {
    let mut r = StringReader::new("x -5");
    r.set_cursor(2);
    let e = ArgumentType::parse(&IntegerArgumentType::min(0), &mut r).unwrap_err();
    assert_eq!(r.cursor(), 2);
    assert_eq!(
        e.to_string(),
        "Integer must not be less than 0, found -5 at position 2: x <--[HERE]"
    );
}
#[test]
fn exact_literal_precedes_argument_even_when_denied() {
    let mut d = CommandDispatcher::new();
    d.register(
        literal("foo")
            .then(
                argument("word", StringArgumentType::word())
                    .executes(|_| panic!("must not try argument")),
            )
            .then(literal("bar").requires(|_| false).executes(|_| Ok(2))),
    );
    assert_eq!(
        d.execute("foo bar", ()).unwrap_err().kind,
        ExceptionKind::UnknownArgument
    );
}
#[test]
fn ambiguity_detection_uses_argument_examples() {
    let mut d = CommandDispatcher::<()>::new();
    let foo = d.register(literal("foo"));
    let word = d.register(argument("word", StringArgumentType::word()));
    let mut found = Vec::new();
    d.find_ambiguities(|parent, child, sibling, inputs| {
        found.push((parent, child, sibling, inputs.to_vec()))
    });
    assert_eq!(found, [(d.root(), foo, word, vec!["foo".into()])]);
}
#[test]
fn structural_indices_and_flags_survive_merges() {
    let mut d = CommandDispatcher::<()>::new();
    let root = d.root();
    let target = d.register(literal("foo").executes(|_| Ok(1)));
    let redirect = d.register(literal("alias").redirect(target));
    d.register(
        literal("foo").then(
            argument("number", IntegerArgumentType::new(1, 9)).suggests(|_, b| Ok(b.build())),
        ),
    );
    assert_eq!(d.node(root).flags(), 0);
    assert_eq!(d.node(target).flags(), 5);
    assert_eq!(d.node(redirect).flags(), 9);
    assert_eq!(d.node(redirect).redirect(), Some(target));
    let number = d.find_node(&["foo", "number"]).unwrap();
    assert_eq!(d.node(number).flags(), 18);
    assert_eq!(
        d.node(number).argument_type::<IntegerArgumentType>(),
        Some(&IntegerArgumentType::new(1, 9))
    );
    assert_eq!(d.get_path(number), ["foo", "number"]);
}
#[derive(Debug)]
struct Translation {
    key: &'static str,
}
impl Message for Translation {
    fn text(&self) -> &str {
        self.key
    }
}
struct TranslatableExceptions;
impl BuiltInExceptionProvider for TranslatableExceptions {
    fn message(&self, _: &ExceptionKind, _: &[String]) -> MessageRef {
        Arc::new(Translation {
            key: "argument.integer.expected",
        })
    }
}
#[test]
fn custom_exception_provider_reaches_nested_arguments() {
    let mut d = CommandDispatcher::new();
    d.set_exception_provider(Arc::new(TranslatableExceptions));
    d.register(
        literal("foo").then(argument("n", IntegerArgumentType::default()).executes(|_| Ok(1))),
    );
    let e = d.execute("foo nope", ()).unwrap_err();
    assert_eq!(e.kind, ExceptionKind::ExpectedNumber("integer"));
    assert_eq!(e.raw_message.text(), "argument.integer.expected");
    assert_eq!(e.cursor, Some(4));
}
#[test]
fn suggestion_tooltips_survive_merge_and_expansion() {
    let tooltip: MessageRef = Arc::new("help".to_string());
    let a = Suggestion::new(StringRange::at(5), "ar").with_tooltip(tooltip.clone());
    let b = Suggestion::new(StringRange::between(4, 5), "baz");
    let r = Suggestions::create("foo b", [a, b]);
    assert_eq!(r.range, StringRange::between(4, 5));
    assert_eq!(r.list[0].text, "bar");
    assert!(Arc::ptr_eq(r.list[0].tooltip.as_ref().unwrap(), &tooltip));
}
#[derive(Debug, PartialEq)]
struct Coordinates(i32, i32);
struct CoordinatesArgument;
impl ArgumentType for CoordinatesArgument {
    type Value = Coordinates;
    fn parse(&self, r: &mut StringReader) -> Result<Coordinates, CommandSyntaxException> {
        let x = r.read_int()?;
        r.expect(44)?;
        let y = r.read_int()?;
        Ok(Coordinates(x, y))
    }
    fn examples(&self) -> Vec<String> {
        vec!["1,2".into()]
    }
}
#[test]
fn custom_argument_preserves_concrete_type_without_source_clone() {
    struct Source;
    let mut d = CommandDispatcher::<Source>::new();
    d.register(
        literal("pos").then(argument("where", CoordinatesArgument).executes(|c| {
            assert_eq!(
                c.get_argument::<Coordinates>("where"),
                Some(&Coordinates(1, 2))
            );
            Ok(7)
        })),
    );
    assert_eq!(d.execute("pos 1,2", Source).unwrap(), 7);
}
#[test]
fn java_subnormal_error_messages_use_nearest_two_digits() {
    let mut r = StringReader::new("0");
    let e = ArgumentType::parse(&FloatArgumentType::min(f32::from_bits(1)), &mut r).unwrap_err();
    assert_eq!(
        e.raw_message.text(),
        "Float must not be less than 1.4E-45, found 0.0"
    );
    let e = ArgumentType::parse(&DoubleArgumentType::min(f64::from_bits(1)), &mut r).unwrap_err();
    assert_eq!(
        e.raw_message.text(),
        "Double must not be less than 4.9E-324, found 0.0"
    );
}
