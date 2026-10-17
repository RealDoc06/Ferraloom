// Ported from Brigadier 1.3.11 CommandSuggestionsTest (MIT).
use super::*;
use std::sync::Arc;
fn check(
    d: &CommandDispatcher<()>,
    input: &str,
    offset: usize,
    cursor: usize,
    range: StringRange,
    texts: &[&str],
) {
    let mut reader = StringReader::new(input);
    reader.set_cursor(offset);
    let p = d.parse_reader(reader, Arc::new(()));
    let suggestions = d.get_completion_suggestions_at(&p, cursor);
    assert_eq!(suggestions.range, range, "{input} at {cursor}");
    assert_eq!(
        suggestions.list,
        texts
            .iter()
            .map(|text| Suggestion::new(range, *text))
            .collect::<Vec<_>>(),
        "{input} at {cursor}"
    );
}
fn roots() -> CommandDispatcher<()> {
    let mut d = CommandDispatcher::new();
    for name in ["foo", "bar", "baz"] {
        d.register(literal(name));
    }
    d
}
fn parent() -> CommandDispatcher<()> {
    let mut d = CommandDispatcher::new();
    d.register(
        literal("parent")
            .then(literal("foo"))
            .then(literal("bar"))
            .then(literal("baz")),
    );
    d
}
#[test]
fn root_commands() {
    check(
        &roots(),
        "",
        0,
        0,
        StringRange::at(0),
        &["bar", "baz", "foo"],
    );
}
#[test]
fn root_commands_with_input_offset() {
    check(
        &roots(),
        "OOO",
        3,
        3,
        StringRange::at(3),
        &["bar", "baz", "foo"],
    );
}
#[test]
fn root_commands_partial() {
    check(
        &roots(),
        "b",
        0,
        1,
        StringRange::between(0, 1),
        &["bar", "baz"],
    );
}
#[test]
fn root_commands_partial_with_input_offset() {
    check(
        &roots(),
        "Zb",
        1,
        2,
        StringRange::between(1, 2),
        &["bar", "baz"],
    );
}
#[test]
fn sub_commands() {
    check(
        &parent(),
        "parent ",
        0,
        7,
        StringRange::at(7),
        &["bar", "baz", "foo"],
    );
}
#[test]
fn moving_cursor_sub_commands() {
    let mut d = CommandDispatcher::new();
    d.register(
        literal("parent_one")
            .then(literal("faz"))
            .then(literal("fbz"))
            .then(literal("gaz")),
    );
    d.register(literal("parent_two"));
    for (cursor, range, texts) in [
        (0, StringRange::at(0), vec!["parent_one", "parent_two"]),
        (
            1,
            StringRange::between(0, 1),
            vec!["parent_one", "parent_two"],
        ),
        (
            7,
            StringRange::between(0, 7),
            vec!["parent_one", "parent_two"],
        ),
        (8, StringRange::between(0, 8), vec!["parent_one"]),
        (10, StringRange::at(0), vec![]),
        (11, StringRange::at(11), vec!["faz", "fbz", "gaz"]),
        (12, StringRange::between(11, 12), vec!["faz", "fbz"]),
        (13, StringRange::between(11, 13), vec!["faz"]),
        (14, StringRange::at(0), vec![]),
        (15, StringRange::at(0), vec![]),
    ] {
        check(&d, "parent_one faz ", 0, cursor, range, &texts);
    }
}
#[test]
fn sub_commands_partial() {
    check(
        &parent(),
        "parent b",
        0,
        8,
        StringRange::between(7, 8),
        &["bar", "baz"],
    );
}
#[test]
fn sub_commands_partial_with_input_offset() {
    check(
        &parent(),
        "junk parent b",
        5,
        13,
        StringRange::between(12, 13),
        &["bar", "baz"],
    );
}
fn redirect() -> CommandDispatcher<()> {
    let mut d = CommandDispatcher::new();
    let actual = d.register(literal("actual").then(literal("sub")));
    d.register(literal("redirect").redirect(actual));
    d
}
#[test]
fn redirect_empty() {
    check(&redirect(), "redirect ", 0, 9, StringRange::at(9), &["sub"]);
}
#[test]
fn redirect_partial() {
    check(
        &redirect(),
        "redirect s",
        0,
        10,
        StringRange::between(9, 10),
        &["sub"],
    );
}
#[test]
fn moving_cursor_redirect() {
    let mut d = CommandDispatcher::new();
    let actual = d.register(
        literal("actual_one")
            .then(literal("faz"))
            .then(literal("fbz"))
            .then(literal("gaz")),
    );
    d.register(literal("actual_two"));
    d.register(literal("redirect_one").redirect(actual));
    d.register(literal("redirect_two").redirect(actual));
    for (cursor, range, texts) in [
        (
            0,
            StringRange::at(0),
            vec!["actual_one", "actual_two", "redirect_one", "redirect_two"],
        ),
        (
            9,
            StringRange::between(0, 9),
            vec!["redirect_one", "redirect_two"],
        ),
        (10, StringRange::between(0, 10), vec!["redirect_one"]),
        (12, StringRange::at(0), vec![]),
        (13, StringRange::at(13), vec!["faz", "fbz", "gaz"]),
        (14, StringRange::between(13, 14), vec!["faz", "fbz"]),
        (15, StringRange::between(13, 15), vec!["faz"]),
        (16, StringRange::at(0), vec![]),
        (17, StringRange::at(0), vec![]),
    ] {
        check(&d, "redirect_one faz ", 0, cursor, range, &texts);
    }
}
#[test]
fn redirect_partial_with_input_offset() {
    check(
        &redirect(),
        "/redirect s",
        1,
        11,
        StringRange::between(10, 11),
        &["sub"],
    );
}
#[test]
fn redirect_lots() {
    let mut d = CommandDispatcher::new();
    let target = d.register(literal("redirect"));
    d.register(literal("redirect").then(
        literal("loop").then(argument("loop", IntegerArgumentType::default()).redirect(target)),
    ));
    check(
        &d,
        "redirect loop 1 loop 02 loop 003 ",
        0,
        33,
        StringRange::at(33),
        &["loop"],
    );
}
#[test]
fn redirect_contextual_argument() {
    let mut d = CommandDispatcher::new();
    let actual = d.register(literal("actual").then(
        argument("arg_one", StringArgumentType::word()).then(
            argument("arg_two", StringArgumentType::word()).suggests(|c, mut b| {
                b.suggest(format!(
                    "contextual_{}",
                    c.get_argument::<String>("arg_one").unwrap()
                ));
                Ok(b.build())
            }),
        ),
    ));
    d.register(literal("redirect").redirect(actual));
    check(
        &d,
        "redirect first ",
        0,
        15,
        StringRange::at(15),
        &["contextual_first"],
    );
}
#[test]
fn execute_simulation() {
    let mut d = CommandDispatcher::new();
    let execute = d.register(literal("execute"));
    d.register(
        literal("execute")
            .then(
                literal("as").then(argument("name", StringArgumentType::word()).redirect(execute)),
            )
            .then(
                literal("store")
                    .then(argument("name", StringArgumentType::word()).redirect(execute)),
            )
            .then(literal("run").executes(|_| Ok(0))),
    );
    check(
        &d,
        "execute as Dinnerbone as",
        0,
        24,
        StringRange::at(0),
        &[],
    );
}
#[test]
fn execute_simulation_partial() {
    let mut d = CommandDispatcher::new();
    let execute = d.register(literal("execute"));
    d.register(
        literal("execute")
            .then(
                literal("as")
                    .then(literal("bar").redirect(execute))
                    .then(literal("baz").redirect(execute)),
            )
            .then(
                literal("store")
                    .then(argument("name", StringArgumentType::word()).redirect(execute)),
            )
            .then(literal("run").executes(|_| Ok(0))),
    );
    check(
        &d,
        "execute as bar as ",
        0,
        18,
        StringRange::at(18),
        &["bar", "baz"],
    );
}
