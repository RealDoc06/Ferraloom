// Ported from Brigadier 1.3.11 tree and builder tests (MIT).
use super::*;
use std::sync::Arc;
fn new_context(d: &CommandDispatcher<()>, input: &str) -> CommandContext<()> {
    CommandContext::new(Arc::new(()), input, d.root(), 0)
}
macro_rules! shared_node_tests {
    ($module:ident,$builder:expr) => {
        mod $module {
            use super::*;
            fn setup() -> (CommandDispatcher<()>, NodeId) {
                let mut d = CommandDispatcher::new();
                let n = d.build($builder);
                (d, n)
            }
            #[test]
            fn add_child() {
                let (mut d, n) = setup();
                for name in ["child1", "child2", "child1"] {
                    let child = d.build(literal(name));
                    d.add_child(n, child);
                }
                assert_eq!(d.node(n).children().len(), 2);
            }
            #[test]
            fn add_child_merges_grandchildren() {
                let (mut d, n) = setup();
                for name in ["grandchild1", "grandchild2"] {
                    let child = d.build(literal("child").then(literal(name)));
                    d.add_child(n, child);
                }
                assert_eq!(d.node(n).children().len(), 1);
                assert_eq!(d.node(d.node(n).children()[0]).children().len(), 2);
            }
            #[test]
            fn add_child_preserves_command() {
                let (mut d, n) = setup();
                let child = d.build(literal("child").executes(|_| Ok(42)));
                d.add_child(n, child);
                let extra = d.build(literal("child"));
                d.add_child(n, extra);
                let child = d.child(n, "child").unwrap();
                assert_eq!(
                    d.node(child).command.as_ref().unwrap()(&new_context(&d, "")).unwrap(),
                    42
                );
            }
            #[test]
            fn add_child_overwrites_command() {
                let (mut d, n) = setup();
                let child = d.build(literal("child"));
                d.add_child(n, child);
                let extra = d.build(literal("child").executes(|_| Ok(42)));
                d.add_child(n, extra);
                let child = d.child(n, "child").unwrap();
                assert_eq!(
                    d.node(child).command.as_ref().unwrap()(&new_context(&d, "")).unwrap(),
                    42
                );
            }
        }
    };
}
shared_node_tests!(root_shared, ArgumentBuilder::root());
shared_node_tests!(literal_shared, literal("foo"));
shared_node_tests!(
    argument_shared,
    argument("foo", IntegerArgumentType::default())
);
mod literal_node {
    use super::*;
    fn setup() -> (CommandDispatcher<()>, NodeId) {
        let mut d = CommandDispatcher::new();
        let n = d.build(literal("foo"));
        (d, n)
    }
    #[test]
    fn parse() {
        let (d, n) = setup();
        let mut r = StringReader::new("foo bar");
        let mut c = new_context(&d, r.string());
        d.parse_node(n, &mut r, &mut c).unwrap();
        assert_eq!(r.remaining(), " bar");
    }
    #[test]
    fn parse_exact() {
        let (d, n) = setup();
        let mut r = StringReader::new("foo");
        let mut c = new_context(&d, r.string());
        d.parse_node(n, &mut r, &mut c).unwrap();
        assert_eq!(r.remaining(), "");
    }
    #[test]
    fn parse_similar() {
        let (d, n) = setup();
        let mut r = StringReader::new("foobar");
        let mut c = new_context(&d, r.string());
        let e = d.parse_node(n, &mut r, &mut c).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::LiteralIncorrect);
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn parse_invalid() {
        let (d, n) = setup();
        let mut r = StringReader::new("bar");
        let mut c = new_context(&d, r.string());
        let e = d.parse_node(n, &mut r, &mut c).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::LiteralIncorrect);
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn usage() {
        let (d, n) = setup();
        assert_eq!(d.node(n).usage_text(), "foo");
    }
    #[test]
    fn suggestions() {
        let (d, n) = setup();
        for (input, texts) in [
            ("", vec![Suggestion::new(StringRange::at(0), "foo")]),
            ("foo", vec![]),
            ("food", vec![]),
            ("b", vec![]),
        ] {
            assert_eq!(
                d.list_suggestions(
                    n,
                    &new_context(&d, input),
                    SuggestionsBuilder::new(input, 0)
                )
                .unwrap()
                .list,
                texts
            );
        }
    }
    #[test]
    fn create_builder() {
        let (mut d, n) = setup();
        let copy = d.build(d.node(n).create_builder());
        assert_eq!(d.node(copy).name(), d.node(n).name());
        assert!(Arc::ptr_eq(
            &d.node(copy).requirement,
            &d.node(n).requirement
        ));
        assert!(d.node(copy).command.is_none());
    }
}
mod argument_node {
    use super::*;
    fn setup() -> (CommandDispatcher<()>, NodeId) {
        let mut d = CommandDispatcher::new();
        let n = d.build(argument("foo", IntegerArgumentType::default()));
        (d, n)
    }
    #[test]
    fn parse() {
        let (d, n) = setup();
        let mut r = StringReader::new("123 456");
        let mut c = new_context(&d, r.string());
        d.parse_node(n, &mut r, &mut c).unwrap();
        assert_eq!(c.get_argument::<i32>("foo"), Some(&123));
        assert_eq!(r.remaining(), " 456");
    }
    #[test]
    fn usage() {
        let (d, n) = setup();
        assert_eq!(d.node(n).usage_text(), "<foo>");
    }
    #[test]
    fn suggestions() {
        let (d, n) = setup();
        assert!(
            d.list_suggestions(n, &new_context(&d, ""), SuggestionsBuilder::new("", 0))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn create_builder() {
        let (mut d, n) = setup();
        let copy = d.build(d.node(n).create_builder());
        assert_eq!(d.node(copy).name(), d.node(n).name());
        assert_eq!(
            d.node(copy).argument_type::<IntegerArgumentType>(),
            Some(&IntegerArgumentType::default())
        );
        assert!(Arc::ptr_eq(
            &d.node(copy).requirement,
            &d.node(n).requirement
        ));
        assert!(d.node(copy).command.is_none());
    }
}
mod root_node {
    use super::*;
    #[test]
    fn parse() {
        let d = CommandDispatcher::new();
        let mut r = StringReader::new("hello world");
        let mut c = new_context(&d, r.string());
        d.parse_node(d.root(), &mut r, &mut c).unwrap();
        assert_eq!(r.cursor(), 0);
    }
    #[test]
    #[should_panic(expected = "Cannot add a root")]
    fn add_child_no_root() {
        let mut d = CommandDispatcher::<()>::new();
        d.add_child(d.root(), d.root());
    }
    #[test]
    fn usage() {
        let d = CommandDispatcher::<()>::new();
        assert_eq!(d.node(d.root()).usage_text(), "");
    }
    #[test]
    fn suggestions() {
        let d = CommandDispatcher::new();
        assert!(
            d.list_suggestions(
                d.root(),
                &new_context(&d, ""),
                SuggestionsBuilder::new("", 0)
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    #[should_panic(expected = "Cannot convert root")]
    fn create_builder() {
        let d = CommandDispatcher::<()>::new();
        d.node(d.root()).create_builder();
    }
}
mod builder {
    use super::*;
    #[test]
    fn arguments() {
        let mut d = CommandDispatcher::<()>::new();
        let n = d.build(literal("foo").then(argument("bar", IntegerArgumentType::default())));
        assert_eq!(d.node(n).children().len(), 1);
        assert!(d.child(n, "bar").is_some());
    }
    #[test]
    fn redirect() {
        let mut d = CommandDispatcher::<()>::new();
        let root = d.root();
        let n = d.build(literal("foo").redirect(root));
        assert_eq!(d.node(n).redirect(), Some(root));
    }
    #[test]
    #[should_panic(expected = "Cannot forward")]
    fn redirect_with_child() {
        literal::<()>("foo")
            .then(literal("bar"))
            .redirect(NodeId(0));
    }
    #[test]
    #[should_panic(expected = "Cannot add children")]
    fn then_with_redirect() {
        literal::<()>("foo")
            .redirect(NodeId(0))
            .then(literal("bar"));
    }
    #[test]
    fn literal_build() {
        let mut d = CommandDispatcher::<()>::new();
        let n = d.build(literal("foo"));
        assert_eq!(d.node(n).name(), "foo");
    }
    #[test]
    fn literal_build_with_executor() {
        let mut d = CommandDispatcher::new();
        let n = d.build(literal("foo").executes(|_| Ok(42)));
        assert_eq!(d.node(n).name(), "foo");
        assert_eq!(
            d.node(n).command.as_ref().unwrap()(&new_context(&d, "")).unwrap(),
            42
        );
    }
    #[test]
    fn literal_build_with_children() {
        let mut d = CommandDispatcher::<()>::new();
        let n = d.build(
            literal("foo")
                .then(argument("bar", IntegerArgumentType::default()))
                .then(argument("baz", IntegerArgumentType::default())),
        );
        assert_eq!(d.node(n).children().len(), 2);
    }
    #[test]
    fn argument_build() {
        let mut d = CommandDispatcher::<()>::new();
        let n = d.build(argument("foo", IntegerArgumentType::default()));
        assert_eq!(d.node(n).name(), "foo");
        assert_eq!(
            d.node(n).argument_type::<IntegerArgumentType>(),
            Some(&IntegerArgumentType::default())
        );
    }
    #[test]
    fn argument_build_with_executor() {
        let mut d = CommandDispatcher::new();
        let n = d.build(argument("foo", IntegerArgumentType::default()).executes(|_| Ok(42)));
        assert_eq!(d.node(n).name(), "foo");
        assert_eq!(
            d.node(n).argument_type::<IntegerArgumentType>(),
            Some(&IntegerArgumentType::default())
        );
        assert_eq!(
            d.node(n).command.as_ref().unwrap()(&new_context(&d, "")).unwrap(),
            42
        );
    }
    #[test]
    fn argument_build_with_children() {
        let mut d = CommandDispatcher::<()>::new();
        let n = d.build(
            argument("foo", IntegerArgumentType::default())
                .then(argument("bar", IntegerArgumentType::default()))
                .then(argument("baz", IntegerArgumentType::default())),
        );
        assert_eq!(d.node(n).children().len(), 2);
    }
}
