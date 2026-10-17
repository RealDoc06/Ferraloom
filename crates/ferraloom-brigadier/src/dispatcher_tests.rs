// Ported from Brigadier 1.3.11 CommandDispatcherTest (MIT).
use super::*;
use std::sync::{Arc, Mutex};
fn command(_: &CommandContext<i32>) -> Result<i32, CommandSyntaxException> {
    Ok(42)
}
fn fail() -> CommandSyntaxException {
    CommandExceptionType::simple("failure").create(&[])
}
fn error(d: &CommandDispatcher<i32>, input: &str, kind: ExceptionKind, cursor: usize) {
    let error = d.execute(input, 0).unwrap_err();
    assert_eq!(error.kind, kind);
    assert_eq!(error.cursor, Some(cursor));
}
fn record(d: &mut CommandDispatcher<i32>) -> Arc<Mutex<Vec<(i32, bool, i32)>>> {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let copy = calls.clone();
    d.set_consumer(move |c, success, result| {
        copy.lock().unwrap().push((*c.source, success, result))
    });
    calls
}
#[test]
fn create_and_execute_command() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").executes(command));
    assert_eq!(d.execute("foo", 0).unwrap(), 42);
}
#[test]
fn create_and_execute_offset_command() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").executes(command));
    let mut reader = StringReader::new("/foo");
    reader.set_cursor(1);
    assert_eq!(
        d.execute_parsed(&d.parse_reader(reader, Arc::new(0)))
            .unwrap(),
        42
    );
}
#[test]
fn create_and_merge_commands() {
    let mut d = CommandDispatcher::<i32>::new();
    for child in ["foo", "bar"] {
        d.register(literal("base").then(literal(child).executes(command)));
    }
    for input in ["base foo", "base bar"] {
        assert_eq!(d.execute(input, 0).unwrap(), 42);
    }
}
#[test]
fn execute_unknown_command() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("bar"));
    d.register(literal("baz"));
    error(&d, "foo", ExceptionKind::UnknownCommand, 0);
}
#[test]
fn execute_impermissible_command() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").requires(|_| false));
    error(&d, "foo", ExceptionKind::UnknownCommand, 0);
}
#[test]
fn execute_empty_command() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal(""));
    error(&d, "", ExceptionKind::UnknownCommand, 0);
}
#[test]
fn execute_unknown_subcommand() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").executes(command));
    error(&d, "foo bar", ExceptionKind::UnknownArgument, 4);
}
#[test]
fn execute_incorrect_literal() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").executes(command).then(literal("bar")));
    error(&d, "foo baz", ExceptionKind::UnknownArgument, 4);
}
#[test]
fn execute_ambiguous_incorrect_argument() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo")
            .executes(command)
            .then(literal("bar"))
            .then(literal("baz")),
    );
    error(&d, "foo unknown", ExceptionKind::UnknownArgument, 4);
}
#[test]
fn execute_subcommand() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo")
            .then(literal("a"))
            .then(literal("=").executes(|_| Ok(100)))
            .then(literal("c"))
            .executes(command),
    );
    assert_eq!(d.execute("foo =", 0).unwrap(), 100);
}
#[test]
fn parse_incomplete_literal() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("foo").then(literal("bar").executes(command)));
    let p = d.parse("foo ", 0);
    assert_eq!(p.reader.remaining(), " ");
    assert_eq!(p.context.nodes.len(), 1);
}
#[test]
fn parse_incomplete_argument() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo").then(argument("bar", IntegerArgumentType::default()).executes(command)),
    );
    let p = d.parse("foo ", 0);
    assert_eq!(p.reader.remaining(), " ");
    assert_eq!(p.context.nodes.len(), 1);
}
fn ambiguous(d: &mut CommandDispatcher<i32>) -> NodeId {
    d.register(
        literal("test")
            .then(
                argument("incorrect", IntegerArgumentType::default())
                    .executes(|_| panic!("wrong branch")),
            )
            .then(
                argument("right", IntegerArgumentType::default())
                    .then(argument("sub", IntegerArgumentType::default()).executes(|_| Ok(100))),
            ),
    )
}
#[test]
fn execute_ambiguous_parent_subcommand() {
    let mut d = CommandDispatcher::<i32>::new();
    ambiguous(&mut d);
    assert_eq!(d.execute("test 1 2", 0).unwrap(), 100);
}
#[test]
fn execute_ambiguous_parent_subcommand_via_redirect() {
    let mut d = CommandDispatcher::<i32>::new();
    let target = ambiguous(&mut d);
    d.register(literal("redirect").redirect(target));
    assert_eq!(d.execute("redirect 1 2", 0).unwrap(), 100);
}
#[test]
fn execute_redirected_multiple_times() {
    let mut d = CommandDispatcher::<i32>::new();
    let actual = d.register(literal("actual").executes(command));
    let root = d.root();
    let redirect = d.register(literal("redirected").redirect(root));
    let input = "redirected redirected actual";
    let p = d.parse(input, 0);
    for (context, id, text) in [
        (&p.context, redirect, "redirected"),
        (p.context.child.as_ref().unwrap(), redirect, "redirected"),
        (p.context.last_child(), actual, "actual"),
    ] {
        assert_eq!(context.range.get(input), text);
        assert_eq!(context.nodes.len(), 1);
        assert_eq!(context.root_node, root);
        assert_eq!(
            context.nodes[0],
            ParsedCommandNode {
                node: id,
                range: context.range
            }
        );
    }
    assert_eq!(d.execute_parsed(&p).unwrap(), 42);
}
fn source_redirects(d: &mut CommandDispatcher<i32>) {
    let root = d.root();
    d.register(literal("add").then(
        argument("value", IntegerArgumentType::default()).redirect_with(root, |c| {
            Ok(Arc::new(
                *c.source + c.get_argument::<i32>("value").unwrap(),
            ))
        }),
    ));
    d.register(literal("blank").redirect(root));
    d.register(literal("run").executes(|c| Ok(*c.source)));
}
#[test]
fn correct_execute_context_after_redirect() {
    let mut d = CommandDispatcher::<i32>::new();
    source_redirects(&mut d);
    for (input, source, result) in [
        ("run", 0, 0),
        ("run", 1, 1),
        ("add 5 run", 1, 6),
        ("add 5 add 6 run", 2, 13),
        ("add 5 blank run", 1, 6),
        ("blank add 5 run", 1, 6),
        ("add 5 blank add 6 run", 2, 13),
        ("add 5 blank blank add 6 run", 2, 13),
    ] {
        assert_eq!(d.execute(input, source).unwrap(), result);
    }
}
#[test]
fn shared_redirect_and_execute_nodes() {
    let mut d = CommandDispatcher::<i32>::new();
    let root = d.root();
    d.register(
        literal("add").then(
            argument("value", IntegerArgumentType::default())
                .redirect_with(root, |c| {
                    Ok(Arc::new(
                        *c.source + c.get_argument::<i32>("value").unwrap(),
                    ))
                })
                .executes(|c| Ok(*c.source)),
        ),
    );
    assert_eq!(d.execute("add 5", 1).unwrap(), 1);
    assert_eq!(d.execute("add 5 add 6", 1).unwrap(), 6);
}
#[test]
fn execute_redirected() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    let actual = d.register(literal("actual").executes(command));
    let root = d.root();
    let redirect =
        d.register(literal("redirected").fork(root, |_| Ok(vec![Arc::new(1), Arc::new(2)])));
    let p = d.parse("redirected actual", 0);
    assert_eq!(p.context.nodes[0].node, redirect);
    assert_eq!(p.context.child.as_ref().unwrap().nodes[0].node, actual);
    assert_eq!(*p.context.child.as_ref().unwrap().source, 0);
    assert_eq!(d.execute_parsed(&p).unwrap(), 2);
    assert_eq!(*calls.lock().unwrap(), [(1, true, 42), (2, true, 42)]);
}
fn incomplete_tree(d: &mut CommandDispatcher<i32>) -> NodeId {
    d.register(
        literal("foo")
            .then(
                literal("bar").then(
                    argument("value", IntegerArgumentType::default())
                        .executes(|c| Ok(*c.get_argument::<i32>("value").unwrap())),
                ),
            )
            .then(literal("awa").executes(|_| Ok(2))),
    )
}
#[test]
fn incomplete_redirect_should_throw() {
    let mut d = CommandDispatcher::<i32>::new();
    let foo = incomplete_tree(&mut d);
    d.register(literal("baz").redirect(foo));
    assert_eq!(
        d.execute("baz bar", 0).unwrap_err().kind,
        ExceptionKind::UnknownCommand
    );
}
#[test]
fn redirect_modifier_empty_result() {
    let mut d = CommandDispatcher::<i32>::new();
    let foo = incomplete_tree(&mut d);
    d.register(literal("baz").fork(foo, |_| Ok(vec![])));
    assert_eq!(d.execute("baz bar 100", 0).unwrap(), 0);
}
#[test]
fn execute_orphaned_subcommand() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo")
            .then(argument("bar", IntegerArgumentType::default()))
            .executes(command),
    );
    error(&d, "foo 5", ExceptionKind::UnknownCommand, 5);
}
#[test]
fn execute_invalid_other() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(literal("w").executes(|_| panic!("wrong literal")));
    d.register(literal("world").executes(command));
    assert_eq!(d.execute("world", 0).unwrap(), 42);
}
#[test]
fn parse_no_space_separator() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo").then(argument("bar", IntegerArgumentType::default()).executes(command)),
    );
    error(&d, "foo$", ExceptionKind::UnknownCommand, 0);
}
#[test]
fn execute_invalid_subcommand() {
    let mut d = CommandDispatcher::<i32>::new();
    d.register(
        literal("foo")
            .then(argument("bar", IntegerArgumentType::default()))
            .executes(command),
    );
    error(&d, "foo bar", ExceptionKind::ExpectedNumber("integer"), 4);
}
fn path_tree(d: &mut CommandDispatcher<i32>) -> NodeId {
    let bar = d.build(literal("bar"));
    d.register(literal("foo").then_node(bar));
    bar
}
#[test]
fn get_path() {
    let mut d = CommandDispatcher::<i32>::new();
    let bar = path_tree(&mut d);
    assert_eq!(d.get_path(bar), ["foo", "bar"]);
}
#[test]
fn find_node_exists() {
    let mut d = CommandDispatcher::<i32>::new();
    let bar = path_tree(&mut d);
    assert_eq!(d.find_node(&["foo", "bar"]), Some(bar));
}
#[test]
fn find_node_doesnt_exist() {
    let d = CommandDispatcher::<i32>::new();
    assert_eq!(d.find_node(&["foo", "bar"]), None);
}
#[test]
fn result_consumer_in_non_error_run() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("foo").executes(|_| Ok(5)));
    assert_eq!(d.execute("foo", 0).unwrap(), 5);
    assert_eq!(*calls.lock().unwrap(), [(0, true, 5)]);
}
#[test]
fn result_consumer_in_forked_non_error_run() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("foo").executes(|c| Ok(*c.source)));
    let root = d.root();
    d.register(literal("repeat").fork(root, |_| Ok(vec![Arc::new(9), Arc::new(10), Arc::new(11)])));
    assert_eq!(d.execute("repeat foo", 0).unwrap(), 3);
    assert_eq!(
        *calls.lock().unwrap(),
        [(9, true, 9), (10, true, 10), (11, true, 11)]
    );
}
#[test]
fn exception_in_non_forked_command() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("crash").executes(|_| Err(fail())));
    assert_eq!(
        d.execute("crash", 0).unwrap_err().raw_message.text(),
        "failure"
    );
    assert_eq!(*calls.lock().unwrap(), [(0, false, 0)]);
}
#[test]
fn exception_in_non_forked_redirected_command() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("crash").executes(|_| Err(fail())));
    let root = d.root();
    d.register(literal("redirect").redirect(root));
    assert_eq!(
        d.execute("redirect crash", 0)
            .unwrap_err()
            .raw_message
            .text(),
        "failure"
    );
    assert_eq!(*calls.lock().unwrap(), [(0, false, 0)]);
}
#[test]
fn exception_in_forked_redirected_command() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("crash").executes(|_| Err(fail())));
    let root = d.root();
    d.register(literal("redirect").fork(root, |c| Ok(vec![c.source.clone()])));
    assert_eq!(d.execute("redirect crash", 0).unwrap(), 0);
    assert_eq!(*calls.lock().unwrap(), [(0, false, 0)]);
}
#[test]
fn exception_in_non_forked_redirect() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("noop").executes(|_| panic!("should not execute")));
    let root = d.root();
    d.register(literal("redirect").redirect_with(root, |_| Err(fail())));
    assert_eq!(
        d.execute("redirect noop", 0)
            .unwrap_err()
            .raw_message
            .text(),
        "failure"
    );
    assert_eq!(*calls.lock().unwrap(), [(0, false, 0)]);
}
#[test]
fn exception_in_forked_redirect() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("noop").executes(|_| panic!("should not execute")));
    let root = d.root();
    d.register(literal("redirect").fork(root, |_| Err(fail())));
    assert_eq!(d.execute("redirect noop", 0).unwrap(), 0);
    assert_eq!(*calls.lock().unwrap(), [(0, false, 0)]);
}
#[test]
fn partial_exception_in_forked_redirect() {
    let mut d = CommandDispatcher::<i32>::new();
    let calls = record(&mut d);
    d.register(literal("run").executes(|_| Ok(3)));
    let root = d.root();
    d.register(literal("split").fork(root, |_| Ok(vec![Arc::new(0), Arc::new(1), Arc::new(2)])));
    d.register(literal("filter").fork(root, |c| {
        if *c.source == 1 {
            Err(fail())
        } else {
            Ok(vec![c.source.clone()])
        }
    }));
    assert_eq!(d.execute("split filter run", 0).unwrap(), 2);
    assert_eq!(
        *calls.lock().unwrap(),
        [(1, false, 0), (0, true, 3), (2, true, 3)]
    );
}
