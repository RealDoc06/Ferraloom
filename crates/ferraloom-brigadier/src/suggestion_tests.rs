// Ported from Brigadier 1.3.11 SuggestionTest (MIT).
use super::*;
#[test]
fn apply_insertation_start() {
    let suggestion = Suggestion::new(StringRange::at(0), "And so I said: ");
    assert_eq!(
        suggestion.apply("Hello world!"),
        "And so I said: Hello world!"
    );
}
#[test]
fn apply_insertation_middle() {
    let suggestion = Suggestion::new(StringRange::at(6), "small ");
    assert_eq!(suggestion.apply("Hello world!"), "Hello small world!");
}
#[test]
fn apply_insertation_end() {
    let suggestion = Suggestion::new(StringRange::at(5), " world!");
    assert_eq!(suggestion.apply("Hello"), "Hello world!");
}
#[test]
fn apply_replacement_start() {
    let suggestion = Suggestion::new(StringRange::between(0, 5), "Goodbye");
    assert_eq!(suggestion.apply("Hello world!"), "Goodbye world!");
}
#[test]
fn apply_replacement_middle() {
    let suggestion = Suggestion::new(StringRange::between(6, 11), "Alex");
    assert_eq!(suggestion.apply("Hello world!"), "Hello Alex!");
}
#[test]
fn apply_replacement_end() {
    let suggestion = Suggestion::new(StringRange::between(6, 12), "Creeper!");
    assert_eq!(suggestion.apply("Hello world!"), "Hello Creeper!");
}
#[test]
fn apply_replacement_everything() {
    let suggestion = Suggestion::new(StringRange::between(0, 12), "Oh dear.");
    assert_eq!(suggestion.apply("Hello world!"), "Oh dear.");
}
#[test]
fn expand_unchanged() {
    let suggestion = Suggestion::new(StringRange::at(1), "oo");
    assert_eq!(suggestion.expand("f", StringRange::at(1)), suggestion);
}
#[test]
fn expand_left() {
    let suggestion = Suggestion::new(StringRange::at(1), "oo");
    assert_eq!(
        suggestion.expand("f", StringRange::between(0, 1)),
        Suggestion::new(StringRange::between(0, 1), "foo")
    );
}
#[test]
fn expand_right() {
    let suggestion = Suggestion::new(StringRange::at(0), "minecraft:");
    assert_eq!(
        suggestion.expand("fish", StringRange::between(0, 4)),
        Suggestion::new(StringRange::between(0, 4), "minecraft:fish")
    );
}
#[test]
fn expand_both() {
    let suggestion = Suggestion::new(StringRange::at(11), "minecraft:");
    assert_eq!(
        suggestion.expand("give Steve fish_block", StringRange::between(5, 21)),
        Suggestion::new(StringRange::between(5, 21), "Steve minecraft:fish_block")
    );
}
#[test]
fn expand_replacement() {
    let suggestion = Suggestion::new(StringRange::between(6, 11), "strangers");
    assert_eq!(
        suggestion.expand("Hello world!", StringRange::between(0, 12)),
        Suggestion::new(StringRange::between(0, 12), "Hello strangers!")
    );
}
fn builder() -> SuggestionsBuilder {
    SuggestionsBuilder::new("Hello w", 6)
}
#[test]
fn suggest_appends() {
    let r = builder().suggest("world!").build();
    assert_eq!(
        r.list,
        [Suggestion::new(StringRange::between(6, 7), "world!")]
    );
    assert_eq!(r.range, StringRange::between(6, 7));
    assert!(!r.is_empty());
}
#[test]
fn suggest_replaces() {
    let r = builder().suggest("everybody").build();
    assert_eq!(
        r.list,
        [Suggestion::new(StringRange::between(6, 7), "everybody")]
    );
    assert_eq!(r.range, StringRange::between(6, 7));
    assert!(!r.is_empty());
}
#[test]
fn suggest_noop() {
    assert!(builder().suggest("w").build().is_empty());
}
#[test]
fn suggest_multiple() {
    let r = builder()
        .suggest("world!")
        .suggest("everybody")
        .suggest("weekend")
        .build();
    assert_eq!(
        r.list,
        ["everybody", "weekend", "world!"].map(|s| Suggestion::new(StringRange::between(6, 7), s))
    );
    assert_eq!(r.range, StringRange::between(6, 7));
    assert!(!r.is_empty());
}
#[test]
fn restart() {
    let mut b = builder();
    b.suggest("won't be included in restart");
    let other = b.restart();
    assert_eq!(other.input, b.input);
    assert_eq!(other.start, b.start);
    assert_eq!(other.remaining(), b.remaining());
    assert!(other.build().is_empty());
}
#[test]
fn sort_alphabetical() {
    let mut b = builder();
    for s in ["2", "4", "6", "8", "30", "32"] {
        b.suggest(s);
    }
    assert_eq!(
        b.build()
            .list
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["2", "30", "32", "4", "6", "8"]
    );
}
#[test]
fn sort_numerical() {
    let mut b = builder();
    for s in [2, 4, 6, 8, 30, 32] {
        b.suggest_integer(s);
    }
    assert_eq!(
        b.build()
            .list
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["2", "4", "6", "8", "30", "32"]
    );
}
#[test]
fn sort_mixed() {
    let mut b = builder();
    for s in ["11", "22", "33", "a", "b", "c"] {
        b.suggest(s);
    }
    for s in [2, 4, 6, 8, 30, 32] {
        b.suggest_integer(s);
    }
    b.suggest("3a").suggest("a3");
    assert_eq!(
        b.build()
            .list
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        [
            "11", "2", "22", "33", "3a", "4", "6", "8", "30", "32", "a", "a3", "b", "c"
        ]
    );
}
#[test]
fn merge_empty() {
    assert!(Suggestions::merge("foo b", vec![]).is_empty());
}
#[test]
fn merge_single() {
    let s = Suggestions {
        range: StringRange::at(5),
        list: vec![Suggestion::new(StringRange::at(5), "ar")],
    };
    assert_eq!(Suggestions::merge("foo b", vec![s.clone()]), s);
}
#[test]
fn merge_multiple() {
    let a = Suggestions {
        range: StringRange::at(5),
        list: ["ar", "az", "Az"]
            .map(|s| Suggestion::new(StringRange::at(5), s))
            .to_vec(),
    };
    let b = Suggestions {
        range: StringRange::between(4, 5),
        list: ["foo", "qux", "apple", "Bar"]
            .map(|s| Suggestion::new(StringRange::between(4, 5), s))
            .to_vec(),
    };
    let r = Suggestions::merge("foo b", vec![a, b]);
    assert_eq!(
        r.list,
        ["apple", "bar", "Bar", "baz", "bAz", "foo", "qux"]
            .map(|s| Suggestion::new(StringRange::between(4, 5), s))
    );
}
