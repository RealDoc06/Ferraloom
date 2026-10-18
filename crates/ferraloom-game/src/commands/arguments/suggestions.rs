//! Port of SharedSuggestionProvider's substring and resource completion rules.
use ferraloom_brigadier::SuggestionsBuilder;

pub fn matches_substring(pattern: &str, input: &str) -> bool {
    input.starts_with(pattern)
        || input.char_indices().any(|(index, character)| {
            matches!(character, '.' | '_' | '/') && input[index + 1..].starts_with(pattern)
        })
}
pub fn resource_matches(remaining: &str, id: &str) -> bool {
    if remaining.contains(':') {
        return matches_substring(remaining, id);
    }
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    matches_substring(remaining, namespace) || matches_substring(remaining, path)
}
pub fn resources<'a>(
    builder: &mut SuggestionsBuilder,
    values: impl IntoIterator<Item = &'a str>,
    prefix: &str,
) {
    let remaining = builder.remaining_lower_case();
    let filter = if prefix.is_empty() {
        remaining.as_str()
    } else if remaining.is_empty() {
        ""
    } else {
        // Vanilla strips the common prefix, including a partially typed multi-character prefix.
        let common = remaining
            .chars()
            .zip(prefix.chars())
            .take_while(|(a, b)| a == b)
            .map(|(character, _)| character.len_utf8())
            .sum::<usize>();
        if common == 0 {
            return;
        }
        &remaining[common..]
    };
    for id in values {
        if resource_matches(filter, id) {
            builder.suggest(format!("{prefix}{id}"));
        }
    }
}
