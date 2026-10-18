//! Minimal Commands dispatcher. Execution stays synchronous in the server lane.
use ferraloom_brigadier::{
    ArgumentType, CommandDispatcher, CommandExceptionType, CommandSyntaxException, NodeKind,
    StringArgumentType, StringReader, argument, literal,
};
use ferraloom_game::entity::player::GameType;
use ferraloom_protocol::{
    codec::VarInt,
    play::{self, CommandArgument, CommandNode, CommandNodeKind, EntityArgumentFlags},
};
use ferraloom_util::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct CommandSourceStack {
    pub permission: i32,
    pub position: Vec3,
}
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
}
#[derive(Clone, Copy)]
struct Coordinate {
    value: f64,
    relative: bool,
}
#[derive(Clone, Copy)]
struct Coordinates([Coordinate; 3]);
impl Coordinates {
    fn resolve(self, source: Vec3) -> Vec3 {
        let origin = [source.x, source.y, source.z];
        let [x, y, z] = std::array::from_fn(|i| {
            self.0[i].value + if self.0[i].relative { origin[i] } else { 0. }
        });
        Vec3::new(x, y, z)
    }
}
fn word(reader: &mut StringReader) -> String {
    let text: String = reader
        .remaining()
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    for _ in 0..text.encode_utf16().count() {
        reader.skip();
    }
    text
}
struct Vec3Argument;
fn invalid(message: &str) -> CommandSyntaxException {
    CommandExceptionType::simple(message).create(&[])
}
impl ArgumentType for Vec3Argument {
    type Value = Coordinates;
    fn parse(&self, reader: &mut StringReader) -> Result<Coordinates, CommandSyntaxException> {
        let mut coordinates = [Coordinate {
            value: 0.,
            relative: false,
        }; 3];
        for (index, coordinate) in coordinates.iter_mut().enumerate() {
            if index > 0 {
                if !reader.can_read() || reader.peek() != u16::from(b' ') {
                    return Err(invalid("Incomplete (expected 3 coordinates)"));
                }
                reader.skip();
            }
            let text = word(reader);
            let relative = text.starts_with('~');
            let number = text.strip_prefix('~').unwrap_or(&text);
            let value = if number.is_empty() && relative {
                0.
            } else {
                number
                    .parse::<f64>()
                    .map_err(|_| invalid("Invalid coordinate"))?
            };
            if !value.is_finite() {
                return Err(invalid("Invalid coordinate"));
            }
            *coordinate = Coordinate {
                value: if !relative && index != 1 && !number.contains('.') {
                    value + 0.5
                } else {
                    value
                },
                relative,
            };
        }
        Ok(Coordinates(coordinates))
    }
}
struct EntityArgument;
impl ArgumentType for EntityArgument {
    type Value = String;
    fn parse(&self, reader: &mut StringReader) -> Result<String, CommandSyntaxException> {
        let text = word(reader);
        if text.is_empty() {
            Err(invalid("Expected player"))
        } else {
            Ok(text)
        }
    }
}
struct GameModeArgument;
impl ArgumentType for GameModeArgument {
    type Value = GameType;
    fn parse(&self, reader: &mut StringReader) -> Result<GameType, CommandSyntaxException> {
        match word(reader).as_str() {
            "survival" => Ok(GameType::Survival),
            "creative" => Ok(GameType::Creative),
            "adventure" => Ok(GameType::Adventure),
            "spectator" => Ok(GameType::Spectator),
            _ => Err(invalid("Unknown game mode")),
        }
    }
}
struct MessageArgument;
impl ArgumentType for MessageArgument {
    type Value = String;
    fn parse(&self, reader: &mut StringReader) -> Result<String, CommandSyntaxException> {
        StringArgumentType::greedy_string().parse(reader)
    }
}

