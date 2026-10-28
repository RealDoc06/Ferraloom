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
    pub functions: Vec<String>,
    pub function_tags: Vec<String>,
    pub players: Vec<String>,
    pub operators: Vec<String>,
    pub whitelist: Vec<String>,
    pub banned_players: Vec<String>,
    pub banned_ips: Vec<String>,
    pub op_candidates: Vec<String>,
    pub whitelist_candidates: Vec<String>,
    pub default_clocks: std::collections::BTreeMap<String, String>,
    pub clock_markers: std::collections::BTreeMap<String, Vec<String>>,
    pub clock_timelines: std::collections::BTreeMap<String, Vec<String>>,
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
        if provider == "minecraft:ask_server"
            && matches!(argument.wire, CommandArgument::ScoreHolder { .. })
        {
            return entity(argument, &context.source, data, builder);
        }
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
                        let description = format!("entity.{}", key.replace([':', '/'], "."));
                        builder.suggest_with_tooltip(key, Arc::new(translated(&description, &[])));
                    }
                }
            }
            "minecraft:post_effects" => {} // CommandSourceStack#getAvailablePostEffects is empty.
            "minecraft:ask_server" => {
                if matches!(argument.wire, CommandArgument::Function {}) {
                    resources(
                        &mut builder,
                        data.function_tags.iter().map(String::as_str),
                        "#",
                    );
                    resources(&mut builder, data.functions.iter().map(String::as_str), "");
                    return Ok(builder.build());
                }
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
                    ("time", _) => {
                        let clock = context
                            .get_argument::<super::arguments::ArgumentValue>("clock")
                            .and_then(|argument| {
                                if let super::arguments::ArgumentValue::Identifier(id) = argument {
                                    Some(id)
                                } else {
                                    None
                                }
                            })
                            .or_else(|| data.default_clocks.get(&context.source.level.dimension));
                        let clock = clock.ok_or_else(|| {
                            super::exceptions::error(
                                "commands.time.no_default_clock",
                                std::slice::from_ref(&context.source.level.dimension),
                            )
                        })?;
                        let values = match &argument.wire {
                            CommandArgument::ResourceLocation {} => data.clock_markers.get(clock),
                            CommandArgument::Resource { registry }
                                if registry.as_str() == "minecraft:timeline" =>
                            {
                                data.clock_timelines.get(clock)
                            }
                            _ => None,
                        };
                        if let Some(values) = values {
                            resources(&mut builder, values.iter().map(String::as_str), "");
                        }
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
        return entity(argument, &context.source, data, builder);
    }
    argument.list_suggestions(context, builder)
}

