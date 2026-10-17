//! Port of MessageArgument#resolveChatMessage and OutgoingChatMessage#create.
use super::source::{CommandSigningContext, SignedArgument};
use ferraloom_util::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutgoingChatKind {
    Disguised,
    Player,
}
impl CommandSigningContext {
    pub fn argument(&self, name: &str) -> Option<&SignedArgument> {
        self.arguments
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, argument)| argument)
    }
    /// An argument may be present without a signature. Only absence takes the system path.
    pub fn outgoing_chat_kind(&self, name: &str) -> OutgoingChatKind {
        if self
            .argument(name)
            .is_some_and(|argument| argument.sender != Uuid::NIL)
        {
            OutgoingChatKind::Player
        } else {
            OutgoingChatKind::Disguised
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn unsigned_player_argument_and_absent_argument_match_java_packet_variants() {
        let expected: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("vanilla/message-variants.json")).unwrap();
        let anonymous = CommandSigningContext::default();
        let unsigned = CommandSigningContext {
            arguments: Arc::from([(
                "message".into(),
                SignedArgument {
                    sender: Uuid::new(1, 2),
                    index: 0,
                    signature: None,
                },
            )]),
        };
        for (context, key) in [
            (&anonymous, "absent_argument"),
            (&unsigned, "present_unsigned_argument"),
        ] {
            assert_eq!(
                format!("{:?}", context.outgoing_chat_kind("message")),
                expected[key]
            );
        }
        assert_eq!(
            unsigned.outgoing_chat_kind("another_argument"),
            OutgoingChatKind::Disguised
        );
    }
}
