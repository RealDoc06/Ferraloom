//! Port of ComponentPredicateParser's grammar and ItemPredicateArgument's typed tests.
use super::{identifier, items::persistent_component_type, nbt};
use crate::commands::{
    context::CommandBuildContext,
    exceptions::{error, error_at},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::Item;
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    Identifier,
    component::{ComponentPredicate, ComponentTypeId, DataComponent, IntBounds},
};

#[derive(Clone, Debug)]
pub enum ItemFilter {
    Item(Item),
    Tag { name: String, members: Vec<Item> },
}
#[derive(Clone, Debug)]
pub enum ItemTest {
    ComponentExists(ComponentTypeId),
    ComponentEquals(DataComponent),
    Count(IntBounds),
    Predicate(ComponentPredicate),
    Always,
}
#[derive(Clone, Debug)]
pub struct Term {
    pub negated: bool,
    pub test: ItemTest,
}
#[derive(Clone, Debug)]
pub struct ItemPredicate {
    pub item: Option<ItemFilter>,
    /// All groups must match; terms within a group are alternatives.
    pub conditions: Vec<Vec<Term>>,
}
/// Grammar errors retain their furthest rule mark, independently of an exception's cursor.
struct Parser<'a> {
    reader: &'a mut StringReader,
    context: &'a CommandBuildContext,
    best: Option<(usize, CommandSyntaxException)>,
    completion: Option<(usize, Vec<Expected>)>,
}
#[derive(Clone, Copy)]
enum Expected {
    Character(u16),
    Resource(Lookup),
}
impl Parser<'_> {
    fn record(&mut self, mark: usize, error: CommandSyntaxException) {
        if let Some((cursor, expected)) = &mut self.completion {
            if mark > *cursor {
                *cursor = mark;
                expected.clear();
            }
        }
        if self.best.as_ref().is_none_or(|(cursor, _)| mark > *cursor) {
            self.best = Some((mark, error));
        }
    }
    fn expect(&mut self, mark: usize, value: Expected) {
        if let Some((cursor, expected)) = &mut self.completion {
            if mark == *cursor {
                expected.push(value);
            }
        }
    }
    fn character(&mut self, character: u16) -> bool {
        self.reader.skip_whitespace();
        if self.reader.can_read() && self.reader.peek() == character {
            self.reader.skip();
            return true;
        }
        let at = self.reader.cursor();
        self.record(
            at,
            error_at(
                self.reader,
                "argument.literal.incorrect",
                &[char::from_u32(character as u32).unwrap().to_string()],
            ),
        );
        self.expect(at, Expected::Character(character));
        false
    }
    fn lookup(&mut self, kind: Lookup) -> Option<String> {
        self.reader.skip_whitespace();
        let mark = self.reader.cursor();
        let id = match identifier(self.reader) {
            Ok(id) if self.reader.cursor() > mark => id,
            _ => {
                self.reader.set_cursor(mark);
                self.record(mark, error_at(self.reader, "argument.id.invalid", &[]));
                self.expect(mark, Expected::Resource(kind));
                return None;
            }
        };
        let known = match kind {
            Lookup::Item => Item::from_name(&id).is_some(),
            Lookup::Tag => self.context.tag("minecraft:item", &id).is_some(),
            Lookup::Component => {
                id == "minecraft:count" || persistent_component_type(&id).is_some()
            }
            Lookup::Predicate => {
                id == "minecraft:count"
                    || ComponentTypeId::from_name(&id).is_some()
                    || self
                        .context
                        .contains("minecraft:data_component_predicate_type", &id)
            }
        };
        if known {
            return Some(id);
        }
        let key = match kind {
            Lookup::Item => "argument.item.id.invalid",
            Lookup::Tag => "arguments.item.tag.unknown",
            Lookup::Component => "arguments.item.component.unknown",
            Lookup::Predicate => "arguments.item.predicate.unknown",
        };
        self.record(mark, error_at(self.reader, key, &[id]));
        self.expect(mark, Expected::Resource(kind));
        None
    }
    fn item(&mut self) -> Option<Option<ItemFilter>> {
        let start = self.reader.cursor();
        if let Some(id) = self.lookup(Lookup::Item) {
            return Some(Some(ItemFilter::Item(Item::from_name(&id).unwrap())));
        }
        self.reader.set_cursor(start);
        if self.character(b'#' as u16) {
            let name = self.lookup(Lookup::Tag)?;
            let members = self
                .context
                .tag("minecraft:item", &name)
                .unwrap()
                .iter()
                .filter_map(|id| Item::from_name(id))
                .collect();
            return Some(Some(ItemFilter::Tag { name, members }));
        }
        self.reader.set_cursor(start);
        self.character(b'*' as u16).then_some(None)
    }
    fn term(&mut self) -> Option<Term> {
        let start = self.reader.cursor();
        if let Some(test) = self.test() {
            return Some(Term {
                negated: false,
                test,
            });
        }
        self.reader.set_cursor(start);
        if !self.character(b'!' as u16) {
            return None;
        }
        self.test().map(|test| Term {
            negated: true,
            test,
        })
    }
    fn test(&mut self) -> Option<ItemTest> {
        let start = self.reader.cursor();
        if let Some(id) = self.lookup(Lookup::Component) {
            if self.character(b'=' as u16) {
                return self.value(&id, false);
            }
        }
        self.reader.set_cursor(start);
        if let Some(id) = self.lookup(Lookup::Predicate) {
            if self.character(b'~' as u16) {
                return self.value(&id, true);
            }
        }
        self.reader.set_cursor(start);
        self.lookup(Lookup::Component).map(|id| {
            if id == "minecraft:count" {
                ItemTest::Always
            } else {
                ItemTest::ComponentExists(ComponentTypeId::from_name(&id).unwrap())
            }
        })
    }
    fn value(&mut self, id: &str, predicate: bool) -> Option<ItemTest> {
        self.reader.skip_whitespace();
        let mark = self.reader.cursor();
        let tag = match nbt(self.reader, false) {
            Ok(value) => value,
            Err(error) => {
                self.record(mark, error);
                return None;
            }
        };
        let result = if id == "minecraft:count" {
            count(&tag).map(ItemTest::Count)
        } else if predicate
            && !self
                .context
                .contains("minecraft:data_component_predicate_type", id)
        {
            // ItemPredicateArgument falls back to an existence predicate with Unit's map codec.
            if matches!(tag, Tag::Compound(_)) {
                Ok(ItemTest::ComponentExists(
                    ComponentTypeId::from_name(id).unwrap(),
                ))
            } else {
                Err(format!("Not a map: {tag}"))
            }
        } else {
            let key = Identifier::parse(id).unwrap();
            if predicate {
                ComponentPredicate::from_key_nbt(&key, &tag)
                    .map(ItemTest::Predicate)
                    .map_err(|error| error.to_string())
            } else {
                DataComponent::from_nbt(&key, &tag)
                    .map(ItemTest::ComponentEquals)
                    .map_err(|error| error.to_string())
            }
        };
        match result {
            Ok(test) => Some(test),
            Err(message) => {
                let key = if predicate {
                    "arguments.item.predicate.malformed"
                } else {
                    "arguments.item.component.malformed"
                };
                self.record(
                    self.reader.cursor(),
                    error_at(self.reader, key, &[id.into(), message]),
                );
                None
            }
        }
    }
    fn alternatives(&mut self) -> Option<Vec<Term>> {
        let first = self.term()?;
        let end = self.reader.cursor();
        let mut rest = if self.character(b'|' as u16) {
            self.alternatives()
        } else {
            None
        };
        if rest.is_none() {
            self.reader.set_cursor(end)
        }
        let mut terms = rest.take().unwrap_or_default();
        terms.push(first);
        Some(terms)
    }
    fn conditions(&mut self) -> Option<Vec<Vec<Term>>> {
        let first = self.alternatives()?;
        let end = self.reader.cursor();
        let mut rest = if self.character(b',' as u16) {
            self.conditions()
        } else {
            None
        };
        if rest.is_none() {
            self.reader.set_cursor(end)
        }
        let mut groups = rest.take().unwrap_or_default();
        groups.push(first);
        Some(groups)
    }
    fn top(&mut self) -> Option<ItemPredicate> {
        let item = self.item()?;
        let end = self.reader.cursor();
        if !self.character(b'[' as u16) {
            self.reader.set_cursor(end);
            return Some(ItemPredicate {
                item,
                conditions: Vec::new(),
            });
        }
        let start = self.reader.cursor();
        let conditions = match self.conditions() {
            Some(conditions) => conditions,
            None => {
                self.reader.set_cursor(start);
                Vec::new()
            }
        };
        self.character(b']' as u16)
            .then_some(ItemPredicate { item, conditions })
    }
}
#[derive(Clone, Copy)]
enum Lookup {
    Item,
    Tag,
    Component,
    Predicate,
}
fn codec_int(tag: &Tag) -> Option<i32> {
    match tag {
        Tag::Float(value) => Some(*value as i32),
        Tag::Double(value) => Some(*value as i32),
        _ => tag.as_int(),
    }
}
fn count(tag: &Tag) -> Result<IntBounds, String> {
    let bounds = if let Some(value) = codec_int(tag) {
        IntBounds {
            min: Some(value),
            max: Some(value),
        }
    } else if let Tag::Compound(map) = tag {
        let number = |key| {
            map.get(key)
                .map(|v| codec_int(v).ok_or_else(|| "Not a number".to_string()))
                .transpose()
        };
        IntBounds {
            min: number("min")?,
            max: number("max")?,
        }
    } else {
        return Err(format!(
            "Failed to parse either. First: Not a map: {tag}; Second: Not a number"
        ));
    };
    if let (Some(min), Some(max)) = (bounds.min, bounds.max) {
        if min > max {
            return Err(format!(
                "Swapped bounds in range: Optional[{min}] is higher than Optional[{max}]"
            ));
        }
    }
    Ok(bounds)
}
pub fn parse(
    reader: &mut StringReader,
    context: &CommandBuildContext,
) -> Result<ItemPredicate, CommandSyntaxException> {
    let start = reader.cursor();
    let mut parser = Parser {
        reader,
        context,
        best: None,
        completion: None,
    };
    if let Some(value) = parser.top() {
        return Ok(value);
    }
    let failure = parser
        .best
        .map(|(_, error)| error)
        .unwrap_or_else(|| error("argument.id.invalid", &[]));
    parser.reader.set_cursor(start);
    Err(failure)
}

