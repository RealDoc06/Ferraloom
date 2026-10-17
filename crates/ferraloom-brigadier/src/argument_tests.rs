// Ported from Brigadier 1.3.11 argument type tests (MIT).
use super::*;
mod integer {
    use super::*;
    #[test]
    fn parse() {
        let mut r = StringReader::new("15");
        assert_eq!(
            ArgumentType::parse(&IntegerArgumentType::default(), &mut r).unwrap(),
            15
        );
        assert!(!r.can_read());
    }
    #[test]
    fn parse_too_small() {
        let mut r = StringReader::new("-5");
        let e = ArgumentType::parse(&IntegerArgumentType::new(0, 100), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooLow("Integer"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn parse_too_big() {
        let mut r = StringReader::new("5");
        let e = ArgumentType::parse(&IntegerArgumentType::new(-100, 0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooHigh("Integer"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn equals() {
        let groups = [
            IntegerArgumentType::default(),
            IntegerArgumentType::new(-100, 100),
            IntegerArgumentType::new(-100, 50),
            IntegerArgumentType::new(-50, 100),
        ];
        for (i, a) in groups.iter().enumerate() {
            for (j, b) in groups.iter().enumerate() {
                assert_eq!(a == b, i == j);
            }
        }
    }
    #[test]
    fn display() {
        assert_eq!(IntegerArgumentType::default().to_string(), "integer()");
        assert_eq!(IntegerArgumentType::min(-100).to_string(), "integer(-100)");
        assert_eq!(
            IntegerArgumentType::new(-100, 100).to_string(),
            "integer(-100, 100)"
        );
        assert_eq!(
            IntegerArgumentType::new(IntegerArgumentType::default().minimum, 100).to_string(),
            "integer(-2147483648, 100)"
        );
    }
}
mod long {
    use super::*;
    #[test]
    fn parse() {
        let mut r = StringReader::new("15");
        assert_eq!(
            ArgumentType::parse(&LongArgumentType::default(), &mut r).unwrap(),
            15
        );
        assert!(!r.can_read());
    }
    #[test]
    fn parse_too_small() {
        let mut r = StringReader::new("-5");
        let e = ArgumentType::parse(&LongArgumentType::new(0, 100), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooLow("Long"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn parse_too_big() {
        let mut r = StringReader::new("5");
        let e = ArgumentType::parse(&LongArgumentType::new(-100, 0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooHigh("Long"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn equals() {
        let groups = [
            LongArgumentType::default(),
            LongArgumentType::new(-100, 100),
            LongArgumentType::new(-100, 50),
            LongArgumentType::new(-50, 100),
        ];
        for (i, a) in groups.iter().enumerate() {
            for (j, b) in groups.iter().enumerate() {
                assert_eq!(a == b, i == j);
            }
        }
    }
    #[test]
    fn display() {
        assert_eq!(LongArgumentType::default().to_string(), "long()");
        assert_eq!(LongArgumentType::min(-100).to_string(), "long(-100)");
        assert_eq!(
            LongArgumentType::new(-100, 100).to_string(),
            "long(-100, 100)"
        );
        assert_eq!(
            LongArgumentType::new(LongArgumentType::default().minimum, 100).to_string(),
            "long(-9223372036854775808, 100)"
        );
    }
}
mod float {
    use super::*;
    #[test]
    fn parse() {
        let mut r = StringReader::new("15");
        assert_eq!(
            ArgumentType::parse(&FloatArgumentType::default(), &mut r).unwrap(),
            15.0
        );
        assert!(!r.can_read());
    }
    #[test]
    fn parse_too_small() {
        let mut r = StringReader::new("-5");
        let e = ArgumentType::parse(&FloatArgumentType::new(0.0, 100.0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooLow("Float"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn parse_too_big() {
        let mut r = StringReader::new("5");
        let e = ArgumentType::parse(&FloatArgumentType::new(-100.0, 0.0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooHigh("Float"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn equals() {
        let groups = [
            FloatArgumentType::default(),
            FloatArgumentType::new(-100.0, 100.0),
            FloatArgumentType::new(-100.0, 50.0),
            FloatArgumentType::new(-50.0, 100.0),
        ];
        for (i, a) in groups.iter().enumerate() {
            for (j, b) in groups.iter().enumerate() {
                assert_eq!(a == b, i == j);
            }
        }
    }
    #[test]
    fn display() {
        assert_eq!(FloatArgumentType::default().to_string(), "float()");
        assert_eq!(FloatArgumentType::min(-100.0).to_string(), "float(-100.0)");
        assert_eq!(
            FloatArgumentType::new(-100.0, 100.0).to_string(),
            "float(-100.0, 100.0)"
        );
        assert_eq!(
            FloatArgumentType::new(FloatArgumentType::default().minimum, 100.0).to_string(),
            "float(-3.4028235E38, 100.0)"
        );
    }
}
mod double {
    use super::*;
    #[test]
    fn parse() {
        let mut r = StringReader::new("15");
        assert_eq!(
            ArgumentType::parse(&DoubleArgumentType::default(), &mut r).unwrap(),
            15.0
        );
        assert!(!r.can_read());
    }
    #[test]
    fn parse_too_small() {
        let mut r = StringReader::new("-5");
        let e = ArgumentType::parse(&DoubleArgumentType::new(0.0, 100.0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooLow("Double"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn parse_too_big() {
        let mut r = StringReader::new("5");
        let e = ArgumentType::parse(&DoubleArgumentType::new(-100.0, 0.0), &mut r).unwrap_err();
        assert_eq!(e.kind, ExceptionKind::TooHigh("Double"));
        assert_eq!(e.cursor, Some(0));
    }
    #[test]
    fn equals() {
        let groups = [
            DoubleArgumentType::default(),
            DoubleArgumentType::new(-100.0, 100.0),
            DoubleArgumentType::new(-100.0, 50.0),
            DoubleArgumentType::new(-50.0, 100.0),
        ];
        for (i, a) in groups.iter().enumerate() {
            for (j, b) in groups.iter().enumerate() {
                assert_eq!(a == b, i == j);
            }
        }
    }
    #[test]
    fn display() {
        assert_eq!(DoubleArgumentType::default().to_string(), "double()");
        assert_eq!(
            DoubleArgumentType::min(-100.0).to_string(),
            "double(-100.0)"
        );
        assert_eq!(
            DoubleArgumentType::new(-100.0, 100.0).to_string(),
            "double(-100.0, 100.0)"
        );
        assert_eq!(
            DoubleArgumentType::new(DoubleArgumentType::default().minimum, 100.0).to_string(),
            "double(-1.7976931348623157E308, 100.0)"
        );
    }
}
#[test]
fn bool_parse() {
    let mut r = StringReader::new("true");
    assert!(ArgumentType::parse(&BoolArgumentType, &mut r).unwrap());
    assert!(!r.can_read());
}
#[test]
fn parse_word() {
    assert_eq!(
        ArgumentType::parse(
            &StringArgumentType::word(),
            &mut StringReader::new("hello world")
        )
        .unwrap(),
        "hello"
    );
}
#[test]
fn parse_string() {
    assert_eq!(
        ArgumentType::parse(
            &StringArgumentType::string(),
            &mut StringReader::new("\"hello world\"")
        )
        .unwrap(),
        "hello world"
    );
}
#[test]
fn parse_greedy_string() {
    let mut r = StringReader::new("Hello world! This is a test.");
    assert_eq!(
        ArgumentType::parse(&StringArgumentType::greedy_string(), &mut r).unwrap(),
        "Hello world! This is a test."
    );
    assert!(!r.can_read());
}
#[test]
fn string_display() {
    assert_eq!(StringArgumentType::string().to_string(), "string()");
}
#[test]
fn escape_not_required() {
    for text in ["hello", ""] {
        assert_eq!(StringArgumentType::escape_if_required(text), text);
    }
}
#[test]
fn escape_multiple_words() {
    assert_eq!(
        StringArgumentType::escape_if_required("hello world"),
        "\"hello world\""
    );
}
#[test]
fn escape_quote() {
    assert_eq!(
        StringArgumentType::escape_if_required("hello \"world\"!"),
        "\"hello \\\"world\\\"!\""
    );
}
#[test]
fn escape_escapes() {
    assert_eq!(StringArgumentType::escape_if_required("\\"), "\"\\\\\"");
}
#[test]
fn escape_single_quote() {
    assert_eq!(StringArgumentType::escape_if_required("\""), "\"\\\"\"");
}
