//! Port of Commands#fillUsableCommands and ClientboundCommandsPacket's breadth-first IDs.
use super::{
    arguments::{MinecraftArgument, wire},
    source::CommandSourceStack,
};
use ferraloom_brigadier::{ArgumentBuilder, CommandDispatcher, NodeId};
use ferraloom_protocol::{
    Identifier,
    codec::VarInt,
    play::{CommandNode, CommandNodeKind, clientbound},
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, OnceLock},
};

/// Vanilla-derived structural data, retaining registration order, not alphabetic report order.
#[derive(Clone, Deserialize)]
pub struct NodeDefinition {
    pub name: String,
    pub kind: String,
    pub executable: bool,
    pub fork: bool,
    pub restricted: bool,
    pub permissions: [bool; 5],
    pub redirect: Option<usize>,
    pub children: Vec<usize>,
    pub parser: Option<String>,
    #[serde(default)]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub examples: Vec<String>,
    pub suggestions: Option<String>,
}
pub fn definitions() -> &'static [NodeDefinition] {
    static DEFINITIONS: OnceLock<Vec<NodeDefinition>> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!("vanilla/tree.json"))
            .expect("vanilla dedicated command definitions")
    })
}
pub fn dispatcher() -> CommandDispatcher<CommandSourceStack> {
    let definitions = definitions();
    let mut dispatcher = CommandDispatcher::new();
    dispatcher.set_exception_provider(Arc::new(super::exceptions::BrigadierExceptions));
    let mut handles = vec![None; definitions.len()];
    handles[0] = Some(dispatcher.root());
    fn build(
        index: usize,
        definitions: &[NodeDefinition],
        handles: &mut [Option<NodeId>],
        dispatcher: &mut CommandDispatcher<CommandSourceStack>,
    ) -> NodeId {
        if let Some(id) = handles[index] {
            return id;
        }
        let definition = &definitions[index];
        let mut builder = if definition.kind == "argument" {
            ArgumentBuilder::argument(
                &definition.name,
                MinecraftArgument {
                    wire: wire::from_report(
                        definition.parser.as_ref().unwrap(),
                        &definition.properties,
                    ),
                    examples: Arc::from(definition.examples.clone()),
                },
            )
        } else {
            ArgumentBuilder::literal(&definition.name)
        };
        let permissions = definition.permissions;
        // The captured predicates are level-monotone; level 0 may still be restricted with NO_PERMISSIONS.
        if let Some(level) = permissions.iter().position(|allowed| *allowed) {
            if level > 0 || definition.restricted {
                builder = builder.requires(move |source: &CommandSourceStack| {
                    source.permissions.has_level(level as i32)
                });
            }
        } else {
            builder = builder.requires(|_| false);
        }
        if definition.executable {
            builder = builder.executes(move |_| Ok(index as i32));
        }
        if definition.suggestions.is_some() {
            builder = builder.suggests(|_, builder| Ok(builder.build()));
        }
        if let Some(redirect) = definition.redirect {
            let target = build(redirect, definitions, handles, dispatcher);
            builder = if definition.fork {
                builder.fork(target, |context| Ok(vec![context.source.clone()]))
            } else {
                builder.redirect(target)
            };
        }
        let id = dispatcher.build(builder);
        handles[index] = Some(id);
        id
    }
    for index in 1..definitions.len() {
        build(index, definitions, &mut handles, &mut dispatcher);
    }
    for (index, definition) in definitions.iter().enumerate() {
        for &child in &definition.children {
            dispatcher.add_child(handles[index].unwrap(), handles[child].unwrap());
        }
    }
    dispatcher
}
/// Network nodes are renumbered after recursively filtering; unreachable nodes never leak.
pub fn packet(source: &CommandSourceStack) -> clientbound::Commands {
    let definitions = definitions();
    let mut allowed = BTreeSet::from([0]);
    fn visit(
        index: usize,
        source: &CommandSourceStack,
        definitions: &[NodeDefinition],
        allowed: &mut BTreeSet<usize>,
    ) {
        for &child in &definitions[index].children {
            let node = &definitions[child];
            let can_use = node.permissions.iter().enumerate().any(|(level, allowed)| {
                *allowed
                    && (level == 0 && !node.restricted
                        || source.permissions.has_level(level as i32))
            });
            if can_use && allowed.insert(child) {
                visit(child, source, definitions, allowed);
            }
        }
    }
    visit(0, source, definitions, &mut allowed);
    let mut queue = VecDeque::from([0]);
    let mut order = Vec::new();
    let mut ids = vec![None; definitions.len()];
    while let Some(index) = queue.pop_front() {
        if ids[index].is_some() {
            continue;
        }
        ids[index] = Some(order.len() as i32);
        order.push(index);
        queue.extend(
            definitions[index]
                .children
                .iter()
                .copied()
                .filter(|child| allowed.contains(child)),
        );
        if let Some(redirect) = definitions[index].redirect.filter(|r| allowed.contains(r)) {
            queue.push_back(redirect);
        }
    }
    let entries = order
        .into_iter()
        .map(|index| {
            let node = &definitions[index];
            let kind = match node.kind.as_str() {
                "root" => CommandNodeKind::Root,
                "literal" => CommandNodeKind::Literal(node.name.clone()),
                _ => CommandNodeKind::Argument {
                    name: node.name.clone(),
                    argument: wire::from_report(node.parser.as_ref().unwrap(), &node.properties),
                    suggestions: node
                        .suggestions
                        .as_ref()
                        .map(|id| Identifier::parse(id).unwrap()),
                },
            };
            CommandNode {
                executable: node.executable,
                restricted: node.restricted,
                children: node
                    .children
                    .iter()
                    .filter_map(|&child| ids[child].map(VarInt))
                    .collect(),
                redirect: node.redirect.and_then(|index| ids[index]).map(VarInt),
                kind,
            }
        })
        .collect();
    clientbound::Commands {
        entries,
        root_index: VarInt(0),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_protocol::Wire;
    use ferraloom_util::Vec3;
    #[test]
    fn command_tree_packets_match_java_for_operator_and_non_operator() {
        for (level, expected) in [
            (0, include_bytes!("vanilla/tree-level-0.bin").as_slice()),
            (4, include_bytes!("vanilla/tree-level-4.bin").as_slice()),
        ] {
            let packet = packet(&CommandSourceStack::server(level, Vec3::ZERO));
            assert_eq!(
                packet.to_bytes().unwrap(),
                expected,
                "permission level {level}"
            );
        }
    }
}
