//! Port of CommandSourceStack#sendSuccess, #broadcastToAdmins and #sendFailure.
use super::{CommandSourceStack, source::CommandSource};
use crate::gamerules::{GameRule, GameRules};
use ferraloom_protocol::chat::{Argument, Component, Style, TextColor};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedbackPlan {
    /// Native sendSuccess evaluates its supplier before game rules filter broadcasts.
    pub evaluate_message: bool,
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
        evaluate_message: (accepts && !source.silent) || broadcast,
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
        let source = CommandSourceStack::server(4, Vec3::ZERO).with_source(CommandSource::Player(
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
        ));
        assert_eq!(
            success_plan(&source, &rules, true),
            FeedbackPlan {
                evaluate_message: true,
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
                evaluate_message: true,
                source: false,
                operators: false,
                server_log: true
            }
        );
        assert_eq!(
            success_plan(&source.with_suppressed_output(), &rules, true),
            FeedbackPlan {
                evaluate_message: false,
                source: false,
                operators: false,
                server_log: false
            }
        );
    }
    #[test]
    fn routing_components_and_supplier_evaluation_match_java() {
        use crate::commands::source::NamesProvider;
        #[derive(serde::Deserialize)]
        struct Vector {
            kind: String,
            accepts: bool,
            informs: bool,
            suppress: bool,
            always: bool,
            feedback: bool,
            log: bool,
            broadcast: bool,
            events: serde_json::Value,
            suppliers: usize,
        }
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/feedback-vectors.json")).unwrap();
        for vector in vectors {
            let mut source =
                CommandSourceStack::server(4, Vec3::ZERO).with_source(match vector.kind.as_str() {
                    "server" => CommandSource::Server,
                    "player" => CommandSource::Player(EntityRef::fixture(
                        crate::level::RegionId::FIXTURE,
                        1,
                    )),
                    _ => CommandSource::Other(1),
                });
            let name = Component::literal(if vector.kind == "server" {
                "Server"
            } else {
                "Invoker"
            });
            source.names = NamesProvider::Constant(Box::new(name.clone()));
            source.accepts_success = vector.accepts;
            source.should_inform_admins = vector.informs;
            source.always_accepts = vector.always;
            if vector.suppress {
                source = source.with_suppressed_output();
            }
            let mut rules = GameRules::default();
            rules
                .set(
                    GameRule::SendCommandFeedback,
                    GameRuleValue::Boolean(vector.feedback),
                )
                .unwrap();
            rules
                .set(
                    GameRule::LogAdminCommands,
                    GameRuleValue::Boolean(vector.log),
                )
                .unwrap();
            let plan = success_plan(&source, &rules, vector.broadcast);
            assert_eq!(
                usize::from(plan.evaluate_message),
                vector.suppliers,
                "{}",
                vector.kind
            );
            let mut events = Vec::new();
            let mut record = |channel: &str, message: Component| {
                events.push(serde_json::json!({"channel":channel,"message":serde_json::from_str::<serde_json::Value>(&message.to_json().unwrap()).unwrap()}))
            };
            let message = Component::translatable("commands.save.success", vec![]);
            if plan.source {
                record("source", message.clone());
            }
            let admin = admin_message(name, message);
            if plan.operators {
                record("operator", admin.clone());
            }
            if plan.server_log {
                record("server", admin);
            }
            assert_eq!(serde_json::json!(events), vector.events, "{}", vector.kind);
        }
    }
}
