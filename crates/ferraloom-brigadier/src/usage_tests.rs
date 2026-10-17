// Ported from Brigadier 1.3.11 CommandDispatcherUsagesTest (MIT).
use super::*;
use std::sync::Arc;
fn setup() -> CommandDispatcher<()> {
    let mut d = CommandDispatcher::new();

    d.register(
        literal("a")
            .then(
                literal("1")
                    .then(literal("i").executes(|_| Ok(0)))
                    .then(literal("ii").executes(|_| Ok(0))),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(|_| Ok(0)))
                    .then(literal("ii").executes(|_| Ok(0))),
            ),
    );
    d.register(literal("b").then(literal("1").executes(|_| Ok(0))));
    d.register(literal("c").executes(|_| Ok(0)));
    d.register(literal("d").requires(|_| false).executes(|_| Ok(0)));
    d.register(
        literal("e").executes(|_| Ok(0)).then(
            literal("1")
                .executes(|_| Ok(0))
                .then(literal("i").executes(|_| Ok(0)))
                .then(literal("ii").executes(|_| Ok(0))),
        ),
    );
    d.register(
        literal("f")
            .then(
                literal("1")
                    .then(literal("i").executes(|_| Ok(0)))
                    .then(literal("ii").executes(|_| Ok(0)).requires(|_| false)),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(|_| Ok(0)).requires(|_| false))
                    .then(literal("ii").executes(|_| Ok(0))),
            ),
    );
    d.register(
        literal("g")
            .executes(|_| Ok(0))
            .then(literal("1").then(literal("i").executes(|_| Ok(0)))),
    );
    d.register(
        literal("h")
            .executes(|_| Ok(0))
            .then(literal("1").then(literal("i").executes(|_| Ok(0))))
            .then(literal("2").then(literal("i").then(literal("ii").executes(|_| Ok(0)))))
            .then(literal("3").executes(|_| Ok(0))),
    );
    d.register(
        literal("i")
            .executes(|_| Ok(0))
            .then(literal("1").executes(|_| Ok(0)))
            .then(literal("2").executes(|_| Ok(0))),
    );
    d.register(literal("j").redirect(d.root()));
    d.register(literal("k").redirect(d.find_node(&["h"]).unwrap()));
    d
}

#[test]
fn all_usage_no_commands() {
    let d = CommandDispatcher::<()>::new();
    assert!(d.get_all_usage(d.root(), &(), true).is_empty());
}
#[test]
fn smart_usage_no_commands() {
    let d = CommandDispatcher::<()>::new();
    assert!(d.get_smart_usage(d.root(), &()).is_empty());
}
#[test]
fn all_usage_root() {
    let d = setup();
    assert_eq!(
        d.get_all_usage(d.root(), &(), true),
        [
            "a 1 i", "a 1 ii", "a 2 i", "a 2 ii", "b 1", "c", "e", "e 1", "e 1 i", "e 1 ii",
            "f 1 i", "f 2 ii", "g", "g 1 i", "h", "h 1 i", "h 2 i ii", "h 3", "i", "i 1", "i 2",
            "j ...", "k -> h",
        ]
    );
}
#[test]
fn smart_usage_root() {
    let d = setup();
    let node = d.root();
    assert_eq!(
        d.get_smart_usage(node, &()),
        vec![
            (d.find_node(&["a"]).unwrap(), "a (1|2)".to_string()),
            (d.find_node(&["b"]).unwrap(), "b 1".to_string()),
            (d.find_node(&["c"]).unwrap(), "c".to_string()),
            (d.find_node(&["e"]).unwrap(), "e [1]".to_string()),
            (d.find_node(&["f"]).unwrap(), "f (1|2)".to_string()),
            (d.find_node(&["g"]).unwrap(), "g [1]".to_string()),
            (d.find_node(&["h"]).unwrap(), "h [1|2|3]".to_string()),
            (d.find_node(&["i"]).unwrap(), "i [1|2]".to_string()),
            (d.find_node(&["j"]).unwrap(), "j ...".to_string()),
            (d.find_node(&["k"]).unwrap(), "k -> h".to_string())
        ]
    );
}
#[test]
fn smart_usage_h() {
    let d = setup();
    let node = d.find_node(&["h"]).unwrap();
    assert_eq!(
        d.get_smart_usage(node, &()),
        vec![
            (d.find_node(&["h", "1"]).unwrap(), "[1] i".to_string()),
            (d.find_node(&["h", "2"]).unwrap(), "[2] i ii".to_string()),
            (d.find_node(&["h", "3"]).unwrap(), "[3]".to_string())
        ]
    );
}
#[test]
fn smart_usage_offset_h() {
    let d = setup();
    let node = {
        let mut r = StringReader::new("/|/|/h");
        r.set_cursor(5);
        d.parse_reader(r, Arc::new(()))
            .context
            .nodes
            .last()
            .unwrap()
            .node
    };
    assert_eq!(
        d.get_smart_usage(node, &()),
        vec![
            (d.find_node(&["h", "1"]).unwrap(), "[1] i".to_string()),
            (d.find_node(&["h", "2"]).unwrap(), "[2] i ii".to_string()),
            (d.find_node(&["h", "3"]).unwrap(), "[3]".to_string())
        ]
    );
}
