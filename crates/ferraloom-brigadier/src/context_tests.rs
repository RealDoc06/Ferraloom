// Ported from Brigadier 1.3.11 CommandContextTest, ContextChainTest,
// ParsedArgumentTest and exception factory tests (MIT).
use super::*;
use std::sync::{Arc, Mutex};
fn context() -> CommandContext<i32> {
    CommandContext::new(Arc::new(1), "", NodeId(0), 0)
}
fn with_argument() -> CommandContext<i32> {
    let mut c = context();
    c.input = "123".into();
    c.arguments.insert(
        "foo".into(),
        ParsedArgument {
            range: StringRange::between(0, 1),
            value: Arc::new(123i32),
        },
    );
    c
}
#[test]
fn get_argument_nonexistent() {
    assert!(context().get_argument::<i32>("foo").is_none());
}
#[test]
fn get_argument_wrong_type() {
    assert!(with_argument().get_argument::<String>("foo").is_none());
}
#[test]
fn get_argument() {
    assert_eq!(with_argument().get_argument::<i32>("foo"), Some(&123));
}
#[test]
fn source() {
    assert_eq!(*context().source, 1);
}
#[test]
fn root_node() {
    assert_eq!(context().root_node, NodeId(0));
}
#[test]
fn parsed_argument_get_raw() {
    let argument = ParsedArgument {
        range: StringRange::between(2, 5),
        value: Arc::new(String::new()),
    };
    assert_eq!(argument.range.get("0123456789"), "234");
}
type Calls = Arc<Mutex<Vec<(&'static str, bool, i32)>>>;
fn record() -> (ResultConsumer<&'static str>, Calls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let out = calls.clone();
    let consumer: ResultConsumer<&'static str> =
        Arc::new(move |c, s, r| out.lock().unwrap().push((*c.source, s, r)));
    (consumer, calls)
}
#[test]
fn execute_all_single_command() {
    let mut d = CommandDispatcher::<&'static str>::new();
    d.register(literal("foo").executes(|c| {
        assert_eq!(*c.source, "runtime_source");
        Ok(4)
    }));
    let p = d.parse("foo", "compile_source");
    let chain = ContextChain::try_flatten(&p.context).unwrap();
    let (consumer, calls) = record();
    assert_eq!(
        chain
            .execute_all(Arc::new("runtime_source"), &consumer)
            .unwrap(),
        4
    );
    assert_eq!(*calls.lock().unwrap(), [("runtime_source", true, 4)]);
}
#[test]
fn execute_all_redirected_command() {
    let mut d = CommandDispatcher::<&'static str>::new();
    d.register(literal("foo").executes(|c| {
        assert_eq!(*c.source, "redirected_source");
        Ok(4)
    }));
    let root = d.root();
    d.register(literal("bar").redirect_with(root, |_| Ok(Arc::new("redirected_source"))));
    let p = d.parse("bar foo", "compile_source");
    let chain = ContextChain::try_flatten(&p.context).unwrap();
    let (consumer, calls) = record();
    assert_eq!(
        chain
            .execute_all(Arc::new("runtime_source"), &consumer)
            .unwrap(),
        4
    );
    assert_eq!(*calls.lock().unwrap(), [("redirected_source", true, 4)]);
}
#[test]
fn single_stage_execution() {
    let mut d = CommandDispatcher::new();
    d.register(literal("foo").executes(|_| Ok(1)));
    let p = d.parse("foo", ());
    let chain = ContextChain::try_flatten(&p.context).unwrap();
    assert!(chain.modifiers.is_empty());
    assert!(std::ptr::eq(chain.top_context(), &p.context));
    assert!(chain.next_stage().is_none());
}
#[test]
fn multi_stage_execution() {
    let mut d = CommandDispatcher::new();
    d.register(literal("foo").executes(|_| Ok(1)));
    let root = d.root();
    d.register(literal("bar").redirect(root));
    let p = d.parse("bar bar foo", ());
    let s0 = ContextChain::try_flatten(&p.context).unwrap();
    assert_eq!(s0.modifiers.len(), 2);
    assert!(std::ptr::eq(s0.top_context(), &p.context));
    let s1 = s0.next_stage().unwrap();
    assert_eq!(s1.modifiers.len(), 1);
    assert!(std::ptr::eq(
        s1.top_context(),
        p.context.child.as_deref().unwrap()
    ));
    let s2 = s1.next_stage().unwrap();
    assert!(s2.modifiers.is_empty());
    assert!(std::ptr::eq(s2.top_context(), p.context.last_child()));
    assert!(s2.next_stage().is_none());
}
#[test]
fn missing_execute() {
    let mut d = CommandDispatcher::new();
    d.register(literal("foo").executes(|_| Ok(1)));
    let root = d.root();
    d.register(literal("bar").redirect(root));
    let p = d.parse("bar bar", ());
    assert!(ContextChain::try_flatten(&p.context).is_none());
}
#[test]
fn simple_create_with_context() {
    let t = CommandExceptionType::simple("error");
    let mut reader = StringReader::new("Foo bar");
    reader.set_cursor(5);
    let e = t.create_with_context(&reader, &[]);
    assert_eq!(e.kind, t.kind);
    assert_eq!(e.input.as_deref(), Some("Foo bar"));
    assert_eq!(e.cursor, Some(5));
}
#[test]
fn exception_context_none() {
    assert!(
        CommandExceptionType::simple("error")
            .create(&[])
            .context()
            .is_none()
    );
}
#[test]
fn exception_context_short() {
    let e = CommandExceptionType::simple("error")
        .create(&[])
        .with_context("Hello world!", 5);
    assert_eq!(e.context().as_deref(), Some("Hello<--[HERE]"));
    assert_eq!(e.to_string(), "error at position 5: Hello<--[HERE]");
}
#[test]
fn exception_context_long() {
    let e = CommandExceptionType::simple("error")
        .create(&[])
        .with_context("Hello world! This has an error in it. Oh dear!", 20);
    assert_eq!(e.context().as_deref(), Some("...d! This ha<--[HERE]"));
}
#[test]
fn dynamic_create_with_context() {
    let t = CommandExceptionType::new(ExceptionKind::Custom("greeting".into()), |a| {
        Arc::new(format!("Hello, {}!", a[0]))
    });
    let mut r = StringReader::new("Foo bar");
    r.set_cursor(5);
    let e = t.create_with_context(&r, &["World".into()]);
    assert_eq!(e.kind, t.kind);
    assert_eq!(e.raw_message.text(), "Hello, World!");
    assert_eq!(e.input.as_deref(), Some("Foo bar"));
    assert_eq!(e.cursor, Some(5));
}
