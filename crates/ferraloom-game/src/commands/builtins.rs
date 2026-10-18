//! Command behavior without socket or scheduler dependencies.
use super::{PreparedCommand, arguments::ArgumentValue, exceptions::error};
use crate::gamerules::{GameRule, GameRuleChange, GameRuleValue, GameRules};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::chat::{Argument, ClickEvent, Component, HoverEvent, Numeric, TextColor};
#[derive(Clone, Debug)]
pub struct CommandResult {
    pub value: i32,
    pub feedback: Vec<Feedback>,
    pub rule_change: Option<GameRuleChange>,
}
#[derive(Clone, Debug)]
pub struct Feedback {
    pub message: Component,
    pub broadcast: bool,
}
impl CommandResult {
    pub fn success(value: i32, message: Component, broadcast: bool) -> Self {
        Self {
            value,
            feedback: vec![Feedback { message, broadcast }],
            rule_change: None,
        }
    }
}
pub fn string(value: impl Into<String>) -> Argument {
    Argument::String(value.into())
}
pub fn integer(value: i32) -> Argument {
    Argument::Number(Numeric::Int(value))
}
pub fn component(value: Component) -> Argument {
    Argument::Component(Box::new(value))
}
/// Port of GameRuleCommand#queryRule and #setRule, including no-op rejection.
pub fn gamerule(
    rules: &mut GameRules,
    command: &PreparedCommand,
) -> Result<CommandResult, CommandSyntaxException> {
    let name = command.literals.get(1).expect("parsed rule literal");
    let rule = GameRule::by_id(name).expect("registered rule");
    let current = rules
        .get(rule)
        .ok_or_else(|| error("commands.gamerule.not_set", &[name.clone(), "".into()]))?;
    let value = match command.argument("value") {
        Some(ArgumentValue::Boolean(value)) => Some(GameRuleValue::Boolean(*value)),
        Some(ArgumentValue::Integer(value)) => Some(GameRuleValue::Integer(*value)),
        _ => None,
    };
    let short = rule.definition().id.strip_prefix("minecraft:").unwrap();
    if let Some(value) = value {
        if current == value {
            return Err(error(
                "commands.gamerule.not_set",
                &[short.into(), value.to_string()],
            ));
        }
        let change = rules.set(rule, value).map_err(|_| {
            error(
                "commands.gamerule.not_set",
                &[short.into(), value.to_string()],
            )
        })?;
        let mut result = CommandResult::success(
            value.command_result(),
            Component::translatable(
                "commands.gamerule.set",
                vec![string(short), string(value.to_string())],
            ),
            true,
        );
        result.rule_change = Some(change);
        Ok(result)
    } else {
        Ok(CommandResult::success(
            current.command_result(),
            Component::translatable(
                "commands.gamerule.query",
                vec![string(short), string(current.to_string())],
            ),
            false,
        ))
    }
}
/// Port of SeedCommand and ComponentUtils#copyOnClickText.
pub fn seed(seed: i64) -> CommandResult {
    let text = seed.to_string();
    let mut copy = Component::literal(&text);
    copy.style.color = Some(TextColor::parse("green").unwrap());
    copy.style.click_event = Some(ClickEvent::CopyToClipboard(text.clone()));
    copy.style.hover_event = Some(HoverEvent::ShowText(Box::new(Component::translatable(
        "chat.copy.click",
        vec![],
    ))));
    copy.style.insertion = Some(text);
    let brackets = Component::translatable("chat.square_brackets", vec![component(copy)]);
    CommandResult::success(
        seed as i32,
        Component::translatable("commands.seed.success", vec![component(brackets)]),
        false,
    )
}
/// Port of ComponentUtils#formatList(Collection, formatter), preserving insertion order.
pub fn format_list(values: impl IntoIterator<Item = Component>) -> Component {
    let mut values = values.into_iter().collect::<Vec<_>>();
    if values.len() == 1 {
        return values.remove(0);
    }
    let mut result = Component::literal("");
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            let mut separator = Component::literal(", ");
            separator.style.color = Some(TextColor::parse("gray").unwrap());
            result.extra.push(separator);
        }
        result.extra.push(value);
    }
    result
}
pub fn list_players(players: Vec<Component>, max_players: i32) -> CommandResult {
    let count = players.len() as i32;
    CommandResult::success(
        count,
        Component::translatable(
            "commands.list.players",
            vec![
                integer(count),
                integer(max_players),
                component(format_list(players)),
            ],
        ),
        false,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::Vec3;
    #[test]
    fn gamerule_no_op_is_an_error_and_changes_return_the_vanilla_result() {
        let commands = Commands::default();
        let mut rules = GameRules::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        let unchanged = commands
            .prepare("gamerule keep_inventory false", source.clone())
            .unwrap();
        assert!(gamerule(&mut rules, &unchanged).is_err());
        let changed = commands
            .prepare("gamerule minecraft:keep_inventory true", source)
            .unwrap();
        let result = gamerule(&mut rules, &changed).unwrap();
        assert_eq!(result.value, 1);
        assert!(result.rule_change.is_some());
        assert!(result.feedback[0].broadcast);
    }
}
