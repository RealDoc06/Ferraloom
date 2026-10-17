use crate::{MessageRef, StringReader};
use std::cmp::Ordering;

/// Port of StringRange. All offsets count UTF-16 code units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StringRange {
    pub start: usize,
    pub end: usize,
}
impl StringRange {
    pub const fn at(position: usize) -> Self {
        Self {
            start: position,
            end: position,
        }
    }
    pub const fn between(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn encompassing(a: Self, b: Self) -> Self {
        Self::between(a.start.min(b.start), a.end.max(b.end))
    }
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
    pub fn length(self) -> usize {
        self.end - self.start
    }
    pub fn get(self, input: &str) -> String {
        StringReader::new(input).slice(self.start, self.end)
    }
}

/// Port of Suggestion and IntegerSuggestion, retaining numeric ordering and tooltips.
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub range: StringRange,
    pub text: String,
    pub tooltip: Option<MessageRef>,
    pub integer: Option<i32>,
}
impl PartialEq for Suggestion {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range
            && self.text == other.text
            && self.integer == other.integer
            && self.tooltip.as_ref().map(|m| m.text()) == other.tooltip.as_ref().map(|m| m.text())
    }
}
impl Eq for Suggestion {}
impl Suggestion {
    pub fn new(range: StringRange, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
            tooltip: None,
            integer: None,
        }
    }
    pub fn integer(range: StringRange, value: i32) -> Self {
        Self {
            range,
            text: value.to_string(),
            tooltip: None,
            integer: Some(value),
        }
    }
    pub fn with_tooltip(mut self, tooltip: MessageRef) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
    pub fn apply(&self, input: &str) -> String {
        let reader = StringReader::new(input);
        format!(
            "{}{}{}",
            reader.slice(0, self.range.start),
            self.text,
            reader.slice(self.range.end, reader.total_length())
        )
    }
    pub fn expand(&self, command: &str, range: StringRange) -> Self {
        if self.range == range {
            return self.clone();
        }
        let reader = StringReader::new(command);
        let mut result = Self::new(
            range,
            format!(
                "{}{}{}",
                reader.slice(range.start, self.range.start),
                self.text,
                reader.slice(self.range.end, range.end)
            ),
        );
        result.tooltip = self.tooltip.clone();
        result
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        match (self.integer, other.integer) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self.text.encode_utf16().cmp(other.text.encode_utf16()),
        }
    }
    pub fn compare_ignore_case(&self, other: &Self) -> Ordering {
        if self.integer.is_some() {
            return self.compare(other);
        }
        // Java compares individual UTF-16 chars, applying upper then lower casing.
        fn fold(c: u16) -> u16 {
            char::from_u32(u32::from(c))
                .map(|c| {
                    let upper = c.to_uppercase().next().unwrap_or(c);
                    upper.to_lowercase().next().unwrap_or(upper) as u32 as u16
                })
                .unwrap_or(c)
        }
        self.text
            .encode_utf16()
            .map(fold)
            .cmp(other.text.encode_utf16().map(fold))
    }
}

/// Port of Suggestions, including range expansion, deduplication and sorting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub range: StringRange,
    pub list: Vec<Suggestion>,
}
impl Suggestions {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
    pub fn create(command: &str, suggestions: impl IntoIterator<Item = Suggestion>) -> Self {
        let suggestions: Vec<_> = suggestions.into_iter().collect();
        let Some(range) = suggestions
            .iter()
            .map(|s| s.range)
            .reduce(StringRange::encompassing)
        else {
            return Self::default();
        };
        let mut list = Vec::new();
        for suggestion in suggestions {
            let expanded = suggestion.expand(command, range);
            if !list.contains(&expanded) {
                list.push(expanded);
            }
        }
        // Mixed integer/text suggestions do not define a transitive comparator.
        // Use the binary insertion ordering of JDK TimSort's small-list path;
        // Rust's general sort requires a total order and can produce different results.
        for index in 1..list.len() {
            let mut left = 0;
            let mut right = index;
            while left < right {
                let mid = (left + right) / 2;
                if list[index].compare_ignore_case(&list[mid]).is_lt() {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            list[left..=index].rotate_right(1);
        }
        Self { range, list }
    }
    pub fn merge(command: &str, mut input: Vec<Self>) -> Self {
        if input.len() == 1 {
            return input.pop().unwrap();
        }
        Self::create(command, input.into_iter().flat_map(|s| s.list))
    }
}

/// Port of SuggestionsBuilder. Custom argument types can return these synchronously.
#[derive(Clone, Debug)]
pub struct SuggestionsBuilder {
    pub input: String,
    pub start: usize,
    result: Vec<Suggestion>,
}
impl SuggestionsBuilder {
    pub fn new(input: impl Into<String>, start: usize) -> Self {
        Self {
            input: input.into(),
            start,
            result: Vec::new(),
        }
    }
    pub fn remaining(&self) -> String {
        StringReader::new(&self.input).slice(self.start, self.input.encode_utf16().count())
    }
    pub fn remaining_lower_case(&self) -> String {
        self.remaining().to_lowercase()
    }
    pub fn suggest(&mut self, text: impl Into<String>) -> &mut Self {
        let text = text.into();
        if text != self.remaining() {
            self.result.push(Suggestion::new(self.range(), text));
        }
        self
    }
    pub fn suggest_with_tooltip(
        &mut self,
        text: impl Into<String>,
        tooltip: MessageRef,
    ) -> &mut Self {
        let text = text.into();
        if text != self.remaining() {
            self.result
                .push(Suggestion::new(self.range(), text).with_tooltip(tooltip));
        }
        self
    }
    pub fn suggest_integer(&mut self, value: i32) -> &mut Self {
        self.result.push(Suggestion::integer(self.range(), value));
        self
    }
    pub fn suggest_integer_with_tooltip(&mut self, value: i32, tooltip: MessageRef) -> &mut Self {
        self.result
            .push(Suggestion::integer(self.range(), value).with_tooltip(tooltip));
        self
    }
    fn range(&self) -> StringRange {
        StringRange::between(self.start, self.input.encode_utf16().count())
    }
    pub fn add(&mut self, other: Self) -> &mut Self {
        self.result.extend(other.result);
        self
    }
    pub fn create_offset(&self, start: usize) -> Self {
        Self::new(&self.input, start)
    }
    pub fn restart(&self) -> Self {
        self.create_offset(self.start)
    }
    pub fn build(&self) -> Suggestions {
        Suggestions::create(&self.input, self.result.clone())
    }
}
