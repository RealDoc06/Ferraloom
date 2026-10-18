//! Port of TextComponentTagVisitor with NbtContents' empty indentation.
use ferraloom_nbt::Tag;
use ferraloom_protocol::chat::{Component, TextColor};

pub(super) fn render(tag: &Tag, plain: bool) -> Component {
    let mut visitor = Visitor {
        result: Component::default(),
        plain,
    };
    visitor.visit(tag, 0);
    visitor.result
}
struct Visitor {
    result: Component,
    plain: bool,
}
impl Visitor {
    fn text(&mut self, text: impl Into<String>, color: Option<&str>) {
        let mut component = Component::literal(text);
        if !self.plain
            && let Some(color) = color
        {
            component.style.color = Some(TextColor::parse(color).unwrap());
        }
        self.result.extra.push(component);
    }
    fn token(&mut self, text: &str) {
        self.text(
            text,
            match text {
                "<...>" => Some("gray"),
                "b" | "B" | "s" | "I" | "L" | "f" | "d" => Some("red"),
                _ => None,
            },
        );
    }
    fn numeric(&mut self, tag: &Tag) {
        let mut text = tag.to_string();
        if matches!(tag, Tag::Int(_)) {
            self.text(text, Some("gold"));
        } else {
            let suffix = text.pop().unwrap();
            self.text(text, Some("gold"));
            self.token(&suffix.to_string());
        }
    }
    fn array(&mut self, prefix: &str, elements: impl ExactSizeIterator<Item = Tag>) {
        self.token("[");
        self.token(prefix);
        self.token(";");
        let count = elements.len();
        for (index, element) in elements.take(128).enumerate() {
            self.text(" ", None);
            self.numeric(&element);
            if index != count - 1 {
                self.token(",");
            }
        }
        if count > 128 {
            self.token("<...>");
        }
        self.token("]");
    }
    fn visit(&mut self, tag: &Tag, depth: usize) {
        match tag {
            Tag::End => {}
            Tag::Byte(_)
            | Tag::Short(_)
            | Tag::Int(_)
            | Tag::Long(_)
            | Tag::Float(_)
            | Tag::Double(_) => self.numeric(tag),
            Tag::String(value) => {
                let quoted = ferraloom_nbt::quote(value);
                let quote = &quoted[..1];
                self.text(quote, None);
                self.text(&quoted[1..quoted.len() - 1], Some("green"));
                self.text(quote, None);
            }
            Tag::ByteArray(values) => self.array("B", values.iter().copied().map(Tag::Byte)),
            Tag::IntArray(values) => self.array("I", values.iter().copied().map(Tag::Int)),
            Tag::LongArray(values) => self.array("L", values.iter().copied().map(Tag::Long)),
            Tag::List(values) => {
                self.token("[");
                if !values.0.is_empty() {
                    if depth >= 64 {
                        self.token("<...>");
                    } else {
                        let wrap = values.0.len() < 8
                            && values.0.iter().any(|tag| {
                                !matches!(
                                    tag,
                                    Tag::Byte(_)
                                        | Tag::Short(_)
                                        | Tag::Int(_)
                                        | Tag::Long(_)
                                        | Tag::Float(_)
                                        | Tag::Double(_)
                                )
                            });
                        for (index, value) in values.0.iter().enumerate() {
                            if index != 0 {
                                self.token(",");
                                self.text(" ", None);
                            }
                            // The native wrapped branch appends even an empty
                            // indentation component before every element.
                            if wrap {
                                self.text("", None);
                            }
                            self.visit(value, depth + 1);
                        }
                    }
                }
                self.token("]");
            }
            Tag::Compound(values) => {
                self.token("{");
                if !values.is_empty() {
                    if depth >= 64 {
                        self.token("<...>");
                    } else {
                        for (index, (key, value)) in values.iter_java_order().enumerate() {
                            if index != 0 {
                                self.token(",");
                                self.text(" ", None);
                            }
                            self.text("", None);
                            let key_text = key.to_string_lossy();
                            let simple = !key_text.is_empty()
                                && key_text.bytes().all(|byte| {
                                    byte.is_ascii_alphanumeric()
                                        || matches!(byte, b'.' | b'_' | b'+' | b'-')
                                });
                            let mut component;
                            if simple {
                                component = Component::literal(key_text);
                                if !self.plain {
                                    component.style.color = Some(TextColor::parse("aqua").unwrap());
                                }
                            } else {
                                let quoted = ferraloom_nbt::quote(key);
                                let quote = &quoted[..1];
                                component = Component::literal(quote);
                                let mut inner = Component::literal(&quoted[1..quoted.len() - 1]);
                                if !self.plain {
                                    inner.style.color = Some(TextColor::parse("aqua").unwrap());
                                }
                                component.extra.push(inner);
                                component.extra.push(Component::literal(quote));
                            }
                            self.result.extra.push(component);
                            self.token(":");
                            self.text(" ", None);
                            self.visit(value, depth + 1);
                        }
                    }
                }
                self.token("}");
            }
        }
    }
}
