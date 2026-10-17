use super::*;
fn hex(text: &str) -> Vec<u8> {
    text.trim()
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn vanilla_network_components_and_nested_styles() {
    for bytes in [
        include_str!("../../tests/fixtures/component_0.hex"),
        include_str!("../../tests/fixtures/component_1.hex"),
        include_str!("../../tests/fixtures/component_2.hex"),
        include_str!("../../tests/fixtures/component_3.hex"),
        include_str!("../../tests/fixtures/component_4.hex"),
        include_str!("../../tests/fixtures/component_5.hex"),
    ] {
        let bytes = hex(bytes);
        let component = Component::from_bytes(&bytes).unwrap();
        assert_eq!(component.to_bytes().unwrap(), bytes, "{component:?}");
    }
}
#[test]
fn vanilla_json_components() {
    for json in [
        include_str!("../../tests/fixtures/component_0.json"),
        include_str!("../../tests/fixtures/component_1.json"),
        include_str!("../../tests/fixtures/component_2.json"),
        include_str!("../../tests/fixtures/component_3.json"),
        include_str!("../../tests/fixtures/component_4.json"),
        include_str!("../../tests/fixtures/component_5.json"),
    ] {
        let component = Component::from_json(json).unwrap();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        let actual: serde_json::Value =
            serde_json::from_str(&component.to_json().unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}
#[test]
fn lenient_syntax_and_strict_discriminators() {
    let component = Component::from_json_lenient("/*comment*/ {text:'hello'; bold:true}").unwrap();
    assert_eq!(component.contents, Contents::Text("hello".into()));
    assert_eq!(component.style.bold, Some(true));
    assert!(Component::from_json("{text:'hello'}").is_err());
    assert!(Component::from_json(r#"{"type":"unknown","text":"hello"}"#).is_err());
    assert!(Component::from_json(r#"{"text":"hello","extra":[]}"#).is_err());
    assert!(
        Component::from_json(
            r#"{"text":"hello","click_event":{"action":"open_file","path":"file"}}"#
        )
        .is_err()
    );
}

#[test]
fn vanilla_standalone_style_codec() {
    for fixture in [
        include_str!("../../tests/fixtures/style_1.hex"),
        include_str!("../../tests/fixtures/style_2.hex"),
    ] {
        let bytes = hex(fixture);
        let style = Style::from_bytes(&bytes).unwrap();
        assert_eq!(style.to_bytes().unwrap(), bytes);
        assert_eq!(Style::from_json(&style.to_json().unwrap()).unwrap(), style);
    }
    assert_eq!(Style::from_json("{}").unwrap(), Style::default());
    assert!(Style::from_json(r#"{"bold":0}"#).is_err());
}
