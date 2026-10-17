//! Vanilla command parsing and synchronous execution support.
//! Callers enter the server-exclusive lane before selector evaluation or mutation.
pub mod arguments;
pub mod exceptions;
pub mod permissions;
pub mod source;
mod tree;
use crate::entity::player::GameType;
use arguments::{ArgumentValue, coordinates::Coordinates};
use ferraloom_brigadier::{CommandDispatcher, CommandSyntaxException, NodeKind};
use ferraloom_protocol::play;
use ferraloom_util::Vec3;
pub use source::CommandSourceStack;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum CommandAction {
    Stop,
    Save {
        flush: bool,
    },
    Say(String),
    Teleport {
        targets: Option<String>,
        position: Vec3,
    },
    GameMode {
        targets: Option<String>,
        mode: GameType,
    },
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
}
impl PreparedCommand {
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
}
impl Default for Commands {
    fn default() -> Self {
        Self {
            dispatcher: tree::dispatcher(),
        }
    }
}
impl Commands {
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
        self.dispatcher.execute_parsed(&parse)?;
        let mut literals = Vec::new();
        let mut arguments = BTreeMap::new();
        let mut context = &parse.context;
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
        Ok(PreparedCommand {
            source: (*parse.context.source).clone(),
            input: text.into(),
            command,
            literals,
            arguments,
        })
    }
    /// Compatibility bridge for the already-running server while command hosts migrate.
    pub fn parse(
        &self,
        text: &str,
        source: CommandSourceStack,
    ) -> Result<CommandAction, CommandSyntaxException> {
        let prepared = self.prepare(text, source)?;
        let selector_text = |name: &str| -> Option<String> {
            // Preserve the original selector syntax for the legacy server adapter.
            let parse = self
                .dispatcher
                .parse(&prepared.input, prepared.source.clone());
            let mut context = &parse.context;
            loop {
                if let Some(argument) = context.arguments.get(name) {
                    return Some(String::from_utf16_lossy(
                        &prepared.input.encode_utf16().collect::<Vec<_>>()
                            [argument.range.start..argument.range.end],
                    ));
                }
                if let Some(child) = context.child.as_deref() {
                    context = child;
                } else {
                    return None;
                }
            }
        };
        Ok(match prepared.command.as_str() {
            "stop" => CommandAction::Stop,
            "save-all" => CommandAction::Save {
                flush: prepared.literals.iter().any(|s| s == "flush"),
            },
            "say" => CommandAction::Say(match prepared.argument("message") {
                Some(ArgumentValue::Message(message)) => message.text.clone(),
                _ => String::new(),
            }),
            "teleport" if prepared.coordinates("location").is_some() => CommandAction::Teleport {
                targets: selector_text("targets"),
                position: prepared
                    .coordinates("location")
                    .unwrap()
                    .position(&prepared.source, None),
            },
            "gamemode" => CommandAction::GameMode {
                targets: selector_text("target"),
                mode: match prepared.argument("gamemode") {
                    Some(ArgumentValue::GameMode(mode)) => *mode,
                    _ => unreachable!(),
                },
            },
            _ => CommandAction::Run(Box::new(prepared)),
        })
    }
    pub fn packet(&self, permission: i32) -> play::clientbound::Packet {
        play::clientbound::Packet::Commands(tree::packet(&CommandSourceStack::server(
            permission,
            Vec3::ZERO,
        )))
    }
}
