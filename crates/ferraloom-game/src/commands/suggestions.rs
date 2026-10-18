//! Server-side completion reads an owned snapshot at the suggestion request's call site.
use super::{
    CommandSourceStack,
    arguments::{
        MinecraftArgument,
        suggestions::{matches_substring, resource_matches, resources},
    },
    exceptions::translated,
    permissions::Permission,
};
use ferraloom_brigadier::{
    ArgumentType, CommandContext, CommandSyntaxException, StringReader, Suggestions,
    SuggestionsBuilder,
};
use ferraloom_protocol::play::CommandArgument;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct SuggestionData {
    pub players: Vec<String>,
    pub operators: Vec<String>,
    pub whitelist: Vec<String>,
    pub banned_players: Vec<String>,
    pub banned_ips: Vec<String>,
    pub op_candidates: Vec<String>,
    pub whitelist_candidates: Vec<String>,
}
fn words<'a>(builder: &mut SuggestionsBuilder, values: impl IntoIterator<Item = &'a str>) {
    let remaining = builder.remaining_lower_case();
    for value in values {
        if matches_substring(&remaining, &value.to_lowercase()) {
            builder.suggest(value);
        }
    }
}
pub fn argument(
    argument: &MinecraftArgument,
    context: &CommandContext<CommandSourceStack>,
    data: &SuggestionData,
    mut builder: SuggestionsBuilder,
) -> Result<Suggestions, CommandSyntaxException> {
    if let Some(provider) = argument.suggestion_provider.as_deref() {
        match provider {
            "minecraft:available_sounds" => resources(
                &mut builder,
                argument.context.entries("minecraft:sound_event"),
                "",
            ),
            "minecraft:summonable_entities" => {
                let remaining = builder.remaining_lower_case();
                for key in argument.context.entries("minecraft:entity_type") {
                    if ferraloom_data::EntityType::from_name(key)
                        .is_some_and(|entity| entity.data().summonable)
                        && resource_matches(&remaining, key)
                    {
                        let description =
                            format!("entity.{}", key.replace(':', ".").replace('/', "."));
                        builder.suggest_with_tooltip(key, Arc::new(translated(&description, &[])));
                    }
                }
            }
            "minecraft:post_effects" => {} // CommandSourceStack#getAvailablePostEffects is empty.
            "minecraft:ask_server" => {
                let input = StringReader::new(&context.input)
                    .slice(context.range.start, context.input.encode_utf16().count());
                let mut path = input.split_whitespace();
                let command = path.next().unwrap_or("");
                let branch = path.next().unwrap_or("");
                match (command, branch) {
                    ("op", _) => words(&mut builder, data.op_candidates.iter().map(String::as_str)),
                    ("deop", _) => words(&mut builder, data.operators.iter().map(String::as_str)),
                    ("pardon", _) => {
                        words(&mut builder, data.banned_players.iter().map(String::as_str))
                    }
                    ("pardon-ip", _) => {
                        words(&mut builder, data.banned_ips.iter().map(String::as_str))
                    }
                    ("whitelist", "add") => words(
                        &mut builder,
                        data.whitelist_candidates.iter().map(String::as_str),
                    ),
                    ("whitelist", "remove") => {
                        words(&mut builder, data.whitelist.iter().map(String::as_str))
                    }
                    ("tick", "rate") => words(&mut builder, ["20"]),
                    ("tick", "step") => words(&mut builder, ["1t", "1s"]),
                    ("tick", "sprint") => words(&mut builder, ["60s", "1d", "3d"]),
                    _ => {}
                }
            }
            _ => {}
        }
        return Ok(builder.build());
    }
    if matches!(
        argument.wire,
        CommandArgument::Entity { .. } | CommandArgument::GameProfile {}
    ) {
        return entity(&context.source, data, builder);
    }
    argument.list_suggestions(context, builder)
}

fn entity(
    source: &CommandSourceStack,
    data: &SuggestionData,
    mut builder: SuggestionsBuilder,
) -> Result<Suggestions, CommandSyntaxException> {
    let remaining = builder.remaining();
    let allow = source
        .permissions
        .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()));
    if remaining.is_empty() || !remaining.starts_with('@') {
        words(&mut builder, data.players.iter().map(String::as_str));
        if !remaining.is_empty() || !allow {
            return Ok(builder.build());
        }
    } else if !allow {
        return Ok(builder.build());
    }
    if remaining.is_empty()
        || remaining.len() == 1
        || !matches!(
            remaining.as_bytes().get(1),
            Some(b'p' | b'a' | b'r' | b's' | b'e' | b'n')
        )
    {
        for (selector, key) in [
            ("@p", "nearestPlayer"),
            ("@a", "allPlayers"),
            ("@r", "randomPlayer"),
            ("@s", "self"),
            ("@e", "allEntities"),
            ("@n", "nearestEntity"),
        ] {
            builder.suggest_with_tooltip(
                selector,
                Arc::new(translated(&format!("argument.entity.selector.{key}"), &[])),
            );
        }
        return Ok(builder.build());
    }
    if remaining.as_bytes().get(2) != Some(&b'[') {
        return Ok(builder
            .create_offset(builder.start + 2)
            .suggest("[")
            .build());
    }
    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Commands, exceptions::TranslatedMessage};
    use ferraloom_protocol::{chat::Component, play::EntityArgumentFlags};
    use ferraloom_util::Vec3;

    #[derive(serde::Deserialize)]
    struct Vector {
        kind: String,
        input: String,
        level: i32,
        start: usize,
        end: usize,
        suggestions: serde_json::Value,
    }
    #[test]
    fn contextual_ranges_text_and_tooltips_match_java() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/context-suggestion-vectors.json")).unwrap();
        let commands = Commands::default();
        let data = SuggestionData {
            players: vec!["Alex".into(), "Alice".into(), "Doc".into()],
            ..Default::default()
        };
        for vector in vectors {
            let source = CommandSourceStack::server(vector.level, Vec3::ZERO);
            let actual = if vector.kind == "command" {
                commands.suggestions_with(&vector.input, source, &data)
            } else {
                let wire = if vector.kind == "entity" {
                    CommandArgument::Entity {
                        flags: EntityArgumentFlags {
                            single: false,
                            players_only: false,
                        },
                    }
                } else {
                    CommandArgument::GameProfile {}
                };
                let argument_type = MinecraftArgument::new(wire);
                let context = CommandContext::new(
                    Arc::new(source),
                    &vector.input,
                    ferraloom_brigadier::NodeId(0),
                    0,
                );
                argument(
                    &argument_type,
                    &context,
                    &data,
                    SuggestionsBuilder::new(&vector.input, 0),
                )
                .unwrap()
            };
            let entries = actual
                .list
                .iter()
                .map(|suggestion| {
                    let mut entry = serde_json::json!({"text":suggestion.text});
                    if let Some(tooltip) = &suggestion.tooltip {
                        let component = (tooltip.as_ref() as &dyn std::any::Any)
                            .downcast_ref::<TranslatedMessage>()
                            .map_or_else(
                                || Component::literal(tooltip.text()),
                                |message| message.component.clone(),
                            );
                        entry["tooltip"] =
                            serde_json::from_str(&component.to_json().unwrap()).unwrap();
                    }
                    entry
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual.range.start, vector.start,
                "{} {:?}",
                vector.kind, vector.input
            );
            assert_eq!(
                actual.range.end, vector.end,
                "{} {:?}",
                vector.kind, vector.input
            );
            assert_eq!(
                serde_json::json!(entries),
                vector.suggestions,
                "{} {:?}, level {}",
                vector.kind,
                vector.input,
                vector.level
            );
        }
    }
}
