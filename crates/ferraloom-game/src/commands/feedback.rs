//! Port of CommandSourceStack#sendSuccess, #broadcastToAdmins and #sendFailure.
use super::{CommandSourceStack, source::CommandSource};
use crate::gamerules::{GameRule, GameRules};
use ferraloom_protocol::chat::{Argument, Component, Style, TextColor};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedbackPlan {
    pub source: bool,
    pub operators: bool,
    pub server_log: bool,
}
pub fn success_plan(
    source: &CommandSourceStack,
    rules: &GameRules,
    broadcast: bool,
) -> FeedbackPlan {
    let feedback = rules.boolean(GameRule::SendCommandFeedback);
    let accepts =
        source.accepts_success && (!matches!(source.source, CommandSource::Player(_)) || feedback);
    let broadcast = broadcast && source.should_inform_admins && !source.silent;
    FeedbackPlan {
        source: accepts && !source.silent,
        operators: broadcast && feedback,
        server_log: broadcast
            && source.source != CommandSource::Server
            && rules.boolean(GameRule::LogAdminCommands),
    }
}
pub fn admin_message(name: Component, message: Component) -> Component {
    let mut component = Component::translatable(
        "chat.type.admin",
        vec![
            Argument::Component(Box::new(name)),
            Argument::Component(Box::new(message)),
        ],
    );
    component.style = Style {
        color: Some(TextColor::parse("gray").unwrap()),
        italic: Some(true),
        ..Style::default()
    };
    component
}
pub fn failure_message(message: Component) -> Component {
    let mut component = Component::literal("");
    component.extra.push(message);
    component.style.color = Some(TextColor::parse("red").unwrap());
    component
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{entity::EntityRef, gamerules::GameRuleValue};
    use ferraloom_util::Vec3;
    #[test]
    fn changing_feedback_rule_takes_effect_before_feedback_is_sent() {
        let mut rules = GameRules::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO)
            .with_source(CommandSource::Player(EntityRef(1)));
        assert_eq!(
            success_plan(&source, &rules, true),
            FeedbackPlan {
                source: true,
                operators: true,
                server_log: true
            }
        );
        rules
            .set(GameRule::SendCommandFeedback, GameRuleValue::Boolean(false))
            .unwrap();
        assert_eq!(
            success_plan(&source, &rules, true),
            FeedbackPlan {
                source: false,
                operators: false,
                server_log: true
            }
        );
        assert_eq!(
            success_plan(&source.with_suppressed_output(), &rules, true),
            FeedbackPlan {
                source: false,
                operators: false,
                server_log: false
            }
        );
    }
}