/// Port of Grammar#parseForSuggestions and ComponentPredicateParser's resource rules.
pub fn suggestions(
    context: &CommandBuildContext,
    builder: ferraloom_brigadier::SuggestionsBuilder,
) -> ferraloom_brigadier::Suggestions {
    let mut reader = StringReader::new(&builder.input);
    reader.set_cursor(builder.start);
    let mut parser = Parser {
        reader: &mut reader,
        context,
        best: None,
        completion: Some((builder.start, Vec::new())),
    };
    let _ = parser.top();
    let (cursor, expected) = parser.completion.unwrap();
    let mut builder = builder.create_offset(cursor);
    for value in expected {
        match value {
            Expected::Character(character) => {
                let value = char::from_u32(u32::from(character)).unwrap().to_string();
                if super::suggestions::matches_substring(&builder.remaining_lower_case(), &value) {
                    builder.suggest(value);
                }
            }
            Expected::Resource(kind) => {
                let values: Vec<&str> = match kind {
                    Lookup::Item => context.entries("minecraft:item"),
                    Lookup::Tag => context
                        .tags
                        .get("minecraft:item")
                        .map_or_else(Vec::new, |tags| tags.keys().map(String::as_str).collect()),
                    Lookup::Component => std::iter::once("minecraft:count")
                        .chain(
                            context
                                .entries("minecraft:data_component_type")
                                .into_iter()
                                .filter(|key| persistent_component_type(key).is_some()),
                        )
                        .collect(),
                    Lookup::Predicate => std::iter::once("minecraft:count")
                        .chain(context.entries("minecraft:data_component_predicate_type"))
                        .collect(),
                };
                super::suggestions::resources(&mut builder, values, "");
            }
        }
    }
    builder.build()
}
