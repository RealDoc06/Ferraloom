//! Vanilla command parsing and synchronous execution support.
//! Callers enter the server-exclusive lane before selector evaluation or mutation.
pub mod arguments;
pub mod bossbar;
pub mod builtins;
pub mod clocks;
pub mod components;
pub mod context;
pub mod data;
pub mod exceptions;
pub mod execution;
pub mod feedback;
pub mod forceload;
pub mod gamemode;
pub mod give;
pub mod message;
pub mod permissions;
pub mod placement;
pub mod reload;
pub mod scoreboard;
pub mod source;
pub mod spawn;
pub mod storage;
pub mod suggestions;
pub mod team;
pub mod teleport;
pub mod tick;
mod tree;
pub mod trigger;
pub mod weather;
use arguments::{ArgumentValue, coordinates::Coordinates};
use ferraloom_brigadier::{
    CommandContext, CommandDispatcher, CommandSyntaxException, ContextChain, ExceptionKind,
    NodeKind,
};
use ferraloom_protocol::play;
use ferraloom_util::Vec3;
pub use source::CommandSourceStack;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum CommandAction {
    Stop,
    Save { flush: bool },
    Run(Box<PreparedCommand>),
}

/// Parsing stores syntax, never captures entity matches ahead of command execution.
#[derive(Clone, Debug)]
pub struct PreparedCommand {
    pub source: CommandSourceStack,
    pub input: String,
    pub command: String,
    pub literals: Vec<String>,
    pub arguments: BTreeMap<String, ArgumentValue>,
    pub continuation: Option<Box<PreparedCommand>>,
}
impl PreparedCommand {
    pub fn message_argument(&self) -> Option<(&str, &arguments::Message)> {
        self.arguments
            .iter()
            .find_map(|(name, value)| {
                if let ArgumentValue::Message(message) = value {
                    Some((name.as_str(), message))
                } else {
                    None
                }
            })
            .or_else(|| {
                self.continuation
                    .as_deref()
                    .and_then(Self::message_argument)
            })
    }
    pub fn argument(&self, name: &str) -> Option<&ArgumentValue> {
        self.arguments.get(name)
    }
    pub fn string(&self, name: &str) -> Option<&str> {
        match self.argument(name) {
            Some(ArgumentValue::String(value) | ArgumentValue::Identifier(value)) => Some(value),
            _ => None,
        }
    }
    pub fn integer(&self, name: &str) -> Option<i32> {
        match self.argument(name) {
            Some(ArgumentValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }
    pub fn coordinates(&self, name: &str) -> Option<Coordinates> {
        match self.argument(name) {
            Some(ArgumentValue::Coordinates(value)) => Some(*value),
            _ => None,
        }
    }
}
pub struct Commands {
    dispatcher: CommandDispatcher<CommandSourceStack>,
    features: crate::gamerules::RuleFeatures,
}
impl Default for Commands {
    fn default() -> Self {
        Self {
            dispatcher: tree::dispatcher(std::sync::Arc::default()),
            features: Default::default(),
        }
    }
}
impl Commands {
    pub fn new(context: context::CommandBuildContext) -> Self {
        Self {
            features: context.features,
            dispatcher: tree::dispatcher(std::sync::Arc::new(context)),
        }
    }
    pub fn dispatcher(&self) -> &CommandDispatcher<CommandSourceStack> {
        &self.dispatcher
    }
    /// Port of Commands#trimOptionalPrefix: remove exactly one leading slash.
    pub fn trim_optional_prefix(text: &str) -> &str {
        text.strip_prefix('/').unwrap_or(text)
    }
    pub fn prepare(
        &self,
        text: &str,
        source: CommandSourceStack,
    ) -> Result<PreparedCommand, CommandSyntaxException> {
        let text = Self::trim_optional_prefix(text);
        let parse = self.dispatcher.parse(text, source);
        // Commands#finishParsing validates the chain; execution belongs to the live call site.
        if parse.reader.can_read() {
            return Err(if parse.exceptions.len() == 1 {
                parse.exceptions[0].1.clone()
            } else {
                parse.reader.error(
                    if parse.context.range.is_empty() {
                        ExceptionKind::UnknownCommand
                    } else {
                        ExceptionKind::UnknownArgument
                    },
                    &[],
                )
            });
        }
        if ContextChain::try_flatten(&parse.context).is_none() {
            return Err(parse.reader.error(ExceptionKind::UnknownCommand, &[]));
        }
        Ok(self.prepared_context(&parse.context, text))
    }
    fn prepared_context(
        &self,
        root: &CommandContext<CommandSourceStack>,
        text: &str,
    ) -> PreparedCommand {
        let mut literals = Vec::new();
        let mut arguments = BTreeMap::new();
        let mut context = root;
        let mut continuation = None;
        loop {
            for parsed in &context.nodes {
                let node = self.dispatcher.node(parsed.node);
                if node.kind() == NodeKind::Literal {
                    literals.push(node.name().into());
                }
            }
            for name in context.arguments.keys() {
                if let Some(value) = context.get_argument::<ArgumentValue>(name) {
                    arguments.insert(name.clone(), value.clone());
                }
            }
            if let Some(child) = context.child.as_deref() {
                if literals.first().is_some_and(|literal| literal == "return")
                    && child.root_node == self.dispatcher.root()
                {
                    continuation = Some(Box::new(self.prepared_context(child, text)));
                    break;
                }
                context = child;
            } else {
                break;
            }
        }
        let command = match literals.first().map(String::as_str).unwrap_or("") {
            "tp" => "teleport",
            "tell" | "w" => "msg",
            "tm" => "teammsg",
            name => name,
        }
        .to_owned();
        PreparedCommand {
            source: (*root.source).clone(),
            input: text.into(),
            command,
            literals,
            arguments,
            continuation,
        }
    }
    /// Compatibility bridge for the already-running server while command hosts migrate.
    pub fn parse(
        &self,
        text: &str,
        source: CommandSourceStack,
    ) -> Result<CommandAction, CommandSyntaxException> {
        Ok(Self::action(self.prepare(text, source)?))
    }
    pub fn action(prepared: PreparedCommand) -> CommandAction {
        match prepared.command.as_str() {
            "stop" => CommandAction::Stop,
            "save-all" => CommandAction::Save {
                flush: prepared.literals.iter().any(|s| s == "flush"),
            },
            _ => CommandAction::Run(Box::new(prepared)),
        }
    }
    /// Port of HelpCommand, using Brigadier's smart usage and original reader text.
    pub fn help(
        &self,
        command: &PreparedCommand,
    ) -> Result<builtins::CommandResult, CommandSyntaxException> {
        let (node, prefix) = if let Some(input) = command.string("command") {
            let parse = self.dispatcher.parse(input, command.source.clone());
            let node = parse
                .context
                .nodes
                .last()
                .ok_or_else(|| exceptions::error("commands.help.failed", &[]))?
                .node;
            (node, format!("/{input} "))
        } else {
            (self.dispatcher.root(), "/".into())
        };
        let usage = self.dispatcher.get_smart_usage(node, &command.source);
        Ok(builtins::CommandResult {
            value: usage.len() as i32,
            feedback: usage
                .into_iter()
                .map(|(_, line)| builtins::Feedback {
                    message: ferraloom_protocol::chat::Component::literal(format!(
                        "{prefix}{line}"
                    )),
                    broadcast: false,
                })
                .collect(),
            rule_change: None,
        })
    }
    /// Port of ServerCommandSuggestionsProvider's optional slash and 1,000-entry cap.
    pub fn suggestions(
        &self,
        text: &str,
        source: CommandSourceStack,
    ) -> ferraloom_brigadier::Suggestions {
        self.suggestions_with(text, source, &suggestions::SuggestionData::default())
    }
    pub fn suggestions_with(
        &self,
        text: &str,
        source: CommandSourceStack,
        data: &suggestions::SuggestionData,
    ) -> ferraloom_brigadier::Suggestions {
        let mut reader = ferraloom_brigadier::StringReader::with_provider(
            text,
            std::sync::Arc::new(exceptions::BrigadierExceptions),
        );
        if reader.can_read() && reader.peek() == 47 {
            reader.skip();
        }
        let parsed = self
            .dispatcher
            .parse_reader(reader, std::sync::Arc::new(source));
        let cursor = parsed.reader.total_length();
        let point = parsed.context.find_suggestion_context(cursor);
        let mut context = point.context.clone();
        context.input = text.into();
        let mut results = Vec::new();
        for &node in self.dispatcher.node(point.parent).children() {
            let builder =
                ferraloom_brigadier::SuggestionsBuilder::new(text, point.start.min(cursor));
            let result = if let Some(argument) = self
                .dispatcher
                .node(node)
                .argument_type::<arguments::MinecraftArgument>()
            {
                suggestions::argument(argument, &context, data, builder)
            } else {
                self.dispatcher.list_suggestions(node, &context, builder)
            };
            results.push(result.unwrap_or_default());
        }
        let mut suggestions = ferraloom_brigadier::Suggestions::merge(text, results);
        suggestions.list.truncate(1000);
        suggestions
    }
    pub fn packet(&self, permission: i32) -> play::clientbound::Packet {
        play::clientbound::Packet::Commands(tree::packet(
            &CommandSourceStack::server(permission, Vec3::ZERO),
            self.features,
        ))
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;
    #[test]
    fn smart_usage_feedback_and_errors_match_registered_java_command() {
        #[derive(serde::Deserialize)]
        struct Vector {
            input: String,
            level: i32,
            value: Option<i32>,
            error: Option<serde_json::Value>,
            feedback: serde_json::Value,
        }
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/help-vectors.json")).unwrap();
        let commands = Commands::default();
        for vector in vectors {
            let result = commands
                .prepare(
                    &vector.input,
                    CommandSourceStack::server(vector.level, Vec3::ZERO),
                )
                .and_then(|command| commands.help(&command));
            match result {
                Ok(result) => {
                    assert!(vector.error.is_none(), "{}", vector.input);
                    assert_eq!(Some(result.value), vector.value, "{}", vector.input);
                    let feedback = result.feedback.into_iter().map(|feedback| serde_json::json!({
                        "message":serde_json::from_str::<serde_json::Value>(&feedback.message.to_json().unwrap()).unwrap(),
                        "broadcast":feedback.broadcast,
                    })).collect::<Vec<_>>();
                    assert_eq!(
                        serde_json::json!(feedback),
                        vector.feedback,
                        "{}",
                        vector.input
                    );
                }
                Err(error) => {
                    assert_eq!(
                        Some(
                            serde_json::from_str::<serde_json::Value>(
                                &exceptions::error_component(&error).to_json().unwrap()
                            )
                            .unwrap()
                        ),
                        vector.error,
                        "{}",
                        vector.input
                    );
                    assert_eq!(vector.feedback, serde_json::json!([]));
                }
            }
        }
    }
}
