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
    } else if let Some(filter) = remaining.strip_prefix(prefix) {
        filter
    } else {
        return;
    };
    for id in values {
        if resource_matches(filter, id) {
            builder.suggest(format!("{prefix}{id}"));
        }
    }
}