fn entity(
    argument: &MinecraftArgument,
    source: &CommandSourceStack,
    data: &SuggestionData,
    builder: SuggestionsBuilder,
) -> Result<Suggestions, CommandSyntaxException> {
    use super::arguments::selectors::{CompletionMode, CompletionState, EntitySelector};
    let start = builder.start;
    let allow = source
        .permissions
        .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()));
    let mut reader = StringReader::new(&builder.input);
    reader.set_cursor(start);
    let mut completion = CompletionState {
        enabled: true,
        ..Default::default()
    };
    let _ = EntitySelector::parse_suggesting(&mut reader, allow, &mut completion);
    let mut builder = builder.create_offset(reader.cursor());
    match completion.mode {
        CompletionMode::NamesOrSelector => {
            words(&mut builder, data.players.iter().map(String::as_str));
            if allow {
                selector_names(&mut builder);
            }
        }
        CompletionMode::Names => {
            builder = builder.create_offset(start);
            words(&mut builder, data.players.iter().map(String::as_str));
        }
        CompletionMode::Selector => {
            builder = builder.create_offset(builder.start.saturating_sub(1));
            selector_names(&mut builder);
        }
        CompletionMode::OpenOptions => {
            builder.suggest("[");
        }
        CompletionMode::OptionKey => {
            let remaining = builder.remaining_lower_case();
            for key in completion.options {
                if key.starts_with(&remaining) {
                    builder.suggest_with_tooltip(
                        format!("{key}="),
                        Arc::new(translated(
                            &format!("argument.entity.options.{key}.description"),
                            &[],
                        )),
                    );
                }
            }
        }
        CompletionMode::NextOrClose => {
            builder.suggest(",");
            builder.suggest("]");
        }
        CompletionMode::Sort => words(&mut builder, ["nearest", "furthest", "random", "arbitrary"]),
        CompletionMode::GameMode { positive, negative } => {
            if positive {
                words(
                    &mut builder,
                    ["survival", "creative", "adventure", "spectator"],
                );
            }
            if negative {
                words(
                    &mut builder,
                    ["!survival", "!creative", "!adventure", "!spectator"],
                );
            }
        }
        CompletionMode::Type {
            positive,
            negative,
            used_tags,
        } => {
            if negative {
                resources(
                    &mut builder,
                    argument.context.entries("minecraft:entity_type"),
                    "!",
                );
            }
            if positive {
                resources(
                    &mut builder,
                    argument.context.entries("minecraft:entity_type"),
                    "",
                );
            }
            if (positive || negative)
                && let Some(tags) = argument.context.tags.get("minecraft:entity_type")
            {
                resources(
                    &mut builder,
                    tags.keys()
                        .filter(|tag| !used_tags.contains(tag))
                        .map(String::as_str),
                    "#",
                );
                resources(
                    &mut builder,
                    tags.keys()
                        .filter(|tag| !used_tags.contains(tag))
                        .map(String::as_str),
                    "!#",
                );
            }
        }
        CompletionMode::None => {}
    }
    Ok(builder.build())
}
fn selector_names(builder: &mut SuggestionsBuilder) {
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
        let mut vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/context-suggestion-vectors.json")).unwrap();
        vectors.extend(
            serde_json::from_str::<Vec<Vector>>(include_str!(
                "vanilla/nbt-suggestion-vectors.json"
            ))
            .unwrap(),
        );
        let commands = Commands::new(crate::commands::context::CommandBuildContext {
            registries: serde_json::from_str(include_str!("vanilla/registry-elements.json"))
                .unwrap(),
            ..Default::default()
        });
        let mut data = SuggestionData {
            players: vec!["Alex".into(), "Alice".into(), "Doc".into()],
            ..Default::default()
        };
        let clocks: serde_json::Value =
            serde_json::from_str(include_str!("vanilla/clock-suggestion-data.json")).unwrap();
        data.default_clocks = std::collections::BTreeMap::from([(
            "minecraft:overworld".into(),
            "minecraft:overworld".into(),
        )]);
        data.clock_markers = serde_json::from_value(clocks["markers"].clone()).unwrap();
        data.clock_timelines = serde_json::from_value(clocks["timelines"].clone()).unwrap();
        for vector in vectors {
            let source = CommandSourceStack::server(vector.level, Vec3::ZERO);
            let actual = if matches!(vector.kind.as_str(), "command" | "time_command") {
                commands.suggestions_with(&vector.input, source, &data)
            } else if vector.kind.starts_with("resource_") {
                let mut builder = SuggestionsBuilder::new(&vector.input, 0);
                resources(
                    &mut builder,
                    ["example:thing", "minecraft:thing", "example:other"],
                    if vector.kind == "resource_prefix" {
                        "!#"
                    } else {
                        ""
                    },
                );
                builder.build()
            } else {
                let wire = if vector.kind == "nbt" {
                    CommandArgument::NbtTag {}
                } else if vector.kind == "block" {
                    CommandArgument::BlockState {}
                } else if vector.kind == "block_predicate" {
                    CommandArgument::BlockPredicate {}
                } else if vector.kind == "item_predicate" {
                    CommandArgument::ItemPredicate {}
                } else if vector.kind == "item" {
                    CommandArgument::ItemStack {}
                } else if vector.kind == "criteria" {
                    CommandArgument::ObjectiveCriteria {}
                } else if vector.kind == "score_holder" {
                    CommandArgument::ScoreHolder {
                        flags: ferraloom_protocol::play::ScoreHolderFlags { multiple: true },
                    }
                } else if vector.kind == "entity" {
                    CommandArgument::Entity {
                        flags: EntityArgumentFlags {
                            single: false,
                            players_only: false,
                        },
                    }
                } else {
                    CommandArgument::GameProfile {}
                };
                let mut argument_type = MinecraftArgument::new(wire);
                Arc::make_mut(&mut argument_type.context).tags.insert(
                    "minecraft:entity_type".into(),
                    serde_json::from_value(clocks["entity_tags"].clone()).unwrap(),
                );
                if vector.kind == "score_holder" {
                    argument_type.suggestion_provider = Some("minecraft:ask_server".into());
                }
                if vector.kind.starts_with("block") {
                    Arc::make_mut(&mut argument_type.context).tags.insert(
                        "minecraft:block".into(),
                        std::collections::BTreeMap::from([(
                            "example:mixed".into(),
                            vec!["minecraft:oak_log".into(), "minecraft:chest".into()],
                        )]),
                    );
                }
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