pub struct Commands {
    dispatcher: CommandDispatcher<CommandSourceStack>,
}
impl Default for Commands {
    fn default() -> Self {
        let mut dispatcher = CommandDispatcher::<CommandSourceStack>::new();
        dispatcher.register(
            literal("stop")
                .requires(|s: &CommandSourceStack| s.permission >= 4)
                .executes(|_| Ok(1)),
        );
        dispatcher.register(
            literal("save-all")
                .requires(|s: &CommandSourceStack| s.permission >= 4)
                .executes(|_| Ok(2))
                .then(literal("flush").executes(|_| Ok(3))),
        );
        dispatcher.register(
            literal("say")
                .requires(|s: &CommandSourceStack| s.permission >= 2)
                .then(argument("message", MessageArgument).executes(|_| Ok(4))),
        );
        let teleport = dispatcher.register(
            literal("teleport")
                .requires(|s: &CommandSourceStack| s.permission >= 2)
                .then(argument("location", Vec3Argument).executes(|_| Ok(5)))
                .then(
                    argument("targets", EntityArgument)
                        .then(argument("location", Vec3Argument).executes(|_| Ok(5))),
                ),
        );
        dispatcher.register(
            literal("tp")
                .requires(|s: &CommandSourceStack| s.permission >= 2)
                .redirect(teleport),
        );
        dispatcher.register(
            literal("gamemode")
                .requires(|s: &CommandSourceStack| s.permission >= 2)
                .then(
                    argument("gamemode", GameModeArgument)
                        .executes(|_| Ok(6))
                        .then(argument("target", EntityArgument).executes(|_| Ok(6))),
                ),
        );
        Self { dispatcher }
    }
}
impl Commands {
    pub fn parse(
        &self,
        text: &str,
        source: CommandSourceStack,
    ) -> Result<CommandAction, CommandSyntaxException> {
        let parse = self.dispatcher.parse(text, source);
        let code = self.dispatcher.execute_parsed(&parse)?;
        let context = parse.context.last_child();
        Ok(match code {
            1 => CommandAction::Stop,
            2 | 3 => CommandAction::Save { flush: code == 3 },
            4 => CommandAction::Say(context.get_argument::<String>("message").unwrap().clone()),
            5 => CommandAction::Teleport {
                targets: context.get_argument::<String>("targets").cloned(),
                position: context
                    .get_argument::<Coordinates>("location")
                    .unwrap()
                    .resolve(source.position),
            },
            6 => CommandAction::GameMode {
                targets: context.get_argument::<String>("target").cloned(),
                mode: *context.get_argument::<GameType>("gamemode").unwrap(),
            },
            _ => unreachable!(),
        })
    }
    /// Port of Commands#sendCommands, deriving network nodes from the dispatcher.
    pub fn packet(&self, permission: i32) -> play::clientbound::Packet {
        let source = CommandSourceStack {
            permission,
            position: Vec3::ZERO,
        };
        let nodes = self
            .dispatcher
            .nodes()
            .map(|(_, node)| {
                let kind = match node.kind() {
                    NodeKind::Root => CommandNodeKind::Root,
                    NodeKind::Literal => CommandNodeKind::Literal(node.name().into()),
                    NodeKind::Argument => CommandNodeKind::Argument {
                        name: node.name().into(),
                        argument: if node.argument_type::<Vec3Argument>().is_some() {
                            CommandArgument::Vec3 {}
                        } else if node.argument_type::<EntityArgument>().is_some() {
                            CommandArgument::Entity {
                                flags: EntityArgumentFlags {
                                    single: false,
                                    players_only: true,
                                },
                            }
                        } else if node.argument_type::<GameModeArgument>().is_some() {
                            CommandArgument::Gamemode {}
                        } else {
                            CommandArgument::Message {}
                        },
                        suggestions: None,
                    },
                };
                CommandNode {
                    executable: node.is_executable(),
                    restricted: !node.can_use(&source),
                    children: node
                        .children()
                        .iter()
                        .filter(|&&id| self.dispatcher.node(id).can_use(&source))
                        .map(|id| VarInt(id.0 as i32))
                        .collect(),
                    redirect: node.redirect().map(|id| VarInt(id.0 as i32)),
                    kind,
                }
            })
            .collect();
        play::clientbound::Packet::Commands(play::clientbound::Commands {
            entries: nodes,
            root_index: VarInt(0),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_preserve_permissions_and_relative_positions() {
        let commands = Commands::default();
        let source = CommandSourceStack {
            permission: 4,
            position: Vec3::new(1., 2., 3.),
        };
        assert!(
            matches!(commands.parse("tp ~1 ~ ~-2",source).unwrap(),CommandAction::Teleport{position,..}if position==Vec3::new(2.,2.,1.))
        );
        assert!(
            commands
                .parse(
                    "stop",
                    CommandSourceStack {
                        permission: 0,
                        ..source
                    }
                )
                .is_err()
        );
        assert!(matches!(
            commands.parse("save-all flush", source).unwrap(),
            CommandAction::Save { flush: true }
        ));
    }
}
