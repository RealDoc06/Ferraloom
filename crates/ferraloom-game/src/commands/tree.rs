//! Port of Commands#fillUsableCommands and ClientboundCommandsPacket's breadth-first IDs.
use super::{
    arguments::{MinecraftArgument, wire},
    source::CommandSourceStack,
};
use crate::gamerules::{GameRule, RuleFeatures, RuleType};
use ferraloom_brigadier::{ArgumentBuilder, CommandDispatcher, NodeId};
use ferraloom_protocol::{
    Identifier,
    codec::VarInt,
    play::{CommandNode, CommandNodeKind, clientbound},
};
use serde::Deserialize;
use std::{
    borrow::Cow,
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
        let mut nodes: Vec<NodeDefinition> =
            serde_json::from_str(include_str!("vanilla/tree.json"))
                .expect("vanilla dedicated command definitions");
        let rules = nodes
            .iter()
            .position(|node| node.name == "gamerule" && node.kind == "literal")
            .unwrap();
        let mut children = nodes[rules].children.clone();
        children.sort_by_key(|&index| {
            let name = &nodes[index].name;
            let rule = crate::gamerules::GameRule::by_id(name).expect("vanilla rule literal");
            (rule as usize, name.starts_with("minecraft:"))
        });
        nodes[rules].children = children;
        nodes
    })
}
fn definitions_with_features(features: RuleFeatures) -> Cow<'static, [NodeDefinition]> {
    if !features.minecart_improvements {
        return Cow::Borrowed(definitions());
    }
    let mut nodes = definitions().to_vec();
    let root = nodes
        .iter()
        .position(|n| n.name == "gamerule" && n.kind == "literal")
        .unwrap();
    let rule = GameRule::MaxMinecartSpeed.definition();
    let RuleType::Integer { min, max, .. } = rule.kind else {
        unreachable!()
    };
    for name in [rule.id.strip_prefix("minecraft:").unwrap(), rule.id] {
        let literal = nodes.len();
        nodes.push(NodeDefinition {
            name: name.into(),
            kind: "literal".into(),
            executable: true,
            fork: false,
            restricted: false,
            permissions: [true; 5],
            redirect: None,
            children: vec![literal + 1],
            parser: None,
            properties: serde_json::Value::Null,
            examples: Vec::new(),
            suggestions: None,
        });
        nodes.push(NodeDefinition {
            name: "value".into(),
            kind: "argument".into(),
            executable: true,
            fork: false,
            restricted: false,
            permissions: [true; 5],
            redirect: None,
            children: Vec::new(),
            parser: Some("brigadier:integer".into()),
            properties: serde_json::json!({"min":min,"max":max}),
            examples: vec!["0".into(), "123".into(), "-123".into()],
            suggestions: None,
        });
        nodes[root].children.push(literal);
    }
    let mut children = nodes[root].children.clone();
    children.sort_by_key(|&index| {
        (
            GameRule::by_id(&nodes[index].name).unwrap() as usize,
            nodes[index].name.starts_with("minecraft:"),
        )
    });
    nodes[root].children = children;
    Cow::Owned(nodes)
}
pub fn dispatcher(
    context: Arc<super::context::CommandBuildContext>,
) -> CommandDispatcher<CommandSourceStack> {
    let definitions = definitions_with_features(context.features);
    let mut dispatcher = CommandDispatcher::new();
    dispatcher.set_exception_provider(Arc::new(super::exceptions::BrigadierExceptions));
    let mut handles = vec![None; definitions.len()];
    handles[0] = Some(dispatcher.root());
    fn build(
        index: usize,
        definitions: &[NodeDefinition],
        handles: &mut [Option<NodeId>],
        dispatcher: &mut CommandDispatcher<CommandSourceStack>,
        context: &Arc<super::context::CommandBuildContext>,
    ) -> NodeId {
        if let Some(id) = handles[index] {
            return id;
        }
        let definition = &definitions[index];
        let mut builder = if definition.kind == "argument" {
            ArgumentBuilder::argument(
                &definition.name,
                MinecraftArgument {
                    context: context.clone(),
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
            let target = build(redirect, definitions, handles, dispatcher, context);
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
        build(index, &definitions, &mut handles, &mut dispatcher, &context);
    }
    for (index, definition) in definitions.iter().enumerate() {
        for &child in &definition.children {
            dispatcher.add_child(handles[index].unwrap(), handles[child].unwrap());
        }
    }
    dispatcher
}
/// Network nodes are renumbered after recursively filtering; unreachable nodes never leak.
pub fn packet(source: &CommandSourceStack, features: RuleFeatures) -> clientbound::Commands {
    let definitions = definitions_with_features(features);
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
    visit(0, source, &definitions, &mut allowed);
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
    fn minecart_feature_gates_rule_parsing_and_vanilla_tree() {
        let features = RuleFeatures {
            minecart_improvements: true,
        };
        let commands =
            crate::commands::Commands::new(crate::commands::context::CommandBuildContext {
                features,
                ..Default::default()
            });
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        assert!(
            crate::commands::Commands::default()
                .prepare("gamerule max_minecart_speed", source.clone())
                .is_err()
        );
        for name in ["max_minecart_speed", "minecraft:max_minecart_speed"] {
            assert!(
                commands
                    .prepare(&format!("gamerule {name} 1000"), source.clone())
                    .is_ok()
            );
            assert!(
                commands
                    .prepare(&format!("gamerule {name} 1001"), source.clone())
                    .is_err()
            );
        }
        for (level, bytes) in [
            (
                0,
                include_bytes!("vanilla/tree-minecart-level-0.bin").as_slice(),
            ),
            (
                4,
                include_bytes!("vanilla/tree-minecart-level-4.bin").as_slice(),
            ),
        ] {
            let actual = packet(&CommandSourceStack::server(level, Vec3::ZERO), features)
                .to_bytes()
                .unwrap();
            let expected = clientbound::Commands::from_bytes(bytes).unwrap();
            let expected = if level == 4 {
                normalize_game_rules(expected).to_bytes().unwrap()
            } else {
                bytes.to_vec()
            };
            assert_eq!(actual, expected, "minecart feature, level {level}");
        }
    }
    #[test]
    fn command_tree_packets_match_java_for_operator_and_non_operator() {
        for (level, expected) in [
            (0, include_bytes!("vanilla/tree-level-0.bin").as_slice()),
            (4, include_bytes!("vanilla/tree-level-4.bin").as_slice()),
        ] {
            let packet = packet(
                &CommandSourceStack::server(level, Vec3::ZERO),
                RuleFeatures::default(),
            );
            let expected_packet = clientbound::Commands::from_bytes(expected).unwrap();
            let expected = if level == 4 {
                normalize_game_rules(expected_packet).to_bytes().unwrap()
            } else {
                expected.to_vec()
            };
            assert_eq!(
                packet.to_bytes().unwrap(),
                expected,
                "permission level {level}"
            );
            if level == 4 {
                let second = clientbound::Commands::from_bytes(include_bytes!(
                    "vanilla/tree-level-4-second-jvm.bin"
                ))
                .unwrap();
                assert_eq!(
                    packet.to_bytes().unwrap(),
                    normalize_game_rules(second).to_bytes().unwrap()
                );
            }
        }
    }
}

#[cfg(test)]
fn normalize_game_rules(mut packet: clientbound::Commands) -> clientbound::Commands {
    // Vanilla iterates identity-hashed rule keys. Two fresh JVMs produce different
    // orders; canonicalize ONLY those children and their consequent BFS indices.
    let rules = packet
        .entries
        .iter()
        .position(|node| matches!(&node.kind, CommandNodeKind::Literal(name) if name=="gamerule"))
        .unwrap();
    let mut children = packet.entries[rules].children.clone();
    children.sort_by_key(|index| {
        let CommandNodeKind::Literal(name) = &packet.entries[index.0 as usize].kind else {
            panic!("rule literal")
        };
        (
            crate::gamerules::GameRule::by_id(name).unwrap() as usize,
            name.starts_with("minecraft:"),
        )
    });
    packet.entries[rules].children = children;
    let mut order = Vec::new();
    let mut ids = vec![None; packet.entries.len()];
    let mut queue = VecDeque::from([packet.root_index.0 as usize]);
    while let Some(index) = queue.pop_front() {
        if ids[index].is_some() {
            continue;
        }
        ids[index] = Some(order.len() as i32);
        order.push(index);
        let node = &packet.entries[index];
        queue.extend(node.children.iter().map(|index| index.0 as usize));
        if let Some(redirect) = node.redirect {
            queue.push_back(redirect.0 as usize);
        }
    }
    let entries = order
        .into_iter()
        .map(|index| {
            let mut node = packet.entries[index].clone();
            for child in &mut node.children {
                child.0 = ids[child.0 as usize].unwrap();
            }
            if let Some(redirect) = &mut node.redirect {
                redirect.0 = ids[redirect.0 as usize].unwrap();
            }
            node
        })
        .collect();
    clientbound::Commands {
        entries,
        root_index: VarInt(0),
    }
}
